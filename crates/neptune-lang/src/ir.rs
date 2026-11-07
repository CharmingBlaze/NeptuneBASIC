//! Typed intermediate representation produced by semantic analysis.
//!
//! Every expression carries its resolved static type, every variable is
//! resolved to a slot, and every implicit conversion is explicit. Code
//! generators (the bytecode backend today, a native backend later) consume
//! this form and never need to re-run name resolution or type checking.

use std::rc::Rc;

use crate::ast::BinOp;
use crate::bytecode::{Cmp, FieldInit, ValKind};
use crate::commands::ConstVal;
use crate::diag::Span;

pub type TypeId = u32;
pub type FuncId = u32;

/// Static type of a value.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Ty {
    #[default]
    Int,
    Float,
    Str,
    Obj(TypeId),
    /// Type of the `Null` literal (converts to any object type).
    Null,
    /// Dynamically typed value.
    Variant,
    /// Blitz vector (`v[n]`) of the element type.
    Vector(Box<Ty>),
    Void,
}

impl Ty {
    pub fn kind(&self) -> ValKind {
        match self {
            Ty::Int | Ty::Void => ValKind::Int,
            Ty::Float => ValKind::Float,
            Ty::Str => ValKind::Str,
            Ty::Obj(_) | Ty::Null => ValKind::Obj,
            Ty::Variant => ValKind::Variant,
            Ty::Vector(_) => ValKind::Vector,
        }
    }
    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float)
    }
}

/// Numeric category selecting the typed instruction family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumKind {
    I,
    F,
    S,
    O,
    V,
}

#[derive(Clone, Debug)]
pub enum Place {
    Local(u32),
    Global(u32),
    Field { obj: Box<HExpr>, index: u32 },
    Array { array: u32, index: Vec<HExpr> },
    Vector { vec: Box<HExpr>, index: Box<HExpr> },
}

#[derive(Clone, Debug)]
pub struct HExpr {
    pub ty: Ty,
    pub kind: HKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HUnary {
    Neg,
    Not,
    BitNot,
    Abs,
    Sgn,
}

#[derive(Clone, Debug)]
pub enum HKind {
    Int(i32),
    Float(f32),
    Str(Rc<str>),
    Null,
    Load(Place),
    /// Unary operation; `NumKind` is the operand category.
    Unary(HUnary, NumKind, Box<HExpr>),
    /// Arithmetic/bitwise operation on operands already converted to a common kind.
    Binary(BinOp, NumKind, Box<HExpr>, Box<HExpr>),
    Compare(Cmp, NumKind, Box<HExpr>, Box<HExpr>),
    AndAlso(Box<HExpr>, Box<HExpr>),
    OrElse(Box<HExpr>, Box<HExpr>),
    /// Converts the inner expression to `self.ty`.
    Convert(Box<HExpr>),
    Call(FuncId, Vec<HExpr>),
    Native(u32, Vec<HExpr>),
    New(TypeId),
    First(TypeId),
    Last(TypeId),
    After(Box<HExpr>),
    Before(Box<HExpr>),
    Handle(Box<HExpr>),
    Object(TypeId, Box<HExpr>),
    /// `IIf(cond, a, b)` - only the selected branch is evaluated.
    IfElse(Box<HExpr>, Box<HExpr>, Box<HExpr>),
}

impl HExpr {
    pub fn int(v: i32, span: Span) -> HExpr {
        HExpr { ty: Ty::Int, kind: HKind::Int(v), span }
    }
    pub fn as_const(&self) -> Option<ConstVal> {
        match &self.kind {
            HKind::Int(v) => Some(ConstVal::Int(*v)),
            HKind::Float(v) => Some(ConstVal::Float(*v)),
            HKind::Str(s) => Some(ConstVal::Str(s.clone())),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ForStep {
    /// Step known at compile time (sign decides loop direction).
    Const(HExpr, bool),
    /// Step computed once at loop entry and stored in a temporary local.
    Dynamic(HExpr, u32),
}

#[derive(Clone, Debug)]
pub struct HStmt {
    pub kind: HStmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum HStmtKind {
    /// Evaluate for side effects; `pop` if a value is produced.
    Expr(HExpr, bool),
    Store(Place, HExpr),
    If(HExpr, Vec<HStmt>, Vec<HStmt>),
    While(HExpr, Vec<HStmt>),
    Repeat(Vec<HStmt>, Option<HExpr>),
    For {
        var: Place,
        kind: NumKind,
        from: HExpr,
        to: HExpr,
        to_tmp: u32,
        step: ForStep,
        body: Vec<HStmt>,
    },
    ForEach { var: Place, ty: TypeId, next_tmp: u32, body: Vec<HStmt> },
    /// `For Each x In vector`
    ForVector { var: Place, vec: HExpr, vec_tmp: u32, idx_tmp: u32, body: Vec<HStmt> },
    Select { tmp: u32, value: HExpr, cases: Vec<(Vec<HExpr>, Vec<HStmt>)>, default: Vec<HStmt> },
    Exit,
    Continue,
    Return(Option<HExpr>),
    GosubReturn,
    Goto(u32),
    Gosub(u32),
    Label(u32),
    Delete(HExpr),
    DeleteEach(TypeId),
    Insert { obj: HExpr, before: bool, target: HExpr },
    Read(Place, ValKind),
    Restore(u32),
    Dim { array: u32, dims: Vec<HExpr> },
    NewVector(Place, u32, ValKind),
    Try { body: Vec<HStmt>, catch: Option<Place>, handler: Vec<HStmt> },
    Throw(HExpr),
    End,
    Stop,
}

#[derive(Clone, Debug)]
pub struct HFunc {
    pub name: String,
    pub n_params: u32,
    pub locals: Vec<(String, Ty)>,
    pub ret: Ty,
    pub body: Vec<HStmt>,
    pub n_labels: u32,
    pub is_main: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct HType {
    pub name: String,
    pub fields: Vec<(String, Ty, FieldInit)>,
}

/// The complete typed program.
#[derive(Clone, Debug)]
pub struct HProgram {
    pub funcs: Vec<HFunc>,
    pub main: FuncId,
    pub types: Vec<HType>,
    pub globals: Vec<(String, Ty)>,
    pub arrays: Vec<(String, Ty, u8)>,
    pub data: Vec<ConstVal>,
    /// Initialization code for globals runs at the start of main.
    pub n_natives: u32,
}
