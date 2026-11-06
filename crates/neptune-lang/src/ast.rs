//! Abstract syntax tree produced by the parser.

use crate::diag::Span;

/// A type annotation as written in source: a sigil tag (`%`, `#`, `$`, `.T`)
/// or an `As` clause.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeAnn {
    Int,
    Float,
    Str,
    Variant,
    Obj(String),
}

#[derive(Clone, Debug)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod, Pow,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or, Xor, Shl, Shr, Sar,
    AndAlso, OrElse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg, Plus, Not, BitNot, Abs, Sgn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastTo {
    Int, Float, Str,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Int(i32),
    Float(f32),
    Str(String),
    Null,
    SelfRef,
    /// Variable reference (also array element / field access through [`Var`]).
    Var(Box<Var>),
    Call { name: Ident, tag: Option<TypeAnn>, args: Vec<Expr> },
    MethodCall { target: Box<Expr>, method: Ident, args: Vec<Expr> },
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Cast(CastTo, Box<Expr>),
    New(Ident),
    First(Ident),
    Last(Ident),
    After(Box<Expr>),
    Before(Box<Expr>),
    Handle(Box<Expr>),
    Object(Ident, Box<Expr>),
    /// `$"text {expr} text"` – alternating literal parts and expressions.
    Interp(Vec<InterpPart>),
}

#[derive(Clone, Debug)]
pub enum InterpPart {
    Lit(String),
    Expr(Expr),
}

/// An assignable location.
#[derive(Clone, Debug)]
pub enum Var {
    Ident { name: Ident, tag: Option<TypeAnn> },
    /// `Dim` array element: `a(i, j)`.
    Array { name: Ident, tag: Option<TypeAnn>, index: Vec<Expr> },
    /// `obj\field`
    Field { target: Expr, field: Ident, tag: Option<TypeAnn> },
    /// Blitz vector element: `v[i]`
    Vector { target: Expr, index: Expr },
}

impl Var {
    pub fn span(&self) -> Span {
        match self {
            Var::Ident { name, .. } => name.span,
            Var::Array { name, index, .. } => index.last().map_or(name.span, |e| name.span.to(e.span)),
            Var::Field { target, field, .. } => target.span.to(field.span),
            Var::Vector { target, index } => target.span.to(index.span),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclKind {
    Local,
    Global,
    Const,
    Param,
    Field,
}

/// `name[tag] [As T] [= init]` or `name[tag][size]`.
#[derive(Clone, Debug)]
pub struct VarDecl {
    pub name: Ident,
    pub ty: Option<TypeAnn>,
    pub init: Option<Expr>,
    /// Blitz vector size (`Local v[10]` holds 11 elements).
    pub vector: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Expr(Expr),
    Assign(Var, Expr),
    CompoundAssign(Var, BinOp, Expr),
    Decl(DeclKind, VarDecl),
    Dim { name: Ident, tag: Option<TypeAnn>, dims: Vec<Expr> },
    If { cond: Expr, then: Vec<Stmt>, els: Option<Vec<Stmt>> },
    While { cond: Expr, body: Vec<Stmt> },
    Repeat { body: Vec<Stmt>, until: Option<Expr> },
    For { var: Var, from: Expr, to: Expr, step: Option<Expr>, body: Vec<Stmt> },
    ForEach { var: Var, ty: Ident, body: Vec<Stmt> },
    /// `For Each x In <type name | Blitz array>`
    ForIn { var: Ident, iter: Expr, body: Vec<Stmt> },
    Select { expr: Expr, cases: Vec<(Vec<Expr>, Vec<Stmt>)>, default: Option<Vec<Stmt>> },
    Exit,
    Continue,
    Return(Option<Expr>),
    Goto(Ident),
    Gosub(Ident),
    Label(Ident),
    Delete(Expr),
    DeleteEach(Ident),
    Insert { obj: Expr, before: bool, target: Expr },
    Read(Vec<Var>),
    Restore(Option<Ident>),
    Try { body: Vec<Stmt>, var: Option<Var>, handler: Vec<Stmt> },
    Throw(Expr),
    End,
    Stop,
}

#[derive(Clone, Debug)]
pub struct FuncDecl {
    pub name: Ident,
    pub ret: Option<TypeAnn>,
    pub params: Vec<VarDecl>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TypeDecl {
    pub name: Ident,
    pub fields: Vec<VarDecl>,
    pub methods: Vec<FuncDecl>,
    pub span: Span,
}

/// `Enum Name ... End Enum`
#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: Ident,
    pub members: Vec<(Ident, Option<Expr>)>,
}

/// Compile-time `Data` value.
#[derive(Clone, Debug)]
pub struct DataItem {
    pub expr: Expr,
}

/// A whole program after include resolution.
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub consts: Vec<VarDecl>,
    pub types: Vec<TypeDecl>,
    pub enums: Vec<EnumDecl>,
    pub funcs: Vec<FuncDecl>,
    pub data: Vec<DataItem>,
    /// Data labels: label name → index into `data`.
    pub data_labels: Vec<(String, usize)>,
    pub main: Vec<Stmt>,
    pub strict: bool,
    pub classic: bool,
}
