//! `neptune lsp`: a Language Server Protocol server over stdin/stdout.
//!
//! Gives any LSP editor (VS Code, Neovim, Helix, Zed, ...) live errors and
//! warnings as you type, completion of keywords, commands, constants and the
//! program's own functions/types/globals, hover help with command
//! signatures, signature help while typing arguments, an outline (document
//! symbols) and go-to-definition for functions, types and labels.

use std::collections::HashMap;
use std::io::{BufRead, Write};

use neptune_lang::commands::{CommandSig, ConstVal, NativeTy};
use neptune_lang::{compile, Env, SourceLoader};

use crate::json::{n, obj, parse, s, Json};

const KEYWORDS: &[&str] = &[
    "If", "Then", "Else", "ElseIf", "EndIf", "End If", "While", "Wend", "For", "To", "Step", "Next", "Each", "In",
    "Repeat", "Until", "Forever", "Select", "Case", "Default", "End Select", "Function", "End Function", "Return",
    "Type", "Field", "End Type", "Method", "End Method", "Local", "Global", "Const", "Dim", "As", "New", "Delete",
    "First", "Last", "Before", "After", "Insert", "Goto", "Gosub", "Exit", "Continue", "Data", "Read", "Restore",
    "Include", "Import", "Try", "Catch", "End Try", "Throw", "Enum", "End Enum", "Strict", "Classic", "End", "Stop",
    "And", "Or", "Xor", "Not", "Mod", "Shl", "Shr", "Sar", "AndAlso", "OrElse", "True", "False", "Null", "Pi",
    "Self", "Int", "Float", "String", "Variant", "Object", "Handle", "IIf",
];

fn tag(t: &NativeTy) -> &'static str {
    match t {
        NativeTy::Int => "%",
        NativeTy::Float => "#",
        NativeTy::Str => "$",
        NativeTy::Any => "",
    }
}

fn const_text(c: &ConstVal) -> String {
    match c {
        ConstVal::Int(i) => i.to_string(),
        ConstVal::Float(f) => f.to_string(),
        ConstVal::Str(s) => format!("\"{s}\""),
    }
}

/// `PositionEntity(entity%, x#, y#, z#, [global%=0])`
pub fn signature(c: &CommandSig) -> String {
    let params: Vec<String> = c
        .params
        .iter()
        .map(|p| match &p.default {
            Some(d) => format!("[{}{}={}]", p.name, tag(&p.ty), const_text(d)),
            None => format!("{}{}", p.name, tag(&p.ty)),
        })
        .collect();
    let ret = c.ret.as_ref().map(tag).unwrap_or("");
    format!("{}{}({})", c.display, ret, params.join(", "))
}

/// Something the program itself declares.
#[derive(Clone, Debug, PartialEq)]
pub struct Decl {
    pub name: String,
    /// 12 function, 5 class (type), 13 variable, 14 constant, 10 enum, 20 key (label)
    pub kind: u32,
    pub line: usize,
    pub text: String,
}

/// Finds functions, types, globals, constants, enums and labels by line.
pub fn declarations(text: &str) -> Vec<Decl> {
    let mut out = Vec::new();
    for (line, raw) in text.lines().enumerate() {
        let code = raw.split(';').next().unwrap_or("").trim();
        let lower = code.to_ascii_lowercase();
        let word_after = |kw: &str| {
            let rest = code[kw.len()..].trim_start();
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            (!name.is_empty()).then_some(name)
        };
        let mut push = |name: String, kind: u32| out.push(Decl { name, kind, line, text: code.to_string() });
        if lower.starts_with("function ") {
            if let Some(n) = word_after("function") {
                push(n, 12);
            }
        } else if lower.starts_with("type ") {
            if let Some(n) = word_after("type") {
                push(n, 5);
            }
        } else if lower.starts_with("enum ") {
            if let Some(n) = word_after("enum") {
                push(n, 10);
            }
        } else if lower.starts_with("global ") || lower.starts_with("const ") {
            let (kw, kind) = if lower.starts_with("global ") { ("global", 13) } else { ("const", 14) };
            // every name in "Global a = 1, b$ = "x""
            for part in code[kw.len()..].split(',') {
                let name: String = part.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !name.is_empty() && name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
                    push(name, kind);
                }
            }
        } else if let Some(label) = code.strip_prefix('.') {
            let name: String = label.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if !name.is_empty() {
                push(name, 20);
            }
        }
    }
    out
}

/// The identifier under (or just before) a character position.
pub fn word_at(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = col.min(chars.len());
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col.min(chars.len());
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    (start < end).then(|| chars[start..end].iter().collect())
}

/// Name of the command whose arguments the cursor is in, and the argument
/// index: `Foo(a, b|` and `Foo a, b|` both give ("Foo", 1).
pub fn call_at(line: &str, col: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = line.chars().take(col).collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut depth = 0i32;
    let mut commas = 0usize;
    let mut in_str = false;
    let mut i = chars.len();
    while i > 0 {
        i -= 1;
        let c = chars[i];
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        match c {
            ')' => depth += 1,
            '(' if depth > 0 => depth -= 1,
            '(' => {
                // function-style call: the identifier before the parenthesis
                let mut j = i;
                while j > 0 && chars[j - 1] == ' ' {
                    j -= 1;
                }
                let end = j;
                while j > 0 && (is_word(chars[j - 1]) || matches!(chars[j - 1], '%' | '#' | '$')) {
                    j -= 1;
                }
                let name: String = chars[j..end].iter().collect::<String>().trim_end_matches(['%', '#', '$']).to_string();
                return (!name.is_empty()).then_some((name, commas));
            }
            ',' if depth == 0 => commas += 1,
            ':' if depth == 0 => {
                i += 1;
                break;
            }
            _ => {}
        }
    }
    // statement-style call: the first word of the statement
    let rest: String = chars[i..].iter().collect();
    let rest = rest.trim_start();
    let name: String = rest.chars().take_while(|c| is_word(*c)).collect();
    let after = &rest[name.len()..];
    (!name.is_empty() && after.starts_with(' ')).then_some((name, commas))
}

/// 0-based (line, character) of a byte offset in normalised (LF) text.
fn position(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count());
    (line, col)
}

fn uri_to_path(uri: &str) -> String {
    let p = uri.strip_prefix("file://").unwrap_or(uri);
    let mut out = Vec::new();
    let b = p.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&p[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&out).into_owned();
    // file:///C:/x -> C:/x on Windows
    match s.strip_prefix('/') {
        Some(rest) if rest.len() > 2 && rest.as_bytes()[1] == b':' => rest.to_string(),
        _ => s,
    }
}

struct Server {
    env: Env,
    docs: HashMap<String, String>,
}

/// Descriptions of every command (`docs/commands.txt`, also the source of
/// `docs/COMMANDS.md`): `# Category` lines and `Name: description` lines.
const HELP: &str = include_str!("../../../docs/commands.txt");

/// The description of a command; modern names use their command's.
fn help(name: &str) -> Option<&'static str> {
    let find = |n: &str| {
        HELP.lines().filter(|l| !l.starts_with('#') && !l.starts_with(';')).find_map(|l| {
            let (k, text) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(n).then(|| text.trim())
        })
    };
    find(name).or_else(|| neptune_runtime::commands::ALIASES.iter().find(|(a, _)| a.eq_ignore_ascii_case(name)).and_then(|(_, target)| find(target)))
}

impl Server {
    fn diagnostics(&self, uri: &str) -> Json {
        let text = self.docs.get(uri).cloned().unwrap_or_default();
        let path = uri_to_path(uri);
        let normal = text.replace("\r\n", "\n");
        let compiled = compile(&path, &text, &self.env, &mut OpenFiles(self));
        let mut items = Vec::new();
        for d in &compiled.diagnostics.items {
            // spans in included files are reported at the top of this file
            let ((l0, c0), (l1, c1)) = if d.span.file == 0 {
                (position(&normal, d.span.start as usize), position(&normal, d.span.end.max(d.span.start + 1) as usize))
            } else {
                ((0, 0), (0, 1))
            };
            let severity = match d.severity {
                neptune_lang::diag::Severity::Error => 1,
                neptune_lang::diag::Severity::Warning => 2,
            };
            items.push(obj([
                ("range", obj([("start", obj([("line", n(l0 as u32)), ("character", n(c0 as u32))])), ("end", obj([("line", n(l1 as u32)), ("character", n(c1 as u32))]))])),
                ("severity", n(severity)),
                ("source", s("neptune")),
                ("message", s(d.message.clone())),
            ]));
        }
        obj([("uri", s(uri)), ("diagnostics", Json::Arr(items))])
    }

    fn line(&self, params: &Json) -> (String, usize, usize) {
        let uri = params.at("textDocument.uri").str().unwrap_or("");
        let line = params.at("position.line").num().unwrap_or(0.0) as usize;
        let col = params.at("position.character").num().unwrap_or(0.0) as usize;
        let text = self.docs.get(uri).map(String::as_str).unwrap_or("");
        (text.lines().nth(line).unwrap_or("").to_string(), line, col)
    }

    fn command(&self, name: &str) -> Option<&CommandSig> {
        let l = name.to_ascii_lowercase();
        self.env.commands.iter().find(|c| c.name == l)
    }

    fn completion(&self, params: &Json) -> Json {
        let uri = params.at("textDocument.uri").str().unwrap_or("");
        let mut items = Vec::new();
        for k in KEYWORDS {
            items.push(obj([("label", s(*k)), ("kind", n(14))]));
        }
        for c in &self.env.commands {
            let doc = help(&c.display).unwrap_or("");
            items.push(obj([("label", s(c.display.clone())), ("kind", n(3)), ("detail", s(signature(c))), ("documentation", s(doc))]));
        }
        for (name, v) in &self.env.constants {
            items.push(obj([("label", s(name.clone())), ("kind", n(21)), ("detail", s(format!("= {}", const_text(v))))]));
        }
        if let Some(text) = self.docs.get(uri) {
            for d in declarations(text) {
                let kind = match d.kind {
                    12 => 3,
                    5 => 7,
                    10 => 13,
                    14 => 21,
                    20 => 18,
                    _ => 6,
                };
                items.push(obj([("label", s(d.name)), ("kind", n(kind)), ("detail", s(d.text))]));
            }
        }
        Json::Arr(items)
    }

    fn hover(&self, params: &Json) -> Json {
        let (line, _, col) = self.line(params);
        let Some(word) = word_at(&line, col) else { return Json::Null };
        let uri = params.at("textDocument.uri").str().unwrap_or("");
        let text = if let Some(c) = self.command(&word) {
            // the signature, then what the command does (docs/commands.txt)
            format!("```neptunebasic\n{}\n```\n{}", signature(c), help(&c.display).unwrap_or(""))
        } else if let Some((name, v)) = self.env.constants.iter().find(|(k, _)| k.eq_ignore_ascii_case(&word)) {
            format!("```neptunebasic\nConst {name} = {}\n```", const_text(v))
        } else if let Some(d) = self.docs.get(uri).and_then(|t| declarations(t).into_iter().find(|d| d.name.eq_ignore_ascii_case(&word))) {
            format!("```neptunebasic\n{}\n```\nline {}", d.text, d.line + 1)
        } else {
            return Json::Null;
        };
        obj([("contents", obj([("kind", s("markdown")), ("value", s(text))]))])
    }

    fn signature_help(&self, params: &Json) -> Json {
        let (line, _, col) = self.line(params);
        let Some((name, arg)) = call_at(&line, col) else { return Json::Null };
        let Some(c) = self.command(&name) else { return Json::Null };
        let label = signature(c);
        let params: Vec<Json> = c
            .params
            .iter()
            .map(|p| obj([("label", s(match &p.default {
                Some(d) => format!("[{}{}={}]", p.name, tag(&p.ty), const_text(d)),
                None => format!("{}{}", p.name, tag(&p.ty)),
            }))]))
            .collect();
        obj([
            ("signatures", Json::Arr(vec![obj([("label", s(label)), ("documentation", s(help(&c.display).unwrap_or(""))), ("parameters", Json::Arr(params))])])),
            ("activeSignature", n(0)),
            ("activeParameter", n(arg.min(c.params.len().saturating_sub(1)) as u32)),
        ])
    }

    fn symbols(&self, params: &Json) -> Json {
        let uri = params.at("textDocument.uri").str().unwrap_or("");
        let Some(text) = self.docs.get(uri) else { return Json::Arr(Vec::new()) };
        let lines: Vec<&str> = text.lines().collect();
        Json::Arr(
            declarations(text)
                .into_iter()
                .map(|d| {
                    let len = lines.get(d.line).map_or(0, |l| l.chars().count()) as u32;
                    let range = obj([("start", obj([("line", n(d.line as u32)), ("character", n(0))])), ("end", obj([("line", n(d.line as u32)), ("character", n(len))]))]);
                    obj([("name", s(d.name)), ("kind", n(d.kind)), ("range", range.clone()), ("selectionRange", range)])
                })
                .collect(),
        )
    }

    fn definition(&self, params: &Json) -> Json {
        let (line, _, col) = self.line(params);
        let uri = params.at("textDocument.uri").str().unwrap_or("");
        let Some(word) = word_at(&line, col) else { return Json::Null };
        let Some(d) = self.docs.get(uri).and_then(|t| declarations(t).into_iter().find(|d| d.name.eq_ignore_ascii_case(&word))) else {
            return Json::Null;
        };
        let pos = obj([("line", n(d.line as u32)), ("character", n(0))]);
        obj([("uri", s(uri)), ("range", obj([("start", pos.clone()), ("end", pos)]))])
    }

    /// The text of a file: what is open in the editor, or else what is on disk.
    fn text_of(&self, path: &str) -> Option<String> {
        let open = self.docs.iter().find(|(u, _)| canonical(&uri_to_path(u)) == path);
        match open {
            Some((_, t)) => Some(t.clone()),
            None => std::fs::read_to_string(path).ok(),
        }
    }

    /// The files of the program `path` belongs to: the files that include it,
    /// looked for beside it and one folder up, and everything those include.
    fn project(&self, path: &str) -> (Vec<String>, Vec<String>) {
        let mut nearby: Vec<String> = self.docs.keys().map(|u| canonical(&uri_to_path(u))).collect();
        let here = std::path::Path::new(path).parent().map(std::path::Path::to_path_buf).unwrap_or_default();
        for dir in [Some(here.as_path()), here.parent()].into_iter().flatten() {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("nb") || e.eq_ignore_ascii_case("bb")) {
                    nearby.push(canonical(&p.to_string_lossy()));
                }
            }
        }
        nearby.sort();
        nearby.dedup();
        let edges: HashMap<String, Vec<String>> = nearby.iter().map(|f| (f.clone(), self.included_by(f))).collect();
        // walk up to the files nothing else includes
        let mut up = vec![path.to_string()];
        let mut i = 0;
        while i < up.len() {
            for (f, inc) in &edges {
                if inc.contains(&up[i]) && !up.contains(f) {
                    up.push(f.clone());
                }
            }
            i += 1;
        }
        let mut roots: Vec<String> = up.iter().filter(|f| !up.iter().any(|g| edges.get(g).is_some_and(|inc| inc.contains(f)))).cloned().collect();
        if roots.is_empty() {
            roots.push(path.to_string());
        }
        // and back down: includes are followed even outside those folders
        let mut files = roots.clone();
        let mut i = 0;
        while i < files.len() {
            for inc in self.included_by(&files[i]) {
                if !files.contains(&inc) {
                    files.push(inc);
                }
            }
            i += 1;
        }
        if !files.iter().any(|f| f == path) {
            files.push(path.to_string());
        }
        (roots, files)
    }

    /// The other open documents that include this one, directly or not:
    /// their errors depend on its text too.
    fn includers(&self, uri: &str) -> Vec<String> {
        let target = canonical(&uri_to_path(uri));
        let reaches = |start: String| {
            let mut seen = vec![start];
            let mut i = 0;
            while i < seen.len() {
                for inc in self.included_by(&seen[i]) {
                    if inc == target {
                        return true;
                    }
                    if !seen.contains(&inc) {
                        seen.push(inc);
                    }
                }
                i += 1;
            }
            false
        };
        self.docs.keys().filter(|u| *u != uri && reaches(canonical(&uri_to_path(u)))).cloned().collect()
    }

    fn included_by(&self, file: &str) -> Vec<String> {
        let base = std::path::Path::new(file).parent().map(std::path::Path::to_path_buf).unwrap_or_default();
        self.text_of(file).map_or(Vec::new(), |t| neptune_lang::rename::includes(&t).iter().map(|i| canonical(&base.join(i).to_string_lossy())).collect())
    }

    /// `textDocument/rename`, as a workspace edit. Each change also carries
    /// the plain `path`, for the IDE.
    fn rename(&self, params: &Json) -> Result<Json, String> {
        let uri = params.at("textDocument.uri").str().unwrap_or("");
        let new = params.get("newName").str().unwrap_or("").trim().to_string();
        let path = canonical(&uri_to_path(uri));
        let (roots, files) = self.project(&path);
        for root in &roots {
            let text = self.text_of(root).unwrap_or_default();
            let compiled = compile(root, &text, &self.env, &mut OpenFiles(self));
            if compiled.diagnostics.has_errors() {
                return Err("Fix the errors in the program first, then rename".into());
            }
        }
        let texts: Vec<String> = files.iter().map(|f| self.text_of(f).unwrap_or_default().replace("\r\n", "\n")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let at = files.iter().position(|f| *f == path).unwrap_or(0);
        let line = params.at("position.line").num().unwrap_or(0.0) as usize;
        let col = params.at("position.character").num().unwrap_or(0.0) as usize;
        let offset = offset_of(&texts[at], line, col);
        let builtin = |name: &str| self.command(name).is_some() || self.env.constants.iter().any(|(c, _)| c.eq_ignore_ascii_case(name));
        let edits = neptune_lang::rename::rename(&refs, at, offset, &new, &builtin)?;
        let mut changes = Vec::new();
        for (i, f) in files.iter().enumerate() {
            let here: Vec<_> = edits.iter().filter(|e| e.file == i).collect();
            if here.is_empty() {
                continue;
            }
            let list = here.iter().map(|e| {
                let (l0, c0) = position(&texts[i], e.start);
                let (l1, c1) = position(&texts[i], e.end);
                let p = |l: usize, c: usize| obj([("line", n(l as u32)), ("character", n(c as u32))]);
                obj([("range", obj([("start", p(l0, c0)), ("end", p(l1, c1))])), ("newText", s(new.clone()))])
            });
            let doc_uri = self.docs.keys().find(|u| canonical(&uri_to_path(u)) == *f).cloned().unwrap_or_else(|| path_to_uri(f));
            changes.push(obj([("textDocument", obj([("uri", s(doc_uri)), ("version", Json::Null)])), ("path", s(f.clone())), ("edits", Json::Arr(list.collect()))]));
        }
        Ok(obj([("documentChanges", Json::Arr(changes))]))
    }
}

/// Includes come from the editor when they are open there, unsaved text and all.
struct OpenFiles<'a>(&'a Server);

impl SourceLoader for OpenFiles<'_> {
    fn load(&mut self, path: &str, from: &str) -> Result<(String, String), String> {
        let base = std::path::Path::new(from).parent().unwrap_or(std::path::Path::new("."));
        let full = canonical(&base.join(path).to_string_lossy());
        let text = self.0.text_of(&full).ok_or_else(|| format!("unable to open include file '{path}'"))?;
        Ok((full, text))
    }
}

/// One spelling per file: `canonicalize` where the file exists, without the
/// `\\?\` Windows puts in front.
fn canonical(path: &str) -> String {
    let full = std::fs::canonicalize(path).map_or_else(|_| path.to_string(), |p| p.to_string_lossy().into_owned());
    match full.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => rest.to_string(),
        _ => full,
    }
}

fn path_to_uri(path: &str) -> String {
    let unix = path.replace('\\', "/");
    let mut out = String::from("file://");
    if !unix.starts_with('/') {
        out.push('/');
    }
    for b in unix.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'/' | b':' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The byte offset of a 0-based line and character in normalised (LF) text.
fn offset_of(text: &str, line: usize, col: usize) -> usize {
    let mut start = 0;
    for _ in 0..line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return text.len(),
        }
    }
    let rest = &text[start..];
    let line_text = &rest[..rest.find('\n').unwrap_or(rest.len())];
    start + line_text.char_indices().nth(col).map_or(line_text.len(), |(i, _)| i)
}

fn send(out: &mut impl Write, msg: &Json) {
    let body = msg.to_string();
    let _ = write!(out, "Content-Length: {}\r\n\r\n{body}", body.len());
    let _ = out.flush();
}

/// Reads one LSP message; `None` at end of input.
fn read_message(input: &mut impl BufRead) -> Option<Json> {
    let mut len = None;
    loop {
        let mut header = String::new();
        if input.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let header = header.trim();
        if header.is_empty() {
            break;
        }
        if let Some(v) = header.strip_prefix("Content-Length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0u8; len?];
    input.read_exact(&mut body).ok()?;
    parse(&String::from_utf8_lossy(&body)).ok()
}

pub fn run() {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut server = Server { env: neptune_runtime::env(), docs: HashMap::new() };
    while let Some(msg) = read_message(&mut input) {
        let method = msg.get("method").str().unwrap_or("").to_string();
        let params = msg.get("params").clone();
        let id = msg.get("id").clone();
        let reply = |out: &mut std::io::StdoutLock, result: Json| {
            send(out, &obj([("jsonrpc", s("2.0")), ("id", id.clone()), ("result", result)]));
        };
        match method.as_str() {
            "initialize" => reply(
                &mut out,
                obj([
                    (
                        "capabilities",
                        obj([
                            ("textDocumentSync", n(1)),
                            ("completionProvider", obj([("triggerCharacters", Json::Arr(vec![s(".")]))])),
                            ("hoverProvider", Json::Bool(true)),
                            ("signatureHelpProvider", obj([("triggerCharacters", Json::Arr(vec![s("("), s(","), s(" ")]))])),
                            ("documentSymbolProvider", Json::Bool(true)),
                            ("definitionProvider", Json::Bool(true)),
                            ("renameProvider", Json::Bool(true)),
                        ]),
                    ),
                    ("serverInfo", obj([("name", s("neptune")), ("version", s(env!("CARGO_PKG_VERSION")))])),
                ]),
            ),
            "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didSave" => {
                let uri = params.at("textDocument.uri").str().unwrap_or("").to_string();
                let text = if method.ends_with("didOpen") {
                    params.at("textDocument.text").str().map(str::to_string)
                } else {
                    // full sync: the last change holds the whole text
                    params.get("contentChanges").arr().last().and_then(|c| c.get("text").str()).map(str::to_string)
                };
                if let Some(t) = text {
                    server.docs.insert(uri.clone(), t);
                }
                if server.docs.contains_key(&uri) {
                    let mut stale = server.includers(&uri);
                    stale.push(uri.clone());
                    for u in stale {
                        let d = server.diagnostics(&u);
                        send(&mut out, &obj([("jsonrpc", s("2.0")), ("method", s("textDocument/publishDiagnostics")), ("params", d)]));
                    }
                }
            }
            "textDocument/didClose" => {
                let uri = params.at("textDocument.uri").str().unwrap_or("").to_string();
                server.docs.remove(&uri);
                send(
                    &mut out,
                    &obj([("jsonrpc", s("2.0")), ("method", s("textDocument/publishDiagnostics")), ("params", obj([("uri", s(uri)), ("diagnostics", Json::Arr(Vec::new()))]))]),
                );
            }
            "textDocument/completion" => reply(&mut out, server.completion(&params)),
            "textDocument/hover" => reply(&mut out, server.hover(&params)),
            "textDocument/signatureHelp" => reply(&mut out, server.signature_help(&params)),
            "textDocument/documentSymbol" => reply(&mut out, server.symbols(&params)),
            "textDocument/definition" => reply(&mut out, server.definition(&params)),
            "textDocument/rename" => match server.rename(&params) {
                Ok(edit) => reply(&mut out, edit),
                Err(message) => send(&mut out, &obj([("jsonrpc", s("2.0")), ("id", id.clone()), ("error", obj([("code", n(1)), ("message", s(message))]))])),
            },
            "shutdown" => reply(&mut out, Json::Null),
            "exit" => break,
            _ if !id.is_null() => {
                // unknown request: answer "method not found" so the client does not wait
                send(
                    &mut out,
                    &obj([("jsonrpc", s("2.0")), ("id", id.clone()), ("error", obj([("code", n(-32601)), ("message", s(format!("{method} is not supported")))]))]),
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command has a description in docs/commands.txt (modern names
    /// use their command's), so the reference and hover help are complete.
    #[test]
    fn every_command_is_documented() {
        let env = neptune_runtime::env();
        let missing: Vec<&str> = env.commands.iter().map(|c| c.display.as_str()).filter(|n| help(n).is_none()).collect();
        assert!(missing.is_empty(), "add these to docs/commands.txt: {missing:?}");
    }

    #[test]
    fn finds_declarations() {
        let src = "Global score = 0, name$ = \"x\"\nConst MAX = 10\nType Ship\nEnd Type\nFunction Fire(x)\nEnd Function\n.start\n; Function Commented";
        let d = declarations(src);
        let names: Vec<(&str, u32)> = d.iter().map(|d| (d.name.as_str(), d.kind)).collect();
        assert_eq!(names, vec![("score", 13), ("name", 13), ("MAX", 14), ("Ship", 5), ("Fire", 12), ("start", 20)]);
    }

    #[test]
    fn words_and_calls() {
        assert_eq!(word_at("  PositionEntity cube, 1", 5).as_deref(), Some("PositionEntity"));
        assert_eq!(call_at("PositionEntity cube, 1, ", 24), Some(("PositionEntity".to_string(), 2)));
        assert_eq!(call_at("x = EntityX(cube, ", 18), Some(("EntityX".to_string(), 1)));
        assert_eq!(call_at("a = 1 : MoveEntity p, ", 22), Some(("MoveEntity".to_string(), 1)));
        assert_eq!(call_at("Text 0, 0, \"a, b\", ", 19), Some(("Text".to_string(), 3)));
    }

    /// What a program does, leaving out what its names are.
    fn behaviour(p: &neptune_lang::Program) -> String {
        let funcs: Vec<_> = p.functions.iter().map(|f| format!("{}{:?}{:?}", f.n_params, f.locals, f.code)).collect();
        let globals: Vec<_> = p.globals.iter().map(|g| g.1).collect();
        format!("{funcs:?}{globals:?}{}{:?}{}", p.arrays.len(), p.strings, p.types.len())
    }

    /// A rename that missed one use would still compile (BASIC just makes a
    /// new variable), so this renames names in every example and checks the
    /// bytecode stays the same. `NEPTUNE_RENAME_ALL=1` tries every name.
    #[test]
    fn renaming_never_changes_what_an_example_does() {
        use neptune_lang::lexer::{lex, Tok};
        let everything = std::env::var_os("NEPTUNE_RENAME_ALL").is_some();
        let server = Server { env: neptune_runtime::env(), docs: HashMap::new() };
        let builtin = |name: &str| server.command(name).is_some() || server.env.constants.iter().any(|(c, _)| c.eq_ignore_ascii_case(name));
        let (mut renamed, mut refused) = (0, 0);
        let mut other_refusals = Vec::new();
        let mut stack = vec![std::path::PathBuf::from("../../examples")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "nb") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
                if !neptune_lang::rename::includes(&text).is_empty() {
                    continue;
                }
                let before = compile("x.nb", &text, &server.env, &mut neptune_lang::NoLoader);
                let Some(program) = before.program else { continue };
                let want = behaviour(&program);
                // the first and last use of each name
                let mut d = neptune_lang::Diagnostics::default();
                let mut uses: Vec<(String, usize)> = Vec::new();
                for t in lex(&text, 0, 0, &mut d) {
                    if let Tok::Ident(name) = t.tok {
                        match uses.iter().rposition(|(n, _)| *n == name) {
                            Some(last) if uses[..last].iter().any(|(n, _)| *n == name) => uses[last].1 = t.span.start as usize,
                            _ => uses.push((name, t.span.start as usize)),
                        }
                    }
                }
                let step = if everything { 1 } else { (uses.len() / 8).max(1) };
                for (name, at) in uses.iter().step_by(step) {
                    let edits = match neptune_lang::rename::rename(&[&text], 0, *at, "zzrenamed", &builtin) {
                        Ok(e) => e,
                        Err(e) => {
                            refused += 1;
                            if !e.contains("built in") {
                                other_refusals.push(format!("{}: {e}", path.file_name().unwrap_or_default().to_string_lossy()));
                            }
                            continue;
                        }
                    };
                    let mut out = text.clone();
                    for e in edits.iter().rev() {
                        out.replace_range(e.start..e.end, "zzrenamed");
                    }
                    let after = compile("x.nb", &out, &server.env, &mut neptune_lang::NoLoader);
                    let Some(changed) = after.program else {
                        panic!("{}: renaming {name} at {at} broke the program:\n{}", path.display(), after.render_diagnostics());
                    };
                    assert_eq!(behaviour(&changed), want, "{}: renaming {name} at {at} changed the program", path.display());
                    renamed += 1;
                }
            }
        }
        eprintln!("renamed {renamed}, refused {refused}, not built-in: {other_refusals:#?}");
        assert!(renamed > 50, "only {renamed} renames were tried");
    }

    #[test]
    fn rename_positions_round_trip() {
        let text = "ab\nédc x\n";
        let at = offset_of(text, 1, 4);
        assert_eq!(&text[at..at + 1], "x");
        assert_eq!(position(text, at), (1, 4));
        assert_eq!(offset_of(text, 9, 0), text.len());
        assert_eq!(path_to_uri("C:\\My Games\\a.nb"), "file:///C:/My%20Games/a.nb");
        assert_eq!(path_to_uri("/home/me/a.nb"), "file:///home/me/a.nb");
    }

    #[test]
    fn positions_and_uris() {
        assert_eq!(position("ab\ncd", 4), (1, 1));
        assert_eq!(uri_to_path("file:///c%3A/Games/My%20Game/main.nb"), "c:/Games/My Game/main.nb");
        assert_eq!(uri_to_path("file:///home/me/a.nb"), "/home/me/a.nb");
    }

    #[test]
    fn signatures_read_well() {
        let env = neptune_runtime::env();
        let c = env.commands.iter().find(|c| c.name == "positionentity").unwrap();
        assert_eq!(signature(c), "PositionEntity(entity%, x#, y#, z#, [global%=0])");
    }
}
