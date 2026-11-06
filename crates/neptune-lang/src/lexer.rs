//! Lexer: turns NeptuneBASIC source text into tokens.
//!
//! Follows Blitz3D's tokenizer rules (`;` comments, `$FF` hex, `%101` binary,
//! case-insensitive keywords, two-word keywords such as `End If`) and adds
//! `'` comments, `_` line continuation, `$"..."` interpolated strings and
//! compound assignment operators.

use crate::diag::{Diagnostics, FileId, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kw {
    Dim, Goto, Gosub, Return, Exit, Continue, If, Then, Else, EndIf, ElseIf,
    While, Wend, For, To, Step, Next, Function, EndFunction, Type, EndType,
    Each, Local, Global, Field, Const, Select, Case, Default, EndSelect,
    Repeat, Until, Forever, Data, Read, Restore, Abs, Sgn, Mod, Pi, True, False,
    Int, Float, Str, Include, Import, New, Delete, First, Last, Insert, Before,
    After, Null, Object, Handle, And, Or, Xor, Not, Shl, Shr, Sar, AndAlso, OrElse,
    End, Stop, As, Try, Catch, EndTry, Throw, Method, EndMethod, SelfKw, Strict,
    Classic, Variant, Enum, EndEnum, In,
}

fn keyword(s: &str) -> Option<Kw> {
    use Kw::*;
    Some(match s {
        "dim" => Dim, "goto" => Goto, "gosub" => Gosub, "return" => Return,
        "exit" => Exit, "continue" => Continue, "if" => If, "then" => Then,
        "else" => Else, "endif" => EndIf, "elseif" => ElseIf, "while" => While,
        "wend" => Wend, "for" => For, "to" => To, "step" => Step, "next" => Next,
        "function" => Function, "endfunction" => EndFunction, "type" => Type,
        "endtype" => EndType, "each" => Each, "local" => Local, "global" => Global,
        "field" => Field, "const" => Const, "select" => Select, "case" => Case,
        "default" => Default, "endselect" => EndSelect, "repeat" => Repeat,
        "until" => Until, "forever" => Forever, "data" => Data, "read" => Read,
        "restore" => Restore, "abs" => Abs, "sgn" => Sgn, "mod" => Mod, "pi" => Pi,
        "true" => True, "false" => False, "int" => Int, "float" => Float,
        "str" => Str, "include" => Include, "import" => Import, "new" => New,
        "delete" => Delete, "first" => First, "last" => Last, "insert" => Insert,
        "before" => Before, "after" => After, "null" => Null, "object" => Object,
        "handle" => Handle, "and" => And, "or" => Or, "xor" => Xor, "not" => Not,
        "shl" => Shl, "shr" => Shr, "sar" => Sar, "andalso" => AndAlso,
        "orelse" => OrElse, "end" => End, "stop" => Stop, "as" => As, "try" => Try,
        "catch" => Catch, "endtry" => EndTry, "throw" => Throw, "method" => Method,
        "endmethod" => EndMethod, "self" => SelfKw, "strict" => Strict,
        "classic" => Classic, "variant" => Variant, "enum" => Enum,
        "endenum" => EndEnum, "in" => In,
        _ => return None,
    })
}

/// Keywords formed from `End <word>` / `Else If`.
fn compound_keyword(first: Kw, second: &str) -> Option<Kw> {
    match (first, second) {
        (Kw::End, "if") => Some(Kw::EndIf),
        (Kw::End, "function") => Some(Kw::EndFunction),
        (Kw::End, "type") => Some(Kw::EndType),
        (Kw::End, "select") => Some(Kw::EndSelect),
        (Kw::End, "try") => Some(Kw::EndTry),
        (Kw::End, "method") => Some(Kw::EndMethod),
        (Kw::End, "enum") => Some(Kw::EndEnum),
        (Kw::Else, "if") => Some(Kw::ElseIf),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Int(i32),
    Float(f32),
    Str(String),
    /// `$"..."` – raw contents; `{expr}` holes are parsed by the parser.
    Interp(String),
    /// Identifier, lower-cased (the language is case-insensitive).
    Ident(String),
    Kw(Kw),
    /// Single/double character operator or punctuation.
    Op(&'static str),
    Newline,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    /// True when whitespace preceded this token (needed for `a$` vs `a $FF`).
    pub space_before: bool,
}

const OPS2: &[&str] = &["<>", "><", "<=", "=<", ">=", "=>", "+=", "-=", "*=", "/="];
const OPS1: &[&str] = &[
    "+", "-", "*", "/", "^", "=", "<", ">", "(", ")", ",", "\\", "[", "]", ".", ":",
    "%", "#", "$", "~", "{", "}",
];

/// Tokenizes one file. `base` is added to all spans (used for interpolation holes).
pub fn lex(src: &str, file: FileId, base: usize, diags: &mut Diagnostics) -> Vec<Token> {
    let b = src.as_bytes();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    let mut space = true;
    let sp = |s: usize, e: usize| Span::new(file, base + s, base + e);
    while i < b.len() {
        let c = b[i];
        let start = i;
        match c {
            b'\n' => {
                out.push(Token { tok: Tok::Newline, span: sp(i, i + 1), space_before: space });
                i += 1;
                space = true;
                continue;
            }
            b' ' | b'\t' | b'\r' => {
                i += 1;
                space = true;
                continue;
            }
            b';' | b'\'' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'_' if space && rest_is_blank(b, i + 1) => {
                // line continuation: skip to and including newline
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                i += 1;
                space = true;
                continue;
            }
            _ => {}
        }
        let tok;
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let mut is_float = false;
            if i < b.len() && b[i] == b'.' && b.get(i + 1).is_none_or(|d| d.is_ascii_digit() || !d.is_ascii_alphabetic()) {
                is_float = true;
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                if j < b.len() && b[j].is_ascii_digit() {
                    is_float = true;
                    i = j;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text = &src[start..i];
            tok = if is_float {
                Tok::Float(text.parse().unwrap_or(0.0))
            } else {
                match text.parse::<i64>() {
                    Ok(v) if v <= u32::MAX as i64 => Tok::Int(v as u32 as i32),
                    _ => {
                        diags.error(sp(start, i), "integer constant too large");
                        Tok::Int(0)
                    }
                }
            };
        } else if c == b'%' && matches!(b.get(i + 1), Some(b'0' | b'1')) && (space || !prev_is_value(&out)) {
            i += 1;
            let mut v: u32 = 0;
            while i < b.len() && (b[i] == b'0' || b[i] == b'1') {
                v = (v << 1) | (b[i] - b'0') as u32;
                i += 1;
            }
            tok = Tok::Int(v as i32);
        } else if c == b'$' && b.get(i + 1).is_some_and(u8::is_ascii_hexdigit) && (space || !prev_is_value(&out)) {
            i += 1;
            let mut v: u32 = 0;
            while i < b.len() && b[i].is_ascii_hexdigit() {
                v = (v << 4) | (b[i] as char).to_digit(16).unwrap();
                i += 1;
            }
            tok = Tok::Int(v as i32);
        } else if c == b'"' || (c == b'$' && b.get(i + 1) == Some(&b'"')) {
            let interp = c == b'$';
            i += if interp { 2 } else { 1 };
            let s = i;
            // Inside `{...}` holes of an interpolated string, quotes start
            // nested string literals instead of ending the string.
            let mut depth = 0;
            while i < b.len() && b[i] != b'\n' && (b[i] != b'"' || depth > 0) {
                match b[i] {
                    b'{' if interp && depth == 0 && b.get(i + 1) == Some(&b'{') => i += 1,
                    b'}' if interp && depth == 0 && b.get(i + 1) == Some(&b'}') => i += 1,
                    b'{' if interp => depth += 1,
                    b'}' if interp && depth > 0 => depth -= 1,
                    b'"' => {
                        i += 1;
                        while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            let text = src[s..i].to_string();
            if i < b.len() && b[i] == b'"' {
                i += 1;
            } else {
                diags.error(sp(start, i), "unterminated string literal");
            }
            tok = if interp { Tok::Interp(text) } else { Tok::Str(text) };
        } else if c.is_ascii_alphabetic() {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = src[start..i].to_ascii_lowercase();
            match keyword(&word) {
                Some(kw @ (Kw::End | Kw::Else)) => {
                    // look for "End If", "Else If", ...
                    let mut j = i;
                    while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                        j += 1;
                    }
                    let ws = j;
                    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                        j += 1;
                    }
                    let second = src[ws..j].to_ascii_lowercase();
                    if let Some(ck) = (j > ws).then(|| compound_keyword(kw, &second)).flatten() {
                        i = j;
                        tok = Tok::Kw(ck);
                    } else {
                        tok = Tok::Kw(kw);
                    }
                }
                Some(kw) => tok = Tok::Kw(kw),
                None => tok = Tok::Ident(word),
            }
        } else {
            let two = src.get(i..i + 2).and_then(|t| OPS2.iter().find(|o| **o == t));
            if let Some(op) = two {
                i += 2;
                tok = Tok::Op(match *op {
                    "><" => "<>",
                    "=<" => "<=",
                    "=>" => ">=",
                    o => o,
                });
            } else if let Some(op) = OPS1.iter().find(|o| o.as_bytes()[0] == c) {
                i += 1;
                tok = Tok::Op(op);
            } else {
                let ch = src[i..].chars().next().unwrap();
                i += ch.len_utf8();
                diags.error(sp(start, i), format!("unexpected character '{ch}'"));
                space = true;
                continue;
            }
        }
        out.push(Token { tok, span: sp(start, i), space_before: space });
        space = false;
    }
    out.push(Token { tok: Tok::Newline, span: sp(b.len(), b.len()), space_before: true });
    out.push(Token { tok: Tok::Eof, span: sp(b.len(), b.len()), space_before: true });
    out
}

fn rest_is_blank(b: &[u8], mut i: usize) -> bool {
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t' || b[i] == b'\r') {
        i += 1;
    }
    i >= b.len() || b[i] == b'\n' || b[i] == b';' || b[i] == b'\''
}

/// Whether the previous token ends a value (so a following `$`/`%` is a type tag).
fn prev_is_value(out: &[Token]) -> bool {
    matches!(out.last().map(|t| &t.tok), Some(Tok::Ident(_) | Tok::Op(")") | Tok::Op("]")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        let mut d = Diagnostics::default();
        let t = lex(s, 0, 0, &mut d);
        assert!(!d.has_errors(), "{:?}", d.items);
        t.into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn numbers_and_tags() {
        assert_eq!(
            toks("a$ = $FF + %101 + 1.5 + .5"),
            vec![
                Tok::Ident("a".into()), Tok::Op("$"), Tok::Op("="), Tok::Int(255), Tok::Op("+"),
                Tok::Int(5), Tok::Op("+"), Tok::Float(1.5), Tok::Op("+"), Tok::Float(0.5),
                Tok::Newline, Tok::Eof
            ]
        );
    }

    #[test]
    fn compound_keywords_and_comments() {
        assert_eq!(
            toks("End If ; comment\nElse If x ' other\nEND"),
            vec![
                Tok::Kw(Kw::EndIf), Tok::Newline, Tok::Kw(Kw::ElseIf), Tok::Ident("x".into()),
                Tok::Newline, Tok::Kw(Kw::End), Tok::Newline, Tok::Eof
            ]
        );
    }

    #[test]
    fn continuation_and_compound_ops() {
        assert_eq!(
            toks("x += 1 _\n + 2"),
            vec![Tok::Ident("x".into()), Tok::Op("+="), Tok::Int(1), Tok::Op("+"), Tok::Int(2), Tok::Newline, Tok::Eof]
        );
    }

    #[test]
    fn hex_overflow_wraps_like_blitz() {
        assert_eq!(toks("$FFFFFFFF")[0], Tok::Int(-1));
    }
}
