//! What the IDE knows about NeptuneBASIC without asking the compiler:
//! tokens for colouring, blocks for folding and indenting, declarations for
//! the outline, and the command reference.

use std::collections::HashMap;

use crate::buffer::is_word;

pub const KEYWORDS: &[&str] = &[
    "Dim", "Goto", "Gosub", "Return", "Exit", "Continue", "If", "Then", "Else", "EndIf", "ElseIf", "While", "Wend", "For", "To", "Step", "Next", "Function", "EndFunction", "Type",
    "EndType", "Each", "Local", "Global", "Field", "Const", "Select", "Case", "Default", "EndSelect", "Repeat", "Until", "Forever", "Data", "Read", "Restore", "Abs", "Sgn", "Mod", "Pi",
    "True", "False", "Int", "Float", "Str", "Include", "Import", "New", "Delete", "First", "Last", "Insert", "Before", "After", "Null", "Object", "Handle", "And", "Or", "Xor", "Not", "Shl",
    "Shr", "Sar", "AndAlso", "OrElse", "End", "Stop", "As", "Try", "Catch", "EndTry", "Throw", "Method", "EndMethod", "Self", "Strict", "Classic", "Variant", "Enum", "EndEnum", "In", "String",
];

/// `docs/commands.txt`: every command's description.
const HELP: &str = include_str!("../../../docs/commands.txt");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tk {
    Plain,
    Keyword,
    Command,
    Constant,
    Number,
    Str,
    Comment,
    Op,
    Label,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: Tk,
}

#[derive(Clone, Debug)]
pub struct HelpEntry {
    pub name: String,
    pub category: String,
    pub text: String,
    pub ext: bool,
    /// `PositionEntity(entity%, x#, ...)`, once the language server has said.
    pub signature: String,
}

/// Every name with a fixed spelling, looked up in lower case.
#[derive(Default)]
pub struct Known {
    pub words: HashMap<String, (String, Tk)>,
    pub help: Vec<HelpEntry>,
    pub help_index: HashMap<String, usize>,
}

impl Known {
    pub fn new() -> Self {
        let mut k = Known::default();
        let mut category = String::new();
        for line in HELP.lines() {
            if let Some(c) = line.strip_prefix("# ") {
                category = c.trim().to_string();
            } else if !line.starts_with(';') {
                if let Some((name, text)) = line.split_once(':') {
                    let name = name.trim();
                    if name.is_empty() || !name.chars().all(is_word) {
                        continue;
                    }
                    let text = text.trim();
                    let (ext, text) = match text.strip_prefix("(ext)") {
                        Some(t) => (true, t.trim()),
                        None => (false, text),
                    };
                    k.help_index.insert(name.to_ascii_lowercase(), k.help.len());
                    k.help.push(HelpEntry { name: name.to_string(), category: category.clone(), text: text.to_string(), ext, signature: String::new() });
                    k.words.insert(name.to_ascii_lowercase(), (name.to_string(), Tk::Command));
                }
            }
        }
        for w in KEYWORDS {
            k.words.insert(w.to_ascii_lowercase(), (w.to_string(), Tk::Keyword));
        }
        k
    }

    pub fn add(&mut self, name: &str, kind: Tk) {
        let key = name.to_ascii_lowercase();
        // keywords stay keywords
        if self.words.get(&key).is_none_or(|(_, k)| *k != Tk::Keyword) {
            self.words.insert(key, (name.to_string(), kind));
        }
    }

    pub fn help_for(&self, name: &str) -> Option<&HelpEntry> {
        self.help_index.get(&name.to_ascii_lowercase()).map(|&i| &self.help[i])
    }

    pub fn set_signature(&mut self, name: &str, sig: &str) {
        if let Some(&i) = self.help_index.get(&name.to_ascii_lowercase()) {
            self.help[i].signature = sig.to_string();
        }
    }

    pub fn canonical(&self, word: &str) -> Option<&str> {
        self.words.get(&word.to_ascii_lowercase()).map(|(s, _)| s.as_str())
    }
}

/// Splits one line into tokens. Everything in NeptuneBASIC that spans several
/// characters ends with its line, so each line stands alone.
pub fn tokenize(line: &[char], known: &Known, out: &mut Vec<Token>) {
    out.clear();
    let n = line.len();
    let mut i = 0;
    while i < n {
        let c = line[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let kind = if c == ';' || c == '\'' {
            i = n;
            Tk::Comment
        } else if c == '"' || (c == '$' && line.get(i + 1) == Some(&'"')) {
            i += if c == '$' { 2 } else { 1 };
            while i < n && line[i] != '"' {
                i += 1;
            }
            i = (i + 1).min(n);
            Tk::Str
        } else if c.is_ascii_digit() || (c == '.' && line.get(i + 1).is_some_and(char::is_ascii_digit)) {
            while i < n && (line[i].is_ascii_alphanumeric() || line[i] == '.') {
                // 1e-5
                if matches!(line[i], 'e' | 'E') && matches!(line.get(i + 1), Some('-' | '+')) {
                    i += 1;
                }
                i += 1;
            }
            Tk::Number
        } else if c == '$' && line.get(i + 1).is_some_and(char::is_ascii_hexdigit) {
            i += 1;
            while i < n && line[i].is_ascii_hexdigit() {
                i += 1;
            }
            Tk::Number
        } else if c == '%' && matches!(line.get(i + 1), Some('0' | '1')) && (i == 0 || !is_word(line[i - 1])) {
            i += 1;
            while i < n && matches!(line[i], '0' | '1') {
                i += 1;
            }
            Tk::Number
        } else if c == '.' && line[..i].iter().all(|c| c.is_whitespace()) && line.get(i + 1).is_some_and(|c| is_word(*c)) {
            i += 1;
            while i < n && is_word(line[i]) {
                i += 1;
            }
            Tk::Label
        } else if is_word(c) {
            while i < n && is_word(line[i]) {
                i += 1;
            }
            let word: String = line[start..i].iter().collect::<String>().to_ascii_lowercase();
            let kind = known.words.get(&word).map_or(Tk::Plain, |(_, k)| *k);
            // a type tag belongs to its name
            if i < n && matches!(line[i], '%' | '#' | '$') {
                i += 1;
            }
            kind
        } else {
            i += 1;
            Tk::Op
        };
        out.push(Token { start, end: i, kind });
    }
}

/// Column where a comment starts, outside strings.
pub fn comment_start(line: &str) -> Option<usize> {
    let mut in_str = false;
    for (i, c) in line.chars().enumerate() {
        match c {
            '"' => in_str = !in_str,
            ';' | '\'' if !in_str => return Some(i),
            _ => {}
        }
    }
    None
}

fn code_part(line: &str) -> &str {
    match comment_start(line) {
        Some(col) => {
            let at = line.char_indices().nth(col).map_or(line.len(), |(b, _)| b);
            &line[..at]
        }
        None => line,
    }
}

fn first_words(code: &str) -> (String, String) {
    let t = code.trim_start();
    let w: String = t.chars().take_while(|c| is_word(*c)).collect();
    let rest = t[w.len()..].trim_start();
    let w2: String = rest.chars().take_while(|c| is_word(*c)).collect();
    (w.to_ascii_lowercase(), w2.to_ascii_lowercase())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Block {
    Function,
    Method,
    Type,
    Enum,
    If,
    While,
    For,
    Repeat,
    Select,
    Try,
}

fn opens(code: &str) -> Option<Block> {
    let (a, _) = first_words(code);
    Some(match a.as_str() {
        "function" => Block::Function,
        "method" => Block::Method,
        "type" => Block::Type,
        "enum" => Block::Enum,
        "while" => Block::While,
        "for" => Block::For,
        "repeat" => Block::Repeat,
        "select" => Block::Select,
        "try" => Block::Try,
        "if" => {
            // "If a Then b" is complete on its line
            let lower = code.to_ascii_lowercase();
            let mut in_str = false;
            let cs: Vec<char> = lower.chars().collect();
            for i in 0..cs.len() {
                if cs[i] == '"' {
                    in_str = !in_str;
                } else if !in_str && cs[i..].starts_with(&['t', 'h', 'e', 'n']) && (i == 0 || !is_word(cs[i - 1])) && cs.get(i + 4).is_none_or(|c| !is_word(*c)) {
                    let rest: String = cs[i + 4..].iter().collect();
                    if !rest.trim().is_empty() {
                        return None;
                    }
                    break;
                }
            }
            Block::If
        }
        _ => return None,
    })
}

fn closes(code: &str) -> Option<Block> {
    let (a, b) = first_words(code);
    Some(match a.as_str() {
        "endfunction" => Block::Function,
        "endmethod" => Block::Method,
        "endtype" => Block::Type,
        "endenum" => Block::Enum,
        "endif" => Block::If,
        "endselect" => Block::Select,
        "endtry" => Block::Try,
        "wend" => Block::While,
        "next" => Block::For,
        "until" | "forever" => Block::Repeat,
        "end" => match b.as_str() {
            "function" => Block::Function,
            "method" => Block::Method,
            "type" => Block::Type,
            "enum" => Block::Enum,
            "if" => Block::If,
            "select" => Block::Select,
            "try" => Block::Try,
            _ => return None,
        },
        _ => return None,
    })
}

fn is_mid_block(code: &str) -> bool {
    let (a, _) = first_words(code);
    matches!(a.as_str(), "else" | "elseif" | "case" | "default" | "catch")
}

/// How the line after this one is indented relative to it.
pub fn indents_next(line: &str) -> bool {
    let code = code_part(line);
    opens(code).is_some() || is_mid_block(code)
}

/// True for lines that sit one level out from the block's body.
pub fn dedents_self(line: &str) -> bool {
    let code = code_part(line);
    closes(code).is_some() || is_mid_block(code)
}

/// Foldable regions as (first line, last line).
pub fn fold_regions(lines: &[String]) -> Vec<(usize, usize)> {
    let mut stack: Vec<(usize, Block)> = Vec::new();
    let mut out = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        // nearly every line starts with something that is not a block word
        let first = raw.trim_start().chars().next().unwrap_or(' ');
        if !first.is_ascii_alphabetic() {
            continue;
        }
        let code = code_part(raw);
        if let Some(b) = closes(code) {
            if let Some(at) = stack.iter().rposition(|(_, k)| *k == b) {
                let (start, _) = stack[at];
                stack.truncate(at);
                if i > start {
                    out.push((start, i));
                }
            }
        } else if let Some(b) = opens(code) {
            stack.push((i, b));
        }
    }
    out.sort_unstable();
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeclKind {
    Function,
    Method,
    Type,
    Enum,
    Global,
    Const,
    Label,
}

#[derive(Clone, Debug)]
pub struct Decl {
    pub name: String,
    pub kind: DeclKind,
    pub line: usize,
    pub text: String,
}

/// Finds what the program declares: functions, types, globals and labels.
pub fn declarations(lines: &[String]) -> Vec<Decl> {
    let mut out = Vec::new();
    for (line, raw) in lines.iter().enumerate() {
        let code = code_part(raw).trim();
        if code.is_empty() {
            continue;
        }
        let name_after = |kw: usize| {
            let rest = code[kw..].trim_start();
            let name: String = rest.chars().take_while(|c| is_word(*c)).collect();
            (!name.is_empty()).then_some(name)
        };
        let lower = code.get(..7.min(code.len())).unwrap_or("").to_ascii_lowercase();
        let text = code.to_string();
        let kw = |word: &str| lower.starts_with(word) && code[word.len()..].starts_with([' ', '\t']);
        let found = if kw("function") || code.to_ascii_lowercase().starts_with("function ") {
            Some((8, DeclKind::Function))
        } else if kw("method") {
            Some((6, DeclKind::Method))
        } else if kw("type") {
            Some((4, DeclKind::Type))
        } else if kw("enum") {
            Some((4, DeclKind::Enum))
        } else {
            None
        };
        if let Some((at, kind)) = found {
            if let Some(name) = name_after(at) {
                out.push(Decl { name, kind, line, text });
            }
        } else if kw("global") || kw("const") {
            let (at, kind) = if kw("global") { (6, DeclKind::Global) } else { (5, DeclKind::Const) };
            let mut depth = 0;
            let mut start = at;
            let mut parts = Vec::new();
            let mut in_str = false;
            for (b, c) in code.char_indices().skip_while(|(b, _)| *b < at) {
                match c {
                    '"' => in_str = !in_str,
                    '(' | '[' if !in_str => depth += 1,
                    ')' | ']' if !in_str => depth -= 1,
                    ',' if !in_str && depth == 0 => {
                        parts.push(&code[start..b]);
                        start = b + 1;
                    }
                    _ => {}
                }
            }
            parts.push(&code[start..]);
            for part in parts {
                let name: String = part.trim_start().chars().take_while(|c| is_word(*c)).collect();
                if name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
                    out.push(Decl { name, kind, line, text: text.clone() });
                }
            }
        } else if let Some(label) = code.strip_prefix('.') {
            let name: String = label.chars().take_while(|c| is_word(*c)).collect();
            if !name.is_empty() {
                out.push(Decl { name, kind: DeclKind::Label, line, text });
            }
        }
    }
    out
}

/// The identifier under (or just before) a column.
pub fn word_at(line: &str, col: usize) -> Option<(String, usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let mut start = col.min(chars.len());
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = col.min(chars.len());
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    (start < end).then(|| (chars[start..end].iter().collect(), start, end))
}

/// The command whose arguments the column is in, and the argument index:
/// `Foo(a, b|` and `Foo a, b|` both give ("Foo", 1).
pub fn call_at(line: &str, col: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = line.chars().take(col).collect();
    if comment_start(&chars.iter().collect::<String>()).is_some() {
        return None;
    }
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
            ')' | ']' => depth += 1,
            '[' if depth > 0 => depth -= 1,
            '(' if depth > 0 => depth -= 1,
            '(' => {
                let mut j = i;
                while j > 0 && chars[j - 1] == ' ' {
                    j -= 1;
                }
                let end = j;
                while j > 0 && (is_word(chars[j - 1]) || matches!(chars[j - 1], '%' | '#' | '$')) {
                    j -= 1;
                }
                let name = chars[j..end].iter().collect::<String>().trim_end_matches(['%', '#', '$']).to_string();
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
    let rest: String = chars[i..].iter().collect();
    let rest = rest.trim_start();
    let name: String = rest.chars().take_while(|c| is_word(*c)).collect();
    let after = &rest[name.len()..];
    (!name.is_empty() && after.starts_with(' ')).then_some((name, commas))
}

/// Splits `Name(a%, b#, [c%=0])` into its parameters.
pub fn signature_params(sig: &str) -> Vec<&str> {
    let Some(open) = sig.find('(') else { return Vec::new() };
    let inner = sig[open + 1..].trim_end_matches(')');
    if inner.is_empty() {
        return Vec::new();
    }
    inner.split(", ").collect()
}

/// `name` with an error position like `path:12:5` split off.
pub fn parse_location(text: &str) -> Option<(String, usize, usize)> {
    let text = text.trim();
    let (rest, col) = text.rsplit_once(':')?;
    let col: usize = col.trim().parse().ok()?;
    let (path, line) = rest.rsplit_once(':')?;
    let line: usize = line.trim().parse().ok()?;
    (!path.is_empty()).then(|| (path.to_string(), line, col))
}

/// Lines of program output that name a place in a source file: the compiler's
/// `--> file:1:2` and the runtime's `at file:1:2 (in f)`.
pub fn output_location(line: &str) -> Option<(String, usize, usize)> {
    let t = line.trim_start();
    let t = t.strip_prefix("--> ").or_else(|| t.strip_prefix("at ")).or_else(|| t.strip_prefix("called from "))?;
    let t = t.split(" (in ").next()?;
    parse_location(t)
}

/// A ready-made piece of code offered by completion; `$0` is the caret.
pub const SNIPPETS: &[(&str, &str, &str)] = &[
    ("func", "Function ... End Function", "Function $0()\n\t\nEnd Function"),
    ("for", "For ... Next", "For i = 1 To $0\n\t\nNext"),
    ("foreach", "For Each ... Next", "For t.$0 = Each \n\t\nNext"),
    ("if", "If ... EndIf", "If $0 Then\n\t\nEndIf"),
    ("ife", "If ... Else ... EndIf", "If $0 Then\n\t\nElse\n\t\nEndIf"),
    ("while", "While ... Wend", "While $0\n\t\nWend"),
    ("repeat", "Repeat ... Until", "Repeat\n\t\nUntil $0"),
    ("select", "Select ... End Select", "Select $0\n\tCase \n\t\n\tDefault\n\t\nEnd Select"),
    ("type", "Type ... End Type", "Type $0\n\tField \nEnd Type"),
    ("try", "Try ... Catch ... End Try", "Try\n\t$0\nCatch e$\n\t\nEnd Try"),
    ("main3d", "A complete 3D program", "Graphics3D 1280, 720\nSetBuffer BackBuffer()\n\ncamera = CreateCamera()\nPositionEntity camera, 0, 2, -6\nlight = CreateLight()\n\nWhile Not KeyHit(1)\n\t$0\n\tUpdateWorld\n\tRenderWorld\n\tFlip\nWend\nEnd"),
    ("main2d", "A complete 2D program", "Graphics 1280, 720\nSetBuffer BackBuffer()\n\nWhile Not KeyHit(1)\n\tCls\n\t$0\n\tFlip\nWend\nEnd"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(String, Tk)> {
        let k = Known::new();
        let chars: Vec<char> = src.chars().collect();
        let mut out = Vec::new();
        tokenize(&chars, &k, &mut out);
        out.iter().map(|t| (chars[t.start..t.end].iter().collect(), t.kind)).collect()
    }

    #[test]
    fn colours_a_line() {
        let t = kinds("If x% > $FF Then Print \"a;b\" ; done");
        assert_eq!(t[0], ("If".into(), Tk::Keyword));
        assert_eq!(t[1], ("x%".into(), Tk::Plain));
        assert_eq!(t[3], ("$FF".into(), Tk::Number));
        assert_eq!(t[5], ("Print".into(), Tk::Command));
        assert_eq!(t[6], ("\"a;b\"".into(), Tk::Str));
        assert_eq!(t[7], ("; done".into(), Tk::Comment));
        assert_eq!(kinds(".start")[0], (".start".into(), Tk::Label));
        assert_eq!(kinds("x$ = $\"hi {n}\"")[2], ("$\"hi {n}\"".into(), Tk::Str));
        assert_eq!(kinds("name$ = 1")[0], ("name$".into(), Tk::Plain));
        assert_eq!(kinds("a = %101")[2], ("%101".into(), Tk::Number));
        assert_eq!(kinds("a = 1.5e-3")[2], ("1.5e-3".into(), Tk::Number));
    }

    #[test]
    fn folds_blocks() {
        let src = "Function A()\nIf x Then\ny\nEndIf\nIf z Then w\nFor i=1 To 2\nNext\nEnd Function\nWhile 1\n; Wend\nWend";
        let lines: Vec<String> = src.lines().map(String::from).collect();
        assert_eq!(fold_regions(&lines), vec![(0, 7), (1, 3), (5, 6), (8, 10)]);
    }

    #[test]
    fn blocks_guide_indenting() {
        assert!(indents_next("  If a Then ; go"));
        assert!(!indents_next("If a Then b"));
        assert!(indents_next("Else"));
        assert!(dedents_self("\tWend"));
        assert!(dedents_self("End Function"));
        assert!(!dedents_self("End"));
    }

    #[test]
    fn finds_declarations() {
        let src = "Global score = 0, name$ = \"a,b\"\nConst MAX = 10\nType Ship\nEnd Type\nFunction Fire(x)\nEnd Function\n.start\n; Function Commented\n' Type Nope";
        let lines: Vec<String> = src.lines().map(String::from).collect();
        let d = declarations(&lines);
        let names: Vec<&str> = d.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["score", "name", "MAX", "Ship", "Fire", "start"]);
    }

    #[test]
    fn finds_calls_and_words() {
        assert_eq!(word_at("  PositionEntity cube", 5).unwrap().0, "PositionEntity");
        assert_eq!(call_at("PositionEntity cube, 1, ", 24), Some(("PositionEntity".to_string(), 2)));
        assert_eq!(call_at("x = EntityX(cube, ", 18), Some(("EntityX".to_string(), 1)));
        assert_eq!(call_at("Text 0, 0, \"a, b\", ", 19), Some(("Text".to_string(), 3)));
        assert_eq!(call_at("; Foo a, ", 9), None);
    }

    #[test]
    fn reads_output_locations() {
        assert_eq!(output_location("  --> C:/x/a.nb:2:5"), Some(("C:/x/a.nb".to_string(), 2, 5)));
        assert_eq!(output_location("  at a.nb:7:1 (in main)"), Some(("a.nb".to_string(), 7, 1)));
        assert_eq!(output_location("hello"), None);
    }

    #[test]
    fn command_reference_loads() {
        let k = Known::new();
        assert!(k.help.len() > 500);
        assert_eq!(k.canonical("positionentity"), Some("PositionEntity"));
        assert_eq!(k.canonical("WEND"), Some("Wend"));
        assert!(k.help_for("min").unwrap().ext);
    }

    #[test]
    fn splits_signatures() {
        assert_eq!(signature_params("F(a%, [b#=1])"), vec!["a%", "[b#=1]"]);
        assert!(signature_params("F()").is_empty());
    }
}
