//! Recursive-descent parser.
//!
//! The statement and expression grammar mirrors Blitz3D's `parser.cpp`
//! (including its operator precedence, optional parentheses around command
//! arguments and `Dim`-before-use array disambiguation). NeptuneBASIC
//! extensions: `As` annotations, compound assignment, `Continue`,
//! `AndAlso`/`OrElse`, `Try`/`Catch`/`Throw`, type methods with `Self`,
//! string interpolation and `Import`.

use std::collections::HashSet;

use crate::ast::*;
use crate::diag::{Diagnostics, FileId, SourceMap, Span};
use crate::lexer::{lex, Kw, Tok, Token};

/// Supplies the text of `Include`/`Import`ed files.
pub trait SourceLoader {
    /// Resolves `path` (as written in source) relative to the including file
    /// `from`. Returns a canonical name used for include-once checks and the text.
    fn load(&mut self, path: &str, from: &str) -> Result<(String, String), String>;
}

/// Loader that reads from the file system.
pub struct FsLoader;

impl SourceLoader for FsLoader {
    fn load(&mut self, path: &str, from: &str) -> Result<(String, String), String> {
        let base = std::path::Path::new(from).parent().unwrap_or(std::path::Path::new("."));
        let full = base.join(path);
        let canon = full.canonicalize().unwrap_or(full.clone());
        let text = std::fs::read_to_string(&full).map_err(|e| format!("unable to open include file '{}': {e}", full.display()))?;
        Ok((canon.to_string_lossy().into_owned(), text))
    }
}

/// Loader that refuses all includes (useful for tests and embedding).
pub struct NoLoader;

impl SourceLoader for NoLoader {
    fn load(&mut self, path: &str, _: &str) -> Result<(String, String), String> {
        Err(format!("include of '{path}' is not available"))
    }
}

/// How much nesting one program may use, in units of roughly equal stack use
/// across the compiler's passes. Beyond this the parser reports an error, so
/// [`crate::compile`]'s stack can never run out.
const MAX_DEPTH: u32 = 4000;
/// A statement, or a bracketed or argument expression (about 200 levels).
const NEST_COST: u32 = 16;
/// One `ElseIf` arm, nested in the tree but not in the source (about 1000 arms).
const ELSEIF_COST: u32 = 4;
/// A prefix operator.
const UNARY_COST: u32 = 4;
/// One link of a chain: `a + b + c`, `a\b\c` or an interpolation hole.
const LINK_COST: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Prog,
    Block,
    Line,
}

struct Shared<'a> {
    program: Program,
    arrays: HashSet<String>,
    included: HashSet<String>,
    sources: &'a mut SourceMap,
    loader: &'a mut dyn SourceLoader,
    diags: &'a mut Diagnostics,
    /// Nesting used so far, see [`MAX_DEPTH`].
    depth: u32,
    /// Index of the "too deep" error, once reported.
    too_deep: Option<usize>,
}

struct Parser<'s, 'a> {
    toks: Vec<Token>,
    pos: usize,
    file: FileId,
    sh: &'s mut Shared<'a>,
    /// Inside a `Type` method body (allows `Self`).
    in_method: bool,
}

/// Raised to abandon the current statement after an error has been recorded.
struct Bail;
type PResult<T> = Result<T, Bail>;

/// Parses the main file `file` (already added to `sources`) and everything it includes.
pub fn parse_program(sources: &mut SourceMap, file: FileId, loader: &mut dyn SourceLoader, diags: &mut Diagnostics) -> Program {
    let text = sources.file(file).text.clone();
    let name = sources.file(file).name.clone();
    let toks = lex(&text, file, 0, diags);
    let mut sh = Shared {
        program: Program::default(),
        arrays: HashSet::new(),
        included: HashSet::from([name]),
        sources,
        loader,
        diags,
        depth: 0,
        too_deep: None,
    };
    let mut main = Vec::new();
    {
        let mut p = Parser { toks, pos: 0, file, sh: &mut sh, in_method: false };
        p.stmt_seq(&mut main, Scope::Prog);
        if !matches!(p.peek(), Tok::Eof) {
            p.unexpected("end of file");
        }
    }
    // Everything after "too deep" is noise from the blocks left unfinished.
    if let Some(n) = sh.too_deep {
        sh.diags.items.truncate(n + 1);
    }
    sh.program.main = main;
    sh.program
}

fn is_term(t: &Tok) -> bool {
    matches!(t, Tok::Newline | Tok::Op(":") | Tok::Eof)
}

impl<'s, 'a> Parser<'s, 'a> {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn peek_at(&self, n: usize) -> &Tok {
        let i = (self.pos + n).min(self.toks.len() - 1);
        &self.toks[i].tok
    }
    fn span(&self) -> Span {
        self.toks[self.pos].span
    }
    fn prev_span(&self) -> Span {
        self.toks[self.pos.saturating_sub(1)].span
    }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }
    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Tok::Op(o) if *o == op)
    }
    fn is_kw(&self, kw: Kw) -> bool {
        matches!(self.peek(), Tok::Kw(k) if *k == kw)
    }
    fn eat_op(&mut self, op: &str) -> bool {
        if self.is_op(op) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn eat_kw(&mut self, kw: Kw) -> bool {
        if self.is_kw(kw) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.sh.diags.error(span, msg);
    }

    /// Uses up nesting budget, failing once when there is none left.
    fn spend(&mut self, cost: u32) -> PResult<()> {
        self.sh.depth += cost;
        if self.sh.depth <= MAX_DEPTH {
            return Ok(());
        }
        if self.sh.too_deep.is_none() {
            self.sh.too_deep = Some(self.sh.diags.items.len());
            let sp = self.span();
            self.error(sp, "code is nested too deeply, or this statement is too long");
        }
        Err(Bail)
    }

    /// Runs `f` one nesting level down.
    fn nested<T>(&mut self, cost: u32, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        let r = self.spend(cost).and_then(|()| f(self));
        self.sh.depth -= cost;
        r
    }

    /// Builds `lhs op rhs`, one more link in a chain of operators.
    fn link(&mut self, op: BinOp, lhs: Expr, rhs: Expr) -> PResult<Expr> {
        self.spend(LINK_COST)?;
        Ok(bin(op, lhs, rhs))
    }

    fn unexpected(&mut self, expected: &str) -> Bail {
        let msg = match self.peek() {
            Tok::Kw(Kw::Next) => "'Next' without 'For'".to_string(),
            Tok::Kw(Kw::Wend) => "'Wend' without 'While'".to_string(),
            Tok::Kw(Kw::Else | Kw::ElseIf) => "'Else' without 'If'".to_string(),
            Tok::Kw(Kw::EndIf) => "'EndIf' without 'If'".to_string(),
            Tok::Kw(Kw::EndFunction) => "'End Function' without 'Function'".to_string(),
            Tok::Kw(Kw::Until) => "'Until' without 'Repeat'".to_string(),
            Tok::Kw(Kw::Forever) => "'Forever' without 'Repeat'".to_string(),
            Tok::Kw(Kw::Case) => "'Case' without 'Select'".to_string(),
            Tok::Kw(Kw::EndSelect) => "'End Select' without 'Select'".to_string(),
            Tok::Kw(Kw::Catch) => "'Catch' without 'Try'".to_string(),
            Tok::Kw(Kw::EndTry) => "'End Try' without 'Try'".to_string(),
            t => format!("expected {expected}, found {}", describe(t)),
        };
        let sp = self.span();
        self.error(sp, msg);
        Bail
    }

    fn expect_op(&mut self, op: &str) -> PResult<()> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("'{op}'")))
        }
    }
    fn expect_kw(&mut self, kw: Kw, what: &str) -> PResult<()> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(self.unexpected(what))
        }
    }

    fn ident(&mut self) -> PResult<Ident> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let span = self.bump().span;
                Ok(Ident { name, span })
            }
            _ => Err(self.unexpected("identifier")),
        }
    }

    /// Skips tokens up to the end of the current line (error recovery).
    fn recover(&mut self) {
        while !matches!(self.peek(), Tok::Newline | Tok::Eof) {
            self.bump();
        }
    }

    // ------------------------------------------------------------------
    // statements

    fn stmt_seq(&mut self, out: &mut Vec<Stmt>, scope: Scope) {
        loop {
            while self.is_op(":") || (scope != Scope::Line && matches!(self.peek(), Tok::Newline)) {
                self.bump();
            }
            let start = self.span();
            // a finished statement's operator chains are no longer on the stack
            let depth = self.sh.depth;
            let result = self.nested(NEST_COST, |p| p.stmt(out, scope));
            self.sh.depth = depth;
            match result {
                Ok(true) => {}
                Ok(false) => return,
                Err(Bail) => {
                    self.recover();
                    if scope == Scope::Line || matches!(self.peek(), Tok::Eof) {
                        return;
                    }
                }
            }
            if let Some(last) = out.last_mut() {
                if last.span == Span::default() {
                    last.span = start.to(self.prev_span());
                }
            }
            if scope == Scope::Line && !self.is_op(":") {
                return;
            }
            if !is_term(self.peek()) && !self.is_kw(Kw::Else) && !self.is_kw(Kw::ElseIf) {
                // statements must be separated
                let _ = self.unexpected("end of statement");
                self.recover();
            }
        }
    }

    fn push(&self, out: &mut Vec<Stmt>, kind: StmtKind, start: Span) {
        out.push(Stmt { kind, span: start.to(self.prev_span()) });
    }

    /// Parses one statement. Returns `Ok(false)` if the current token does not start a statement.
    fn stmt(&mut self, out: &mut Vec<Stmt>, scope: Scope) -> PResult<bool> {
        let start = self.span();
        let tok = self.peek().clone();
        match tok {
            Tok::Kw(Kw::Include | Kw::Import) => {
                self.bump();
                let path = match self.peek().clone() {
                    Tok::Str(s) => {
                        self.bump();
                        s
                    }
                    _ => return Err(self.unexpected("include file name")),
                };
                self.include(&path, start, out, scope)?;
            }
            Tok::Ident(_) => {
                let s = self.ident_stmt()?;
                self.push(out, s, start);
            }
            Tok::Kw(Kw::SelfKw) => {
                let e = self.unary()?;
                let s = self.finish_var_stmt(e)?;
                self.push(out, s, start);
            }
            Tok::Kw(Kw::If) => {
                self.bump();
                let s = self.if_stmt()?;
                self.eat_kw(Kw::EndIf);
                self.push(out, s, start);
            }
            Tok::Kw(Kw::While) => {
                self.bump();
                let cond = self.expr()?;
                let mut body = Vec::new();
                self.stmt_seq(&mut body, Scope::Block);
                self.expect_kw(Kw::Wend, "'Wend'")?;
                self.push(out, StmtKind::While { cond, body }, start);
            }
            Tok::Kw(Kw::Repeat) => {
                self.bump();
                let mut body = Vec::new();
                self.stmt_seq(&mut body, Scope::Block);
                let until = if self.eat_kw(Kw::Until) {
                    Some(self.expr()?)
                } else if self.eat_kw(Kw::Forever) {
                    None
                } else {
                    return Err(self.unexpected("'Until' or 'Forever'"));
                };
                self.push(out, StmtKind::Repeat { body, until }, start);
            }
            Tok::Kw(Kw::Select) => {
                self.bump();
                let expr = self.expr()?;
                let mut cases = Vec::new();
                let mut default = None;
                loop {
                    while is_term(self.peek()) && !matches!(self.peek(), Tok::Eof) {
                        self.bump();
                    }
                    if self.eat_kw(Kw::Case) {
                        let vals = self.expr_list()?;
                        let mut body = Vec::new();
                        self.stmt_seq(&mut body, Scope::Block);
                        cases.push((vals, body));
                    } else if self.eat_kw(Kw::Default) {
                        let mut body = Vec::new();
                        self.stmt_seq(&mut body, Scope::Block);
                        default = Some(body);
                    } else if self.eat_kw(Kw::EndSelect) {
                        break;
                    } else {
                        return Err(self.unexpected("'Case', 'Default' or 'End Select'"));
                    }
                }
                self.push(out, StmtKind::Select { expr, cases, default }, start);
            }
            Tok::Kw(Kw::For) => {
                self.bump();
                if self.eat_kw(Kw::Each) {
                    // NeptuneBASIC: For Each x In <Type | array>
                    let var = self.ident()?;
                    self.expect_kw(Kw::In, "'In'")?;
                    let iter = self.expr()?;
                    let mut body = Vec::new();
                    self.stmt_seq(&mut body, Scope::Block);
                    self.expect_kw(Kw::Next, "'Next'")?;
                    self.eat_next_var();
                    self.push(out, StmtKind::ForIn { var, iter, body }, start);
                    return Ok(true);
                }
                let var = self.var()?;
                self.expect_op("=")?;
                if self.eat_kw(Kw::Each) {
                    let ty = self.ident()?;
                    let mut body = Vec::new();
                    self.stmt_seq(&mut body, Scope::Block);
                    self.expect_kw(Kw::Next, "'Next'")?;
                    self.eat_next_var();
                    self.push(out, StmtKind::ForEach { var, ty, body }, start);
                } else {
                    let from = self.expr()?;
                    self.expect_kw(Kw::To, "'To'")?;
                    let to = self.expr()?;
                    let step = if self.eat_kw(Kw::Step) { Some(self.expr()?) } else { None };
                    let mut body = Vec::new();
                    self.stmt_seq(&mut body, Scope::Block);
                    self.expect_kw(Kw::Next, "'Next'")?;
                    self.eat_next_var();
                    self.push(out, StmtKind::For { var, from, to, step, body }, start);
                }
            }
            Tok::Kw(Kw::Exit) => {
                self.bump();
                self.push(out, StmtKind::Exit, start);
            }
            Tok::Kw(Kw::Continue) => {
                self.bump();
                self.push(out, StmtKind::Continue, start);
            }
            Tok::Kw(Kw::Goto) => {
                self.bump();
                let l = self.ident()?;
                self.push(out, StmtKind::Goto(l), start);
            }
            Tok::Kw(Kw::Gosub) => {
                self.bump();
                let l = self.ident()?;
                self.push(out, StmtKind::Gosub(l), start);
            }
            Tok::Kw(Kw::Return) => {
                self.bump();
                let e = if is_term(self.peek()) || self.is_kw(Kw::Else) { None } else { Some(self.expr()?) };
                self.push(out, StmtKind::Return(e), start);
            }
            Tok::Kw(Kw::Delete) => {
                self.bump();
                if self.eat_kw(Kw::Each) {
                    let t = self.ident()?;
                    self.push(out, StmtKind::DeleteEach(t), start);
                } else {
                    let e = self.expr()?;
                    self.push(out, StmtKind::Delete(e), start);
                }
            }
            Tok::Kw(Kw::Insert) => {
                self.bump();
                let obj = self.expr()?;
                let before = if self.eat_kw(Kw::Before) {
                    true
                } else if self.eat_kw(Kw::After) {
                    false
                } else {
                    return Err(self.unexpected("'Before' or 'After'"));
                };
                let target = self.expr()?;
                self.push(out, StmtKind::Insert { obj, before, target }, start);
            }
            Tok::Kw(Kw::Read) => {
                self.bump();
                let mut vars = vec![self.var()?];
                while self.eat_op(",") {
                    vars.push(self.var()?);
                }
                self.push(out, StmtKind::Read(vars), start);
            }
            Tok::Kw(Kw::Restore) => {
                self.bump();
                let l = if matches!(self.peek(), Tok::Ident(_)) { Some(self.ident()?) } else { None };
                self.push(out, StmtKind::Restore(l), start);
            }
            Tok::Kw(Kw::Data) => {
                self.bump();
                if scope != Scope::Prog {
                    self.error(start, "'Data' can only appear in main program");
                }
                loop {
                    let expr = self.expr()?;
                    self.sh.program.data.push(DataItem { expr });
                    if !self.eat_op(",") {
                        break;
                    }
                }
            }
            Tok::Kw(Kw::Type) => {
                self.bump();
                if scope != Scope::Prog {
                    self.error(start, "'Type' can only appear in main program");
                }
                let t = self.type_decl(start)?;
                self.sh.program.types.push(t);
            }
            Tok::Kw(Kw::Enum) => {
                self.bump();
                if scope != Scope::Prog {
                    self.error(start, "'Enum' can only appear in main program");
                }
                let name = self.ident()?;
                let mut members = Vec::new();
                loop {
                    while is_term(self.peek()) && !matches!(self.peek(), Tok::Eof) || self.is_op(",") {
                        self.bump();
                    }
                    if self.eat_kw(Kw::EndEnum) {
                        break;
                    }
                    let m = self.ident()?;
                    let v = if self.eat_op("=") { Some(self.expr()?) } else { None };
                    members.push((m, v));
                }
                self.sh.program.enums.push(EnumDecl { name, members });
            }
            Tok::Kw(Kw::Const) => {
                self.bump();
                if scope != Scope::Prog {
                    self.error(start, "'Const' can only appear in main program");
                }
                loop {
                    let d = self.var_decl(DeclKind::Const)?;
                    self.sh.program.consts.push(d);
                    if !self.eat_op(",") {
                        break;
                    }
                }
            }
            Tok::Kw(Kw::Function) => {
                self.bump();
                if scope != Scope::Prog {
                    self.error(start, "'Function' can only appear in main program");
                }
                let f = self.func_decl(start, Kw::EndFunction)?;
                self.sh.program.funcs.push(f);
            }
            Tok::Kw(Kw::Dim) => {
                self.bump();
                loop {
                    let s = self.span();
                    let name = self.ident()?;
                    let tag = self.tag()?;
                    self.expect_op("(")?;
                    let dims = self.expr_list()?;
                    self.expect_op(")")?;
                    if dims.is_empty() {
                        self.error(s, "can't have a 0 dimensional array");
                    }
                    self.sh.arrays.insert(name.name.clone());
                    self.push(out, StmtKind::Dim { name, tag, dims }, s);
                    if !self.eat_op(",") {
                        break;
                    }
                }
            }
            Tok::Kw(k @ (Kw::Local | Kw::Global)) => {
                self.bump();
                if k == Kw::Global && scope != Scope::Prog {
                    self.error(start, "'Global' can only appear in main program");
                }
                let kind = if k == Kw::Local { DeclKind::Local } else { DeclKind::Global };
                loop {
                    let s = self.span();
                    let d = self.var_decl(kind)?;
                    self.push(out, StmtKind::Decl(kind, d), s);
                    if !self.eat_op(",") {
                        break;
                    }
                }
            }
            Tok::Op(".") => {
                self.bump();
                let l = self.ident()?;
                let idx = self.sh.program.data.len();
                self.sh.program.data_labels.push((l.name.clone(), idx));
                self.push(out, StmtKind::Label(l), start);
            }
            Tok::Kw(Kw::Try) => {
                self.bump();
                let mut body = Vec::new();
                self.stmt_seq(&mut body, Scope::Block);
                let mut var = None;
                let mut handler = Vec::new();
                if self.eat_kw(Kw::Catch) {
                    if !is_term(self.peek()) {
                        var = Some(self.var()?);
                    }
                    self.stmt_seq(&mut handler, Scope::Block);
                }
                self.expect_kw(Kw::EndTry, "'Catch' or 'End Try'")?;
                self.push(out, StmtKind::Try { body, var, handler }, start);
            }
            Tok::Kw(Kw::Throw) => {
                self.bump();
                let e = self.expr()?;
                self.push(out, StmtKind::Throw(e), start);
            }
            Tok::Kw(Kw::End) => {
                self.bump();
                self.push(out, StmtKind::End, start);
            }
            Tok::Kw(Kw::Stop) => {
                self.bump();
                self.push(out, StmtKind::Stop, start);
            }
            Tok::Kw(Kw::Strict) => {
                self.bump();
                self.sh.program.strict = true;
            }
            Tok::Kw(Kw::Classic) => {
                self.bump();
                self.sh.program.classic = true;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn eat_next_var(&mut self) {
        if matches!(self.peek(), Tok::Ident(_)) {
            self.bump();
            let _ = self.tag();
        }
    }

    fn include(&mut self, path: &str, at: Span, out: &mut Vec<Stmt>, scope: Scope) -> PResult<()> {
        let from = self.sh.sources.file(self.file).name.clone();
        let (name, text) = match self.sh.loader.load(path, &from) {
            Ok(r) => r,
            Err(e) => {
                self.error(at, e);
                return Err(Bail);
            }
        };
        if !self.sh.included.insert(name.clone()) {
            return Ok(());
        }
        let file = self.sh.sources.add(name, text);
        let text = self.sh.sources.file(file).text.clone();
        let toks = lex(&text, file, 0, self.sh.diags);
        let mut sub = Parser { toks, pos: 0, file, sh: &mut *self.sh, in_method: false };
        sub.stmt_seq(out, scope);
        if !matches!(sub.peek(), Tok::Eof) {
            let _ = sub.unexpected("end of file");
        }
        Ok(())
    }

    /// Statement starting with an identifier: command call or assignment.
    fn ident_stmt(&mut self) -> PResult<StmtKind> {
        let name = self.ident()?;
        let tag = self.tag()?;
        let is_array = self.sh.arrays.contains(&name.name);
        let assignish = self.is_op("=")
            || self.is_op("\\")
            || self.is_op("[")
            || matches!(self.peek(), Tok::Op("+=" | "-=" | "*=" | "/="));
        if !is_array && !assignish {
            // command / function call statement
            let args = if self.is_op("(") {
                // Blitz: optional parentheses around the whole argument list
                let mut depth = 0;
                let mut k = 0;
                loop {
                    match self.peek_at(k) {
                        Tok::Op("(") => depth += 1,
                        Tok::Op(")") => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        t if is_term(t) => {
                            let sp = self.span();
                            self.error(sp, "mismatched brackets");
                            return Err(Bail);
                        }
                        _ => {}
                    }
                    k += 1;
                }
                let after = self.peek_at(k + 1).clone();
                if is_term(&after) || matches!(after, Tok::Kw(Kw::Else | Kw::ElseIf)) {
                    self.bump();
                    let a = self.expr_list()?;
                    self.expect_op(")")?;
                    a
                } else {
                    self.expr_list()?
                }
            } else {
                self.expr_list()?
            };
            let span = name.span.to(self.prev_span());
            return Ok(StmtKind::Expr(Expr { kind: ExprKind::Call { name, tag, args }, span }));
        }
        let base = self.var_from(name, tag)?;
        self.finish_var_stmt(base)
    }

    /// Given a parsed postfix expression at statement start, produce an
    /// assignment or a method-call statement.
    fn finish_var_stmt(&mut self, e: Expr) -> PResult<StmtKind> {
        if let ExprKind::MethodCall { .. } = e.kind {
            return Ok(StmtKind::Expr(e));
        }
        let ExprKind::Var(var) = e.kind else {
            return Err(self.unexpected("variable assignment"));
        };
        let op = match self.peek() {
            Tok::Op("=") => None,
            Tok::Op("+=") => Some(BinOp::Add),
            Tok::Op("-=") => Some(BinOp::Sub),
            Tok::Op("*=") => Some(BinOp::Mul),
            Tok::Op("/=") => Some(BinOp::Div),
            _ => return Err(self.unexpected("variable assignment")),
        };
        self.bump();
        let rhs = self.expr()?;
        Ok(match op {
            None => StmtKind::Assign(*var, rhs),
            Some(op) => StmtKind::CompoundAssign(*var, op, rhs),
        })
    }

    fn if_stmt(&mut self) -> PResult<StmtKind> {
        let depth = self.sh.depth;
        let cond = self.expr()?;
        self.sh.depth = depth;
        self.eat_kw(Kw::Then);
        let block = is_term(self.peek());
        let scope = if block { Scope::Block } else { Scope::Line };
        let mut then = Vec::new();
        self.stmt_seq(&mut then, scope);
        let els = if self.is_kw(Kw::ElseIf) {
            let s = self.span();
            self.bump();
            let inner = self.nested(ELSEIF_COST, Self::if_stmt)?;
            Some(vec![Stmt { kind: inner, span: s.to(self.prev_span()) }])
        } else if self.eat_kw(Kw::Else) {
            let mut e = Vec::new();
            self.stmt_seq(&mut e, scope);
            Some(e)
        } else {
            None
        };
        if block {
            if !self.is_kw(Kw::EndIf) {
                return Err(self.unexpected("'EndIf'"));
            }
        } else if !matches!(self.peek(), Tok::Newline | Tok::Eof | Tok::Kw(Kw::EndIf)) {
            return Err(self.unexpected("end of line"));
        }
        Ok(StmtKind::If { cond, then, els })
    }

    // ------------------------------------------------------------------
    // declarations

    fn tag(&mut self) -> PResult<Option<TypeAnn>> {
        // a tag must directly follow the identifier (no whitespace)
        if self.toks[self.pos].space_before {
            return Ok(None);
        }
        Ok(match self.peek() {
            Tok::Op("%") => {
                self.bump();
                Some(TypeAnn::Int)
            }
            Tok::Op("#") => {
                self.bump();
                Some(TypeAnn::Float)
            }
            Tok::Op("$") => {
                self.bump();
                Some(TypeAnn::Str)
            }
            Tok::Op(".") if matches!(self.peek_at(1), Tok::Ident(_)) => {
                self.bump();
                Some(TypeAnn::Obj(self.ident()?.name))
            }
            _ => None,
        })
    }

    /// `As <type>` clause.
    fn as_clause(&mut self) -> PResult<Option<TypeAnn>> {
        if !self.eat_kw(Kw::As) {
            return Ok(None);
        }
        let t = match self.peek().clone() {
            Tok::Kw(Kw::Int) => TypeAnn::Int,
            Tok::Kw(Kw::Float) => TypeAnn::Float,
            Tok::Kw(Kw::Str) => TypeAnn::Str,
            Tok::Kw(Kw::Variant) => TypeAnn::Variant,
            Tok::Ident(n) => match n.as_str() {
                "integer" => TypeAnn::Int,
                "string" => TypeAnn::Str,
                "bool" | "boolean" => TypeAnn::Int,
                "any" => TypeAnn::Variant,
                "single" | "double" => TypeAnn::Float,
                _ => TypeAnn::Obj(n),
            },
            _ => return Err(self.unexpected("type name")),
        };
        self.bump();
        Ok(Some(t))
    }

    fn var_decl(&mut self, kind: DeclKind) -> PResult<VarDecl> {
        let name = self.ident()?;
        let tag = self.tag()?;
        let mut vector = None;
        if self.eat_op("[") {
            if kind == DeclKind::Const {
                self.error(name.span, "arrays may not be constant");
            }
            vector = Some(self.expr()?);
            self.expect_op("]")?;
        }
        let as_ty = self.as_clause()?;
        let ty = match (tag, as_ty) {
            (Some(t), Some(a)) => {
                if t != a {
                    let sp = self.prev_span();
                    self.error(sp, "type tag and 'As' type disagree");
                }
                Some(t)
            }
            (t, a) => t.or(a),
        };
        let init = if vector.is_none() && self.eat_op("=") {
            Some(self.expr()?)
        } else {
            if kind == DeclKind::Const {
                self.error(name.span, "constants must be initialized");
            }
            None
        };
        let span = name.span.to(self.prev_span());
        Ok(VarDecl { name, ty, init, vector, span })
    }

    fn func_decl(&mut self, start: Span, end_kw: Kw) -> PResult<FuncDecl> {
        let name = self.ident()?;
        let tag = self.tag()?;
        self.expect_op("(")?;
        let mut params = Vec::new();
        if !self.is_op(")") {
            loop {
                params.push(self.var_decl(DeclKind::Param)?);
                if !self.eat_op(",") {
                    break;
                }
            }
        }
        self.expect_op(")")?;
        let ret = tag.or(self.as_clause()?);
        let span = start.to(self.prev_span());
        let mut body = Vec::new();
        self.stmt_seq(&mut body, Scope::Block);
        let what = if end_kw == Kw::EndMethod { "'End Method'" } else { "'End Function'" };
        self.expect_kw(end_kw, what)?;
        Ok(FuncDecl { name, ret, params, body, span })
    }

    fn type_decl(&mut self, start: Span) -> PResult<TypeDecl> {
        let name = self.ident()?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        loop {
            while is_term(self.peek()) && !matches!(self.peek(), Tok::Eof) {
                self.bump();
            }
            if self.eat_kw(Kw::Field) {
                loop {
                    fields.push(self.var_decl(DeclKind::Field)?);
                    if !self.eat_op(",") {
                        break;
                    }
                }
            } else if self.is_kw(Kw::Method) {
                let s = self.span();
                self.bump();
                self.in_method = true;
                let m = self.func_decl(s, Kw::EndMethod);
                self.in_method = false;
                methods.push(m?);
            } else if self.eat_kw(Kw::EndType) {
                break;
            } else {
                return Err(self.unexpected("'Field', 'Method' or 'End Type'"));
            }
        }
        Ok(TypeDecl { name, fields, methods, span: start })
    }

    // ------------------------------------------------------------------
    // variables

    fn var(&mut self) -> PResult<Var> {
        let name = self.ident()?;
        let tag = self.tag()?;
        let e = self.var_from(name, tag)?;
        match e.kind {
            ExprKind::Var(v) => Ok(*v),
            _ => {
                self.error(e.span, "expected a variable");
                Err(Bail)
            }
        }
    }

    /// Parses the remainder of a variable after its name/tag, including postfix chains.
    fn var_from(&mut self, name: Ident, tag: Option<TypeAnn>) -> PResult<Expr> {
        let base = if self.is_op("(") {
            self.bump();
            let index = self.expr_list()?;
            self.expect_op(")")?;
            let span = name.span.to(self.prev_span());
            Expr { kind: ExprKind::Var(Box::new(Var::Array { name, tag, index })), span }
        } else {
            let span = name.span;
            Expr { kind: ExprKind::Var(Box::new(Var::Ident { name, tag })), span }
        };
        self.postfix(base)
    }

    fn postfix(&mut self, mut e: Expr) -> PResult<Expr> {
        loop {
            if self.eat_op("\\") {
                self.spend(LINK_COST)?;
                let field = self.ident()?;
                if self.is_op("(") && !self.toks[self.pos].space_before {
                    self.bump();
                    let args = self.expr_list()?;
                    self.expect_op(")")?;
                    let span = e.span.to(self.prev_span());
                    e = Expr { kind: ExprKind::MethodCall { target: Box::new(e), method: field, args }, span };
                } else {
                    let tag = self.tag()?;
                    let span = e.span.to(field.span);
                    e = Expr { kind: ExprKind::Var(Box::new(Var::Field { target: e, field, tag })), span };
                }
            } else if self.is_op("[") && !self.toks[self.pos].space_before {
                self.spend(LINK_COST)?;
                self.bump();
                let index = self.expr()?;
                self.expect_op("]")?;
                let span = e.span.to(self.prev_span());
                e = Expr { kind: ExprKind::Var(Box::new(Var::Vector { target: e, index })), span };
            } else {
                return Ok(e);
            }
        }
    }

    // ------------------------------------------------------------------
    // expressions

    /// Comma separated, possibly empty, expression list.
    fn expr_list(&mut self) -> PResult<Vec<Expr>> {
        let mut v = Vec::new();
        if is_term(self.peek()) || self.is_op(")") || self.is_kw(Kw::Else) || self.is_kw(Kw::ElseIf) {
            return Ok(v);
        }
        loop {
            v.push(self.expr()?);
            if !self.eat_op(",") {
                return Ok(v);
            }
        }
    }

    pub(crate) fn expr(&mut self) -> PResult<Expr> {
        self.nested(NEST_COST, Self::expr0)
    }

    fn expr0(&mut self) -> PResult<Expr> {
        let mut lhs = self.not_expr()?;
        loop {
            let op = match self.peek() {
                Tok::Kw(Kw::AndAlso) => BinOp::AndAlso,
                Tok::Kw(Kw::OrElse) => BinOp::OrElse,
                _ => return Ok(lhs),
            };
            self.bump();
            let rhs = self.not_expr()?;
            lhs = self.link(op, lhs, rhs)?;
        }
    }

    /// Blitz: a leading `Not` applies to the whole And/Or expression.
    fn not_expr(&mut self) -> PResult<Expr> {
        if self.is_kw(Kw::Not) {
            let s = self.bump().span;
            let e = self.expr1()?;
            let span = s.to(e.span);
            return Ok(Expr { kind: ExprKind::Unary(UnOp::Not, Box::new(e)), span });
        }
        self.expr1()
    }

    fn expr1(&mut self) -> PResult<Expr> {
        let mut lhs = self.expr2()?;
        loop {
            let op = match self.peek() {
                Tok::Kw(Kw::And) => BinOp::And,
                Tok::Kw(Kw::Or) => BinOp::Or,
                Tok::Kw(Kw::Xor) => BinOp::Xor,
                _ => return Ok(lhs),
            };
            self.bump();
            // NeptuneBASIC: allow `a And Not b`
            let rhs = if self.is_kw(Kw::Not) {
                let s = self.bump().span;
                let e = self.expr2()?;
                let span = s.to(e.span);
                Expr { kind: ExprKind::Unary(UnOp::Not, Box::new(e)), span }
            } else {
                self.expr2()?
            };
            lhs = self.link(op, lhs, rhs)?;
        }
    }

    fn expr2(&mut self) -> PResult<Expr> {
        let mut lhs = self.expr3()?;
        loop {
            let op = match self.peek() {
                Tok::Op("=") => BinOp::Eq,
                Tok::Op("<>") => BinOp::Ne,
                Tok::Op("<") => BinOp::Lt,
                Tok::Op(">") => BinOp::Gt,
                Tok::Op("<=") => BinOp::Le,
                Tok::Op(">=") => BinOp::Ge,
                _ => return Ok(lhs),
            };
            self.bump();
            let rhs = self.expr3()?;
            lhs = self.link(op, lhs, rhs)?;
        }
    }

    fn expr3(&mut self) -> PResult<Expr> {
        let mut lhs = self.expr4()?;
        loop {
            let op = match self.peek() {
                Tok::Op("+") => BinOp::Add,
                Tok::Op("-") => BinOp::Sub,
                _ => return Ok(lhs),
            };
            self.bump();
            let rhs = self.expr4()?;
            lhs = self.link(op, lhs, rhs)?;
        }
    }

    fn expr4(&mut self) -> PResult<Expr> {
        let mut lhs = self.expr5()?;
        loop {
            let op = match self.peek() {
                Tok::Kw(Kw::Shl) => BinOp::Shl,
                Tok::Kw(Kw::Shr) => BinOp::Shr,
                Tok::Kw(Kw::Sar) => BinOp::Sar,
                _ => return Ok(lhs),
            };
            self.bump();
            let rhs = self.expr5()?;
            lhs = self.link(op, lhs, rhs)?;
        }
    }

    fn expr5(&mut self) -> PResult<Expr> {
        let mut lhs = self.expr6()?;
        loop {
            let op = match self.peek() {
                Tok::Op("*") => BinOp::Mul,
                Tok::Op("/") => BinOp::Div,
                Tok::Kw(Kw::Mod) => BinOp::Mod,
                _ => return Ok(lhs),
            };
            self.bump();
            let rhs = self.expr6()?;
            lhs = self.link(op, lhs, rhs)?;
        }
    }

    fn expr6(&mut self) -> PResult<Expr> {
        let mut lhs = self.unary()?;
        while self.eat_op("^") {
            let rhs = self.unary()?;
            lhs = self.link(BinOp::Pow, lhs, rhs)?;
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        self.nested(UNARY_COST, Self::unary0)
    }

    fn unary0(&mut self) -> PResult<Expr> {
        let start = self.span();
        let wrap = |p: &Self, kind: ExprKind| Expr { kind, span: start.to(p.prev_span()) };
        match self.peek().clone() {
            Tok::Kw(k @ (Kw::Int | Kw::Float | Kw::Str)) => {
                self.bump();
                let (to, tag) = match k {
                    Kw::Int => (CastTo::Int, "%"),
                    Kw::Float => (CastTo::Float, "#"),
                    _ => (CastTo::Str, "$"),
                };
                if !self.toks[self.pos].space_before {
                    self.eat_op(tag);
                }
                let e = self.unary()?;
                Ok(wrap(self, ExprKind::Cast(to, Box::new(e))))
            }
            Tok::Kw(Kw::Object) => {
                self.bump();
                self.eat_op(".");
                let t = self.ident()?;
                let e = self.unary()?;
                Ok(wrap(self, ExprKind::Object(t, Box::new(e))))
            }
            Tok::Kw(Kw::Handle) => {
                self.bump();
                let e = self.unary()?;
                Ok(wrap(self, ExprKind::Handle(Box::new(e))))
            }
            Tok::Kw(Kw::Before) => {
                self.bump();
                let e = self.unary()?;
                Ok(wrap(self, ExprKind::Before(Box::new(e))))
            }
            Tok::Kw(Kw::After) => {
                self.bump();
                let e = self.unary()?;
                Ok(wrap(self, ExprKind::After(Box::new(e))))
            }
            Tok::Op(o @ ("+" | "-" | "~")) => {
                self.bump();
                let e = self.unary()?;
                // fold negative literals directly
                if o == "-" {
                    match e.kind {
                        ExprKind::Int(v) => return Ok(wrap(self, ExprKind::Int(v.wrapping_neg()))),
                        ExprKind::Float(v) => return Ok(wrap(self, ExprKind::Float(-v))),
                        _ => {}
                    }
                }
                let op = match o {
                    "+" => UnOp::Plus,
                    "-" => UnOp::Neg,
                    _ => UnOp::BitNot,
                };
                Ok(wrap(self, ExprKind::Unary(op, Box::new(e))))
            }
            Tok::Kw(k @ (Kw::Abs | Kw::Sgn)) => {
                self.bump();
                let e = self.unary()?;
                let op = if k == Kw::Abs { UnOp::Abs } else { UnOp::Sgn };
                Ok(wrap(self, ExprKind::Unary(op, Box::new(e))))
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> PResult<Expr> {
        let start = self.span();
        let tok = self.peek().clone();
        let kind = match tok {
            Tok::Op("(") => {
                self.bump();
                let e = self.expr()?;
                self.expect_op(")")?;
                let e = Expr { kind: e.kind, span: start.to(self.prev_span()) };
                return self.postfix(e);
            }
            Tok::Kw(Kw::New) => {
                self.bump();
                ExprKind::New(self.ident()?)
            }
            Tok::Kw(Kw::First) => {
                self.bump();
                ExprKind::First(self.ident()?)
            }
            Tok::Kw(Kw::Last) => {
                self.bump();
                ExprKind::Last(self.ident()?)
            }
            Tok::Kw(Kw::Null) => {
                self.bump();
                ExprKind::Null
            }
            Tok::Kw(Kw::SelfKw) => {
                self.bump();
                if !self.in_method {
                    self.error(start, "'Self' can only be used inside a Method");
                }
                let e = Expr { kind: ExprKind::SelfRef, span: start };
                return self.postfix(e);
            }
            Tok::Int(v) => {
                self.bump();
                ExprKind::Int(v)
            }
            Tok::Float(v) => {
                self.bump();
                ExprKind::Float(v)
            }
            Tok::Str(s) => {
                self.bump();
                ExprKind::Str(s)
            }
            Tok::Interp(raw) => {
                let span = self.bump().span;
                ExprKind::Interp(self.interp_parts(&raw, span)?)
            }
            Tok::Kw(Kw::Pi) => {
                self.bump();
                ExprKind::Float(std::f32::consts::PI)
            }
            Tok::Kw(Kw::True) => {
                self.bump();
                ExprKind::Int(1)
            }
            Tok::Kw(Kw::False) => {
                self.bump();
                ExprKind::Int(0)
            }
            Tok::Ident(_) => {
                let name = self.ident()?;
                let tag = self.tag()?;
                if self.is_op("(") && !self.sh.arrays.contains(&name.name) {
                    self.bump();
                    let args = self.expr_list()?;
                    self.expect_op(")")?;
                    let e = Expr { kind: ExprKind::Call { name, tag, args }, span: start.to(self.prev_span()) };
                    return self.postfix(e);
                }
                return self.var_from(name, tag);
            }
            _ => return Err(self.unexpected("expression")),
        };
        Ok(Expr { kind, span: start.to(self.prev_span()) })
    }

    /// Splits `$"a {x} b"` into parts, parsing each hole as an expression.
    fn interp_parts(&mut self, raw: &str, span: Span) -> PResult<Vec<InterpPart>> {
        let mut parts = Vec::new();
        let mut lit = String::new();
        let bytes = raw.as_bytes();
        let content_start = span.start as usize + 2; // skip `$"`
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'{' if bytes.get(i + 1) == Some(&b'{') => {
                    lit.push('{');
                    i += 2;
                }
                b'}' if bytes.get(i + 1) == Some(&b'}') => {
                    lit.push('}');
                    i += 2;
                }
                b'{' => {
                    let close = matching_brace(raw, i);
                    let Some(close) = close else {
                        self.error(span, "unclosed '{' in interpolated string");
                        return Err(Bail);
                    };
                    if !lit.is_empty() {
                        parts.push(InterpPart::Lit(std::mem::take(&mut lit)));
                    }
                    self.spend(LINK_COST)?;
                    let inner = &raw[i + 1..close];
                    let toks = lex(inner, self.file, content_start + i + 1, self.sh.diags);
                    let mut sub = Parser { toks, pos: 0, file: self.file, sh: &mut *self.sh, in_method: self.in_method };
                    let e = sub.expr()?;
                    if !matches!(sub.peek(), Tok::Newline | Tok::Eof) {
                        return Err(sub.unexpected("'}'"));
                    }
                    parts.push(InterpPart::Expr(e));
                    i = close + 1;
                }
                _ => {
                    let ch = raw[i..].chars().next().unwrap();
                    lit.push(ch);
                    i += ch.len_utf8();
                }
            }
        }
        if !lit.is_empty() {
            parts.push(InterpPart::Lit(lit));
        }
        Ok(parts)
    }
}

/// Index of the `}` closing the hole that opens at `open`, skipping
/// string literals and nested braces inside the hole.
fn matching_brace(raw: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_str = false;
    for (i, c) in raw.bytes().enumerate().skip(open) {
        match c {
            b'"' => in_str = !in_str,
            b'{' if !in_str => depth += 1,
            b'}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
    let span = l.span.to(r.span);
    Expr { kind: ExprKind::Binary(op, Box::new(l), Box::new(r)), span }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Int(v) => format!("'{v}'"),
        Tok::Float(v) => format!("'{v}'"),
        Tok::Str(_) | Tok::Interp(_) => "string".into(),
        Tok::Ident(n) => format!("'{n}'"),
        Tok::Kw(k) => format!("keyword '{k:?}'"),
        Tok::Op(o) => format!("'{o}'"),
        Tok::Newline => "end of line".into(),
        Tok::Eof => "end of file".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> (Program, Diagnostics) {
        let mut sm = SourceMap::default();
        let f = sm.add("test.nb", src);
        let mut d = Diagnostics::default();
        let p = parse_program(&mut sm, f, &mut NoLoader, &mut d);
        (p, d)
    }

    #[test]
    fn included_file_may_start_with_byte_order_mark() {
        struct Bom;
        impl SourceLoader for Bom {
            fn load(&mut self, path: &str, _: &str) -> Result<(String, String), String> {
                Ok((path.to_string(), "\u{feff}x = 1\n".to_string()))
            }
        }
        let mut sm = SourceMap::default();
        let f = sm.add("test.nb", "Include \"a.nb\"\n");
        let mut d = Diagnostics::default();
        let p = parse_program(&mut sm, f, &mut Bom, &mut d);
        assert!(!d.has_errors(), "{}", d.render(&sm));
        assert_eq!(p.main.len(), 1);
    }

    #[test]
    fn parses_sample_program() {
        let (p, d) = parse(
            "Graphics3D 1280, 720\ncamera = CreateCamera()\nPositionEntity camera, 0, 5, -10\n\
             While Not KeyDown(KEY_ESCAPE)\n If KeyDown(KEY_W)\n MoveEntity cube, 0, 0, 0.1\n EndIf\n\
             UpdateWorld()\n RenderWorld()\nWend\nEnd\n",
        );
        assert!(!d.has_errors(), "{:?}", d.items);
        assert_eq!(p.main.len(), 5);
        assert!(matches!(p.main[3].kind, StmtKind::While { .. }));
    }

    #[test]
    fn single_line_if_else() {
        let (p, d) = parse("If x > 1 Then y = 2 Else y = 3 : z = 4\n");
        assert!(!d.has_errors(), "{:?}", d.items);
        let StmtKind::If { then, els, .. } = &p.main[0].kind else { panic!() };
        assert_eq!(then.len(), 1);
        assert_eq!(els.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn types_methods_and_functions() {
        let (p, d) = parse(
            "Type Player\n Field x#, y# = 2\n Field name$\n Method Move(dx#)\n Self\\x += dx\n End Method\nEnd Type\n\
             Function Add%(a, b = 2)\n Return a + b\nEnd Function\np.Player = New Player\np\\Move(1.5)\n",
        );
        assert!(!d.has_errors(), "{:?}", d.items);
        assert_eq!(p.types[0].fields.len(), 3);
        assert_eq!(p.types[0].methods.len(), 1);
        assert_eq!(p.funcs[0].params.len(), 2);
    }

    #[test]
    fn reports_missing_wend() {
        let (_, d) = parse("While 1\nPrint 1\n");
        assert!(d.has_errors());
        assert!(d.items[0].message.contains("'Wend'"), "{}", d.items[0].message);
    }

    #[test]
    fn enums_and_for_in() {
        let (p, d) = parse("Enum Color
 Red
 Green = 5, Blue
End Enum
For Each s In Ship
Next
");
        assert!(!d.has_errors(), "{:?}", d.items);
        assert_eq!(p.enums[0].members.len(), 3);
        assert!(matches!(p.main[0].kind, StmtKind::ForIn { .. }));
    }

    #[test]
    fn interpolation() {
        let (p, d) = parse("Print $\"a {x + 1} b\"\n");
        assert!(!d.has_errors(), "{:?}", d.items);
        let StmtKind::Expr(Expr { kind: ExprKind::Call { args, .. }, .. }) = &p.main[0].kind else { panic!() };
        let ExprKind::Interp(parts) = &args[0].kind else { panic!() };
        assert_eq!(parts.len(), 3);
    }
}
