//! Renaming a name everywhere it means the same thing.
//!
//! Works on tokens, which keep the position of every name (the syntax tree
//! does not for type annotations), and classifies each identifier by what
//! surrounds it: a local of one function, a global, a function or array, a
//! type, a field or method, a label or an enum member. Comments and strings
//! are never touched; the `{holes}` of interpolated strings are code and are.
//!
//! A name that cannot be pinned down is refused rather than guessed: fields
//! are tied to their `Type` only through the type of the expression before
//! the `\`, so a field name declared by two types cannot be renamed here.

use std::collections::{HashMap, HashSet};

use crate::diag::Diagnostics;
use crate::lexer::{lex, Kw, Tok};
use crate::parser::matching_brace;

/// Replace `start..end` (bytes) of `files[file]` with the new name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edit {
    pub file: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Scope {
    Global,
    Main,
    Func(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Sym {
    Var(Scope, String),
    Callable(String),
    Type(String),
    Member(String),
    Label(String),
    EnumMember(String, String),
}

struct T {
    tok: Tok,
    file: usize,
    start: usize,
    end: usize,
    space: bool,
}

/// The tokens of an interpolated string's holes, each in brackets.
fn holes(raw: &str, content_start: usize, file: usize, out: &mut Vec<T>) {
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if matches!(b[i], b'{' | b'}') && b.get(i + 1) == Some(&b[i]) {
            i += 2;
        } else if b[i] == b'{' {
            let Some(close) = matching_brace(raw, i) else { return };
            let mut d = Diagnostics::default();
            let base = content_start + i + 1;
            let at = (base, base);
            out.push(T { tok: Tok::Op("("), file, start: at.0, end: at.1, space: false });
            for t in lex(&raw[i + 1..close], file as _, base, &mut d) {
                push(t.tok, file, t.span.start as usize, t.span.end as usize, t.space_before, out);
            }
            out.push(T { tok: Tok::Op(")"), file, start: at.0, end: at.1, space: false });
            i = close + 1;
        } else {
            i += 1;
        }
    }
}

fn push(tok: Tok, file: usize, start: usize, end: usize, space: bool, out: &mut Vec<T>) {
    match tok {
        Tok::Interp(raw) => holes(&raw, start + 2, file, out),
        // the end of a hole is not the end of a statement
        Tok::Newline | Tok::Eof if !out.is_empty() && is_hole_end(out, file, start) => {}
        tok => out.push(T { tok, file, start, end, space }),
    }
}

/// Inside a hole, the sub-lexer's closing `Newline`/`Eof` sit before the
/// bracket that closes the hole. They have zero width, which real line ends
/// never do, and a file's own end is told apart by having no text after it.
fn is_hole_end(out: &[T], file: usize, start: usize) -> bool {
    let mut open = 0;
    for t in out.iter().rev() {
        if t.file != file {
            break;
        }
        match t.tok {
            Tok::Op("(") if t.start == t.end => {
                open += 1;
                if open > 0 {
                    return start >= t.start;
                }
            }
            Tok::Op(")") if t.start == t.end => open -= 1,
            Tok::Newline | Tok::Eof => break,
            _ => {}
        }
    }
    false
}

fn tokens(files: &[&str]) -> Vec<T> {
    let mut out = Vec::new();
    for (file, text) in files.iter().enumerate() {
        let mut d = Diagnostics::default();
        for t in lex(text, file as _, 0, &mut d) {
            push(t.tok, file, t.span.start as usize, t.span.end as usize, t.space_before, &mut out);
        }
    }
    out
}

/// The files named by `Include` and `Import` in `text`.
pub fn includes(text: &str) -> Vec<String> {
    let mut d = Diagnostics::default();
    let t = lex(&text.replace("\r\n", "\n"), 0, 0, &mut d);
    t.windows(2)
        .filter_map(|w| match (&w[0].tok, &w[1].tok) {
            (Tok::Kw(Kw::Include | Kw::Import), Tok::Str(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

#[derive(Default)]
struct Names {
    types: HashSet<String>,
    enums: HashMap<String, HashSet<String>>,
    callables: HashSet<String>,
    /// How many declarations of each field or method there are.
    members: HashMap<String, usize>,
    globals: HashSet<String>,
    locals: HashMap<usize, HashSet<String>>,
    /// The function each token is in.
    func_of: Vec<Option<usize>>,
    /// Tokens that declare a field.
    member_decl: HashSet<usize>,
    /// Tokens that declare an enum member, and of which enum.
    enum_decl: HashMap<usize, String>,
}

fn starts_statement(t: &[T], i: usize) -> bool {
    i == 0 || matches!(t[i - 1].tok, Tok::Newline | Tok::Eof | Tok::Op(":") | Tok::Kw(Kw::Then | Kw::Else))
}

fn ident(t: &[T], i: usize) -> Option<&str> {
    match t.get(i).map(|x| &x.tok) {
        Some(Tok::Ident(n)) => Some(n),
        _ => None,
    }
}

/// The names declared by `Global a, b = 1, c` and the like, from `i + 1` on.
fn declarators(t: &[T], i: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut j = i + 1;
    while let Some(x) = t.get(j) {
        match x.tok {
            Tok::Newline | Tok::Eof | Tok::Op(":") => break,
            Tok::Op("(" | "[") => depth += 1,
            Tok::Op(")" | "]") => depth -= 1,
            Tok::Ident(_) if depth == 0 && (j == i + 1 || matches!(t[j - 1].tok, Tok::Op(","))) => out.push(j),
            _ => {}
        }
        j += 1;
    }
    out
}

fn name_at(t: &[T], i: usize) -> String {
    ident(t, i).unwrap_or("").to_string()
}

fn collect(t: &[T]) -> Names {
    let mut n = Names { func_of: vec![None; t.len()], ..Default::default() };
    let mut func = None;
    let mut next_func = 0;
    let mut in_enum: Option<String> = None;
    for i in 0..t.len() {
        n.func_of[i] = func;
        match t[i].tok {
            Tok::Eof => {
                func = None;
                in_enum = None;
            }
            Tok::Kw(k @ (Kw::Function | Kw::Method)) => {
                let id = next_func;
                next_func += 1;
                func = Some(id);
                n.func_of[i] = func;
                let params = n.locals.entry(id).or_default();
                if ident(t, i + 1).is_some() {
                    if k == Kw::Function {
                        n.callables.insert(name_at(t, i + 1));
                    } else {
                        *n.members.entry(name_at(t, i + 1)).or_default() += 1;
                    }
                }
                // parameters: the first name of each comma-separated part
                let mut j = i + 1;
                while t.get(j).is_some_and(|x| !matches!(x.tok, Tok::Op("(") | Tok::Newline | Tok::Eof)) {
                    j += 1;
                }
                if t.get(j).is_some_and(|x| x.tok == Tok::Op("(")) {
                    let open = j;
                    let mut depth = 0;
                    while let Some(x) = t.get(j) {
                        match x.tok {
                            Tok::Op("(" | "[") => depth += 1,
                            Tok::Op(")" | "]") => depth -= 1,
                            Tok::Newline | Tok::Eof => break,
                            Tok::Ident(_) if depth == 1 && (j == open + 1 || t[j - 1].tok == Tok::Op(",")) => {
                                params.insert(name_at(t, j));
                            }
                            _ => {}
                        }
                        if depth == 0 {
                            break;
                        }
                        j += 1;
                    }
                }
            }
            Tok::Kw(Kw::EndFunction | Kw::EndMethod) => func = None,
            Tok::Kw(Kw::Type) => {
                n.types.insert(name_at(t, i + 1));
            }
            Tok::Kw(Kw::Enum) => {
                let e = name_at(t, i + 1);
                n.enums.entry(e.clone()).or_default();
                in_enum = Some(e);
            }
            Tok::Kw(Kw::EndEnum) => in_enum = None,
            Tok::Ident(_) if in_enum.is_some() && t[i - 1].tok != Tok::Kw(Kw::Enum) && (starts_statement(t, i) || t[i - 1].tok == Tok::Op(",")) => {
                let e = in_enum.clone().unwrap_or_default();
                n.enums.entry(e.clone()).or_default().insert(name_at(t, i));
                n.enum_decl.insert(i, e);
            }
            Tok::Kw(Kw::Dim) => {
                for d in declarators(t, i) {
                    n.callables.insert(name_at(t, d));
                }
            }
            Tok::Kw(Kw::Field) => {
                for d in declarators(t, i) {
                    *n.members.entry(name_at(t, d)).or_default() += 1;
                    n.member_decl.insert(d);
                }
            }
            Tok::Kw(Kw::Global | Kw::Const) => {
                for d in declarators(t, i) {
                    n.globals.insert(name_at(t, d));
                }
            }
            Tok::Kw(Kw::Local) => {
                if let Some(f) = func {
                    for d in declarators(t, i) {
                        let name = name_at(t, d);
                        n.locals.entry(f).or_default().insert(name);
                    }
                }
            }
            _ => {}
        }
    }
    n
}

fn is_tag(tok: &Tok) -> bool {
    matches!(tok, Tok::Op("%" | "#" | "$"))
}

fn classify(t: &[T], n: &Names, i: usize, builtin: &dyn Fn(&str) -> bool) -> Sym {
    let name = name_at(t, i);
    if n.member_decl.contains(&i) {
        return Sym::Member(name);
    }
    if let Some(e) = n.enum_decl.get(&i) {
        return Sym::EnumMember(e.clone(), name);
    }
    let prev = (i > 0).then(|| &t[i - 1].tok);
    match prev {
        Some(Tok::Op("\\")) | Some(Tok::Kw(Kw::Method)) => return Sym::Member(name),
        Some(Tok::Kw(Kw::Function | Kw::Dim)) => return Sym::Callable(name),
        Some(Tok::Kw(Kw::Goto | Kw::Gosub | Kw::Restore)) => return Sym::Label(name),
        Some(Tok::Kw(Kw::New | Kw::First | Kw::Last | Kw::As | Kw::Type | Kw::Enum)) => return Sym::Type(name),
        Some(Tok::Kw(Kw::Each)) if t.get(i + 1).map(|x| &x.tok) != Some(&Tok::Kw(Kw::In)) => return Sym::Type(name),
        Some(Tok::Kw(Kw::In)) if n.types.contains(&name) || n.enums.contains_key(&name) => return Sym::Type(name),
        Some(Tok::Kw(Kw::In)) if n.callables.contains(&name) => return Sym::Callable(name),
        Some(Tok::Op(".")) => {
            let owner = ident(t, i.wrapping_sub(2)).unwrap_or("");
            if n.enums.get(owner).is_some_and(|m| m.contains(&name)) {
                return Sym::EnumMember(owner.to_string(), name);
            }
            return if starts_statement(t, i - 1) { Sym::Label(name) } else { Sym::Type(name) };
        }
        _ => {}
    }
    let next = t.get(i + 1).map(|x| &x.tok);
    if next == Some(&Tok::Op(".")) && n.enums.get(&name).is_some_and(|m| ident(t, i + 2).is_some_and(|x| m.contains(x))) {
        return Sym::Type(name);
    }
    let after_tag = match next {
        Some(x) if is_tag(x) && !t[i + 1].space => t.get(i + 2).map(|x| &x.tok),
        other => other,
    };
    if after_tag == Some(&Tok::Op("(")) {
        return Sym::Callable(name);
    }
    if starts_statement(t, i) && (n.callables.contains(&name) || builtin(&name)) {
        let assigned = matches!(next, Some(Tok::Op("=" | "+=" | "-=" | "*=" | "/=" | "\\" | "[" | ".")));
        let assigned_tagged = after_tag.is_some_and(|x| matches!(x, Tok::Op("=" | "+=" | "-=" | "*=" | "/=")));
        if !assigned && !assigned_tagged {
            return Sym::Callable(name);
        }
    }
    let func = n.func_of[i];
    let scope = match func {
        Some(f) if n.locals.get(&f).is_some_and(|l| l.contains(&name)) => Scope::Func(f),
        _ if n.globals.contains(&name) => Scope::Global,
        Some(f) => Scope::Func(f),
        None => Scope::Main,
    };
    Sym::Var(scope, name)
}

fn taken(t: &[T], n: &Names, sym: &Sym, new: &str, builtin: &dyn Fn(&str) -> bool) -> bool {
    let idents = || (0..t.len()).filter(|i| matches!(t[*i].tok, Tok::Ident(_)));
    match sym {
        Sym::Var(scope, _) => {
            n.globals.contains(new)
                || idents().any(|i| match classify(t, n, i, builtin) {
                    Sym::Var(s, x) => x == new && (*scope == Scope::Global || s == Scope::Global || s == *scope),
                    _ => false,
                })
        }
        Sym::Callable(_) => n.callables.contains(new),
        Sym::Type(_) => n.types.contains(new) || n.enums.contains_key(new),
        Sym::Member(_) => n.members.contains_key(new),
        Sym::Label(_) => idents().any(|i| classify(t, n, i, builtin) == Sym::Label(new.to_string())),
        Sym::EnumMember(e, _) => n.enums.get(e).is_some_and(|m| m.contains(new)),
    }
}

/// Everything to change to rename what is at `offset` of `files[file]` to
/// `new`. Offsets are bytes of text with `\n` line ends. `builtin` says
/// whether a lower-case name is a command or constant of the host.
pub fn rename(files: &[&str], file: usize, offset: usize, new: &str, builtin: &dyn Fn(&str) -> bool) -> Result<Vec<Edit>, String> {
    let t = tokens(files);
    let n = collect(&t);
    let at = (0..t.len())
        .find(|&i| t[i].file == file && t[i].start <= offset && offset <= t[i].end && matches!(t[i].tok, Tok::Ident(_)))
        .ok_or("Put the caret on a name to rename it")?;
    let sym = classify(&t, &n, at, builtin);
    let old = name_at(&t, at);
    let built_in = || format!("{old} is built in, so it cannot be renamed");
    match &sym {
        Sym::Callable(x) if !n.callables.contains(x) => return Err(built_in()),
        Sym::Var(s, x) if *s != Scope::Global && builtin(x) => return Err(built_in()),
        Sym::Type(x) if !n.types.contains(x) && !n.enums.contains_key(x) => return Err(format!("There is no Type called {x}")),
        Sym::Member(x) => match n.members.get(x).copied().unwrap_or(0) {
            0 => return Err(format!("No Type declares {x}")),
            1 => {}
            _ => return Err(format!("More than one Type has a field or method called {x}, so a rename could change the wrong one. Use Find in Files.")),
        },
        _ => {}
    }
    let mut d = Diagnostics::default();
    let lexed = lex(new, 0, 0, &mut d);
    let valid = !d.has_errors() && matches!(lexed.first().map(|x| &x.tok), Some(Tok::Ident(_))) && lexed[0].span.end as usize == new.len() && lexed.len() == 3;
    if !valid {
        return Err(format!("'{new}' cannot be used as a name"));
    }
    let lower = new.to_ascii_lowercase();
    if lower != old {
        if builtin(&lower) {
            return Err(format!("{new} is already a command or constant"));
        }
        if taken(&t, &n, &sym, &lower, builtin) {
            return Err(format!("{new} is already used here, so renaming would mix the two up"));
        }
    }
    let mut edits: Vec<Edit> = (0..t.len())
        .filter(|&i| matches!(t[i].tok, Tok::Ident(_)) && classify(&t, &n, i, builtin) == sym)
        .map(|i| Edit { file: t[i].file, start: t[i].start, end: t[i].end })
        .collect();
    edits.sort();
    edits.dedup();
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_builtins(n: &str) -> bool {
        matches!(n, "print" | "graphics3d" | "key_escape")
    }

    /// Renames the name at the first `@` and returns the changed program.
    fn run(src: &str, new: &str) -> Result<String, String> {
        let offset = src.find('@').expect("mark the name with @");
        let text = src.replacen('@', "", 1);
        let edits = rename(&[&text], 0, offset, new, &no_builtins)?;
        let mut out = text;
        for e in edits.iter().rev() {
            out.replace_range(e.start..e.end, new);
        }
        Ok(out)
    }

    #[test]
    fn a_global_changes_everywhere_but_not_in_longer_names() {
        let src = "Global @score = 0\nFunction AddScore(n)\n score = score + n\nEnd Function\nAddScore(1) : Print SCORE ; score\nPrint \"score\"\n";
        assert_eq!(run(src, "points").unwrap(), "Global points = 0\nFunction AddScore(n)\n points = points + n\nEnd Function\nAddScore(1) : Print points ; score\nPrint \"score\"\n");
    }

    #[test]
    fn a_local_stays_in_its_function() {
        let src = "Function A(n)\n @n = n + 1\n Return n\nEnd Function\nFunction B(n)\n Return n\nEnd Function\nn = 5\n";
        assert_eq!(run(src, "count").unwrap(), "Function A(count)\n count = count + 1\n Return count\nEnd Function\nFunction B(n)\n Return n\nEnd Function\nn = 5\n");
    }

    #[test]
    fn an_implicit_main_variable_is_not_a_function_local() {
        let src = "@x = 1\nFunction F()\n x = 2\nEnd Function\nPrint x\n";
        assert_eq!(run(src, "y").unwrap(), "y = 1\nFunction F()\n x = 2\nEnd Function\nPrint y\n");
    }

    #[test]
    fn a_local_hides_a_global() {
        let src = "Global v\nFunction F()\n Local @v\n v = 1\nEnd Function\nv = 2\n";
        assert_eq!(run(src, "w").unwrap(), "Global v\nFunction F()\n Local w\n w = 1\nEnd Function\nv = 2\n");
    }

    #[test]
    fn functions_and_variables_are_separate() {
        let src = "Function @Score()\n Return 1\nEnd Function\nscore = Score()\nScore\n";
        assert_eq!(run(src, "Total").unwrap(), "Function Total()\n Return 1\nEnd Function\nscore = Total()\nTotal\n");
    }

    #[test]
    fn tags_stay_where_they_are() {
        assert_eq!(run("@a# = 1.5\nPrint a#\n", "b").unwrap(), "b# = 1.5\nPrint b#\n");
        assert_eq!(run("Function @F%(x)\nEnd Function\ny = F%(1)\n", "G").unwrap(), "Function G%(x)\nEnd Function\ny = G%(1)\n");
    }

    #[test]
    fn arrays_follow_their_dim() {
        assert_eq!(run("Dim @a(3)\na(1) = 2\nPrint a(1)\nb = 1\n", "grid").unwrap(), "Dim grid(3)\ngrid(1) = 2\nPrint grid(1)\nb = 1\n");
    }

    #[test]
    fn types_are_renamed_in_every_form() {
        let src = "Type @Enemy\n Field hp\nEnd Type\nFunction Spawn.Enemy()\n e.Enemy = New Enemy\n Return e\nEnd Function\nFor e.Enemy = Each Enemy\nNext\nx.Enemy = First Enemy\ny As Enemy = Last Enemy\nh = Handle(x) : z.Enemy = Object.Enemy(h)\nDelete Each Enemy\nFor Each q In Enemy : Next\nenemy = 3\n";
        let got = run(src, "Foe").unwrap();
        assert_eq!(got.matches("Foe").count(), 14, "{got}");
        assert!(got.ends_with("enemy = 3\n"), "{got}");
    }

    #[test]
    fn a_field_is_renamed_through_every_object() {
        let src = "Type T\n Field @hp, mana\n Method Heal()\n  Self\\hp = 1\n End Method\nEnd Type\na.T = New T\na\\hp = 2\nhp = 3\n";
        assert_eq!(run(src, "health").unwrap(), "Type T\n Field health, mana\n Method Heal()\n  Self\\health = 1\n End Method\nEnd Type\na.T = New T\na\\health = 2\nhp = 3\n");
    }

    #[test]
    fn a_field_two_types_share_is_refused() {
        let e = run("Type A\n Field @x\nEnd Type\nType B\n Field x\nEnd Type\n", "y").unwrap_err();
        assert!(e.contains("More than one Type"), "{e}");
    }

    #[test]
    fn methods_are_members() {
        let src = "Type S\n Method @Go(d)\n End Method\nEnd Type\ns.S = New S\ns\\Go(1)\nGo = 1\n";
        assert_eq!(run(src, "Run").unwrap(), "Type S\n Method Run(d)\n End Method\nEnd Type\ns.S = New S\ns\\Run(1)\nGo = 1\n");
    }

    #[test]
    fn labels_are_separate_from_variables() {
        let src = ".@top\ntop = 1\nGoto top\nGosub top\nRestore top\n";
        assert_eq!(run(src, "start").unwrap(), ".start\ntop = 1\nGoto start\nGosub start\nRestore start\n");
    }

    #[test]
    fn enums_and_their_members() {
        let src = "Enum @State\n Idle\n Run = 10\nEnd Enum\nLocal s As State = State.Run\nstate = 1\n";
        assert_eq!(run(src, "Mode").unwrap(), "Enum Mode\n Idle\n Run = 10\nEnd Enum\nLocal s As Mode = Mode.Run\nstate = 1\n");
        let src = "Enum State\n @Run = 10\nEnd Enum\nx = State.Run\nrun = 2\n";
        assert_eq!(run(src, "Go").unwrap(), "Enum State\n Go = 10\nEnd Enum\nx = State.Go\nrun = 2\n");
    }

    #[test]
    fn holes_of_interpolated_strings_are_code() {
        let src = "@n = 1\nPrint $\"n is {n} and {{n}}\"\n";
        assert_eq!(run(src, "count").unwrap(), "count = 1\nPrint $\"count is {count} and {{n}}\"\n".replace("count is", "n is"));
    }

    #[test]
    fn a_hole_does_not_end_the_statement() {
        let src = "Function F(@a)\n x = $\"{a}\" + Str(a)\n Return a\nEnd Function\n";
        assert_eq!(run(src, "b").unwrap(), "Function F(b)\n x = $\"{b}\" + Str(b)\n Return b\nEnd Function\n");
    }

    #[test]
    fn across_included_files() {
        let main = "Include \"lib.nb\"\nGlobal score\nscore = Bump(score)\n";
        let lib = "Function Bump(n)\n score = score + n\n Return score\nEnd Function\n";
        let at = main.find("score").unwrap();
        let edits = rename(&[main, lib], 0, at, "points", &no_builtins).unwrap();
        assert_eq!(edits.iter().filter(|e| e.file == 0).count(), 3);
        assert_eq!(edits.iter().filter(|e| e.file == 1).count(), 3);
    }

    #[test]
    fn built_ins_and_bad_names_are_refused() {
        assert!(run("@Print 1\n", "Say").unwrap_err().contains("built in"));
        assert!(run("x = @KEY_ESCAPE\n", "K").unwrap_err().contains("built in"));
        assert!(run("x = @Undefined(1)\n", "K").unwrap_err().contains("built in"));
        assert!(run("; nothing @here\n", "K").unwrap_err().contains("caret"));
        for bad in ["1a", "a b", "If", "a-b", "", "a#"] {
            assert!(run("@x = 1\n", bad).is_err(), "{bad}");
        }
        assert!(run("@x = 1\n", "Print").unwrap_err().contains("already"));
    }

    #[test]
    fn a_name_already_in_use_is_refused() {
        assert!(run("@a = 1\nb = 2\n", "b").unwrap_err().contains("already used"));
        assert!(run("Global @a\nFunction F()\n b = 1\nEnd Function\n", "b").is_err());
        assert!(run("Global b\nFunction F()\n @a = 1\nEnd Function\n", "b").is_err());
        assert!(run("Function @A()\nEnd Function\nFunction B()\nEnd Function\n", "b").is_err());
        assert!(run("Type @A\nEnd Type\nType B\nEnd Type\n", "B").is_err());
        // another function's local is no clash
        assert!(run("Function F()\n @a = 1\nEnd Function\nFunction G()\n b = 1\nEnd Function\n", "b").is_ok());
    }

    #[test]
    fn only_the_spelling_can_change() {
        assert_eq!(run("@score = 1\nPrint SCORE\n", "Score").unwrap(), "Score = 1\nPrint Score\n");
    }

    #[test]
    fn finds_includes() {
        assert_eq!(includes("Include \"a.nb\"\r\n; Include \"no.nb\"\nImport \"b.bb\"\n"), vec!["a.nb", "b.bb"]);
    }
}
