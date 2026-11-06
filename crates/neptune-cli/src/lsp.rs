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
use neptune_lang::{compile, Env, FsLoader};

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
        let compiled = compile(&path, &text, &self.env, &mut FsLoader);
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
                    let d = server.diagnostics(&uri);
                    send(&mut out, &obj([("jsonrpc", s("2.0")), ("method", s("textDocument/publishDiagnostics")), ("params", d)]));
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
