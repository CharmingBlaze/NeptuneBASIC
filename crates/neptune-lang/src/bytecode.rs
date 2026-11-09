//! Bytecode format executed by `neptune-vm`.
//!
//! Instructions are statically typed (`AddI`, `AddF`, ...) so the VM never
//! has to inspect operand types on hot paths; only `Variant` operations
//! dispatch dynamically.

use crate::commands::{CommandSig, ConstVal};
use crate::diag::{SourceMap, Span};

/// Runtime representation category of a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValKind {
    Int,
    Float,
    Str,
    Obj,
    Vector,
    Variant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

/// Dynamically typed binary operation on `Variant` operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    PushInt(i32),
    PushFloat(f32),
    PushStr(u32),
    PushNull,
    Pop,
    Dup,

    LoadLocal(u32),
    StoreLocal(u32),
    LoadGlobal(u32),
    StoreGlobal(u32),
    /// `[obj] -> [value]`
    LoadField(u32),
    /// `[obj, value] -> []`
    StoreField(u32),
    /// `[d0..dn] -> []` (re)allocates a `Dim` array.
    DimArray { array: u32, dims: u8 },
    /// `[i0..in] -> [value]`
    LoadArray { array: u32, dims: u8 },
    /// `[i0..in, value] -> []`
    StoreArray { array: u32, dims: u8 },
    /// `[] -> [vector]` allocates a Blitz vector with `len` elements.
    NewVector { len: u32, elem: ValKind },
    /// `[vec] -> [length]`
    VectorLen,
    /// `[vec, index] -> [value]`
    LoadVector,
    /// `[vec, index, value] -> []`
    StoreVector,

    AddI, SubI, MulI, DivI, ModI, NegI,
    AddF, SubF, MulF, DivF, ModF, NegF, PowF,
    Concat,
    AndI, OrI, XorI, ShlI, ShrI, SarI, BitNotI,
    AbsI, AbsF, SgnI, SgnF,
    /// Logical not: pushes 1 if operand is zero / empty / Null.
    NotI, NotF, NotS, NotO,
    CmpI(Cmp), CmpF(Cmp), CmpS(Cmp),
    /// Object identity comparison (`true` = equal).
    CmpO(bool),
    BinV(VarOp),
    NegV,
    NotV,
    CmpV(Cmp),

    IToF, FToI, IToS, FToS, SToI, SToF,
    /// Converts a `Variant` (any value) to the given static kind.
    FromVariant(ValKind),

    Jump(u32),
    JumpIfZero(u32),
    JumpIfNotZero(u32),
    /// Calls bytecode function; arguments are on the stack.
    Call(u32),
    /// Calls a native command; `argc` arguments are on the stack.
    CallNative { id: u32, argc: u16 },
    /// Returns the value on top of the stack.
    Return,
    Gosub(u32),
    GosubReturn,

    New(u32),
    Delete,
    DeleteEach(u32),
    First(u32),
    Last(u32),
    After,
    Before,
    /// `[obj, target] -> []`
    Insert { before: bool },
    ToHandle,
    FromHandle(u32),

    /// Reads the next `Data` value converted to the given kind.
    Read(ValKind),
    Restore(u32),

    /// Installs an error handler jumping to `handler`.
    TryBegin { handler: u32 },
    TryEnd,
    /// `[message] -> !`
    Throw,
    End,
    Stop,

    /// Superinstruction: `local += imm` for Int locals (loop counters).
    IncLocalI(u32, i32),
}

/// Initial contents of an object field.
#[derive(Clone, Debug)]
pub enum FieldInit {
    Value(ConstVal),
    Null,
    Variant,
    Vector { len: u32, elem: ValKind },
}

#[derive(Clone, Debug)]
pub struct TypeInfo {
    pub name: String,
    pub field_names: Vec<String>,
    pub fields: Vec<FieldInit>,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub n_params: u32,
    /// Kinds of all local slots, parameters first.
    pub locals: Vec<ValKind>,
    /// Names of the local slots, parameters first (the debugger shows them).
    /// Compiler temporaries start with `\0`.
    pub local_names: Vec<String>,
    pub code: Vec<Op>,
    /// `(first pc, source span)` pairs, sorted by pc.
    pub lines: Vec<(u32, Span)>,
    /// First instruction of each statement, ascending: where the debugger can stop.
    pub stmts: Vec<u32>,
    pub span: Span,
}

impl Function {
    /// Source span for an instruction index.
    pub fn span_at(&self, pc: u32) -> Span {
        match self.lines.binary_search_by_key(&pc, |l| l.0) {
            Ok(i) => self.lines[i].1,
            Err(0) => self.span,
            Err(i) => self.lines[i - 1].1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArrayInfo {
    pub name: String,
    pub elem: ValKind,
}

/// A fully compiled program.
#[derive(Debug)]
pub struct Program {
    pub functions: Vec<Function>,
    /// Index of the main program function.
    pub main: u32,
    pub types: Vec<TypeInfo>,
    pub globals: Vec<(String, ValKind)>,
    pub arrays: Vec<ArrayInfo>,
    pub strings: Vec<std::rc::Rc<str>>,
    pub data: Vec<ConstVal>,
    /// Native commands in call-id order.
    pub natives: Vec<CommandSig>,
    pub sources: SourceMap,
}

impl Program {
    /// Disassembles one function (debugging aid).
    pub fn disassemble(&self, func: u32) -> String {
        let f = &self.functions[func as usize];
        let mut s = format!("function {} ({} params, {} locals)\n", f.name, f.n_params, f.locals.len());
        for (pc, op) in f.code.iter().enumerate() {
            let extra = match op {
                Op::PushStr(i) => format!("  ; {:?}", self.strings[*i as usize]),
                Op::Call(i) => format!("  ; {}", self.functions[*i as usize].name),
                Op::CallNative { id, .. } => format!("  ; {}", self.natives[*id as usize].display),
                _ => String::new(),
            };
            s += &format!("{pc:5}  {op:?}{extra}\n");
        }
        s
    }
}
