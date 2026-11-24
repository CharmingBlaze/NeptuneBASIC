//! Bytecode generation from the typed IR.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::BinOp;
use crate::bytecode::*;
use crate::commands::CommandSig;
use crate::diag::{SourceMap, Span};
use crate::ir::*;

struct Loop {
    breaks: Vec<usize>,
    continues: Vec<usize>,
    try_depth: u32,
}

struct FnGen<'a> {
    code: Vec<Op>,
    lines: Vec<(u32, Span)>,
    stmts: Vec<u32>,
    strings: &'a mut Vec<Rc<str>>,
    string_map: &'a mut HashMap<Rc<str>, u32>,
    loops: Vec<Loop>,
    labels: Vec<Option<u32>>,
    label_fixups: Vec<(usize, u32)>,
    /// `Try` depth at each label.
    label_try_depth: Vec<u32>,
    /// `TryLeave` to fill in: (instruction, `Try` depth of the `Goto`, label).
    try_leaves: Vec<(usize, u32, u32)>,
    try_depth: u32,
    is_main: bool,
    ret: Ty,
    local_kinds: Vec<ValKind>,
    scratch: Option<u32>,
}

/// Generates the final program.
pub fn generate(p: &HProgram, natives: &[CommandSig], sources: SourceMap) -> Program {
    let mut strings = Vec::new();
    let mut string_map = HashMap::new();
    let mut functions = Vec::with_capacity(p.funcs.len());
    for f in &p.funcs {
        let mut g = FnGen {
            code: Vec::new(),
            lines: Vec::new(),
            stmts: Vec::new(),
            strings: &mut strings,
            string_map: &mut string_map,
            loops: Vec::new(),
            labels: vec![None; f.n_labels as usize],
            label_fixups: Vec::new(),
            label_try_depth: vec![0; f.n_labels as usize],
            try_leaves: Vec::new(),
            try_depth: 0,
            is_main: f.is_main,
            ret: f.ret.clone(),
            local_kinds: f.locals.iter().map(|(_, t)| t.kind()).collect(),
            scratch: None,
        };
        g.block(&f.body);
        // implicit end
        if f.is_main {
            g.emit(Op::End);
        } else {
            g.push_default(&f.ret.clone());
            g.emit(Op::Return);
        }
        for (at, label) in std::mem::take(&mut g.label_fixups) {
            let target = g.labels[label as usize].expect("label resolved by sema");
            g.patch(at, target);
        }
        // sema rejects jumps into a `Try`, so a label is never deeper than its `Goto`
        for (at, depth, label) in std::mem::take(&mut g.try_leaves) {
            g.code[at] = Op::TryLeave(depth - g.label_try_depth[label as usize]);
        }
        crate::fuse::fuse(&mut g.code, &mut g.lines, &mut g.stmts);
        functions.push(Function {
            name: f.name.clone(),
            n_params: f.n_params,
            locals: g.local_kinds,
            local_names: f.locals.iter().map(|(n, _)| n.clone()).collect(),
            code: g.code,
            lines: g.lines,
            stmts: g.stmts,
            span: f.span,
        });
    }
    Program {
        functions,
        main: p.main,
        types: p
            .types
            .iter()
            .map(|t| TypeInfo {
                name: t.name.clone(),
                field_names: t.fields.iter().map(|f| f.0.clone()).collect(),
                fields: t.fields.iter().map(|f| f.2.clone()).collect(),
            })
            .collect(),
        globals: p.globals.iter().map(|(n, t)| (n.clone(), t.kind())).collect(),
        arrays: p.arrays.iter().map(|(n, t, _)| ArrayInfo { name: n.clone(), elem: t.kind() }).collect(),
        strings,
        data: p.data.clone(),
        natives: natives.to_vec(),
        sources,
    }
}

impl<'a> FnGen<'a> {
    fn emit(&mut self, op: Op) -> usize {
        self.code.push(op);
        self.code.len() - 1
    }
    fn here(&self) -> u32 {
        self.code.len() as u32
    }
    fn mark(&mut self, span: Span) {
        let pc = self.here();
        if let Some(last) = self.lines.last_mut() {
            if last.0 == pc {
                last.1 = span;
                return;
            }
            if last.1 == span {
                return;
            }
        }
        self.lines.push((pc, span));
    }
    fn patch(&mut self, at: usize, target: u32) {
        match &mut self.code[at] {
            Op::Jump(t) | Op::JumpIfZero(t) | Op::JumpIfNotZero(t) | Op::Gosub(t) => *t = target,
            Op::TryBegin { handler } => *handler = target,
            op => unreachable!("patching non-jump {op:?}"),
        }
    }
    fn str_const(&mut self, s: &Rc<str>) -> u32 {
        if let Some(&i) = self.string_map.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.push(s.clone());
        self.string_map.insert(s.clone(), i);
        i
    }

    fn push_default(&mut self, ty: &Ty) {
        match ty {
            Ty::Float => self.emit(Op::PushFloat(0.0)),
            Ty::Str => {
                let i = self.str_const(&Rc::from(""));
                self.emit(Op::PushStr(i))
            }
            Ty::Obj(_) | Ty::Null | Ty::Vector(_) => self.emit(Op::PushNull),
            _ => self.emit(Op::PushInt(0)),
        };
    }

    fn block(&mut self, stmts: &[HStmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &HStmt) {
        self.mark(s.span);
        let pc = self.here();
        if self.stmts.last() != Some(&pc) {
            self.stmts.push(pc);
        }
        match &s.kind {
            HStmtKind::Expr(e, pop) => {
                self.expr(e);
                if *pop {
                    self.emit(Op::Pop);
                }
            }
            HStmtKind::Store(p, e) => self.store(p, e),
            HStmtKind::If(c, t, e) => {
                self.expr(c);
                let jf = self.emit(Op::JumpIfZero(0));
                self.block(t);
                if e.is_empty() {
                    let h = self.here();
                    self.patch(jf, h);
                } else {
                    let je = self.emit(Op::Jump(0));
                    let h = self.here();
                    self.patch(jf, h);
                    self.block(e);
                    let h = self.here();
                    self.patch(je, h);
                }
            }
            HStmtKind::While(c, body) => {
                let top = self.here();
                self.expr(c);
                let jf = self.emit(Op::JumpIfZero(0));
                self.loops.push(Loop { breaks: vec![jf], continues: vec![], try_depth: self.try_depth });
                self.block(body);
                self.emit(Op::Jump(top));
                self.end_loop(top);
            }
            HStmtKind::Repeat(body, until) => {
                let top = self.here();
                self.loops.push(Loop { breaks: vec![], continues: vec![], try_depth: self.try_depth });
                self.block(body);
                let cont = self.here();
                match until {
                    Some(c) => {
                        self.expr(c);
                        self.emit(Op::JumpIfZero(top));
                    }
                    None => {
                        self.emit(Op::Jump(top));
                    }
                }
                self.end_loop(cont);
            }
            HStmtKind::For { var, kind, from, to, to_tmp, step, body } => {
                self.store(var, from);
                self.expr(to);
                self.emit(Op::StoreLocal(*to_tmp));
                if let ForStep::Dynamic(e, tmp) = step {
                    self.expr(e);
                    self.emit(Op::StoreLocal(*tmp));
                }
                let (cmp_op, sub_cmp): (fn(Cmp) -> Op, _) = match kind {
                    NumKind::F => (Op::CmpF as fn(Cmp) -> Op, NumKind::F),
                    _ => (Op::CmpI as fn(Cmp) -> Op, NumKind::I),
                };
                let top = self.here();
                let mut exits = Vec::new();
                match step {
                    ForStep::Const(_, up) => {
                        self.load(var);
                        self.emit(Op::LoadLocal(*to_tmp));
                        self.emit(cmp_op(if *up { Cmp::Gt } else { Cmp::Lt }));
                        exits.push(self.emit(Op::JumpIfNotZero(0)));
                    }
                    ForStep::Dynamic(_, tmp) => {
                        self.emit(Op::LoadLocal(*tmp));
                        if sub_cmp == NumKind::F {
                            self.emit(Op::PushFloat(0.0));
                        } else {
                            self.emit(Op::PushInt(0));
                        }
                        self.emit(cmp_op(Cmp::Lt));
                        let jneg = self.emit(Op::JumpIfNotZero(0));
                        self.load(var);
                        self.emit(Op::LoadLocal(*to_tmp));
                        self.emit(cmp_op(Cmp::Gt));
                        exits.push(self.emit(Op::JumpIfNotZero(0)));
                        let jbody = self.emit(Op::Jump(0));
                        let h = self.here();
                        self.patch(jneg, h);
                        self.load(var);
                        self.emit(Op::LoadLocal(*to_tmp));
                        self.emit(cmp_op(Cmp::Lt));
                        exits.push(self.emit(Op::JumpIfNotZero(0)));
                        let h = self.here();
                        self.patch(jbody, h);
                    }
                }
                self.loops.push(Loop { breaks: exits, continues: vec![], try_depth: self.try_depth });
                self.block(body);
                let cont = self.here();
                // increment
                match (var, step, kind) {
                    (Place::Local(slot), ForStep::Const(HExpr { kind: HKind::Int(v), .. }, _), NumKind::I) => {
                        self.emit(Op::IncLocalI(*slot, *v));
                    }
                    _ => {
                        self.load(var);
                        match step {
                            ForStep::Const(e, _) => self.expr(e),
                            ForStep::Dynamic(_, tmp) => {
                                self.emit(Op::LoadLocal(*tmp));
                            }
                        }
                        self.emit(if *kind == NumKind::F { Op::AddF } else { Op::AddI });
                        self.store_top(var);
                    }
                }
                self.emit(Op::Jump(top));
                self.end_loop(cont);
            }
            HStmtKind::ForEach { var, ty, next_tmp, body } => {
                self.emit(Op::First(*ty));
                self.emit(Op::StoreLocal(*next_tmp));
                let top = self.here();
                self.emit(Op::LoadLocal(*next_tmp));
                self.emit(Op::NotO);
                let jexit = self.emit(Op::JumpIfNotZero(0));
                self.emit(Op::LoadLocal(*next_tmp));
                self.store_top(var);
                // precompute the successor so the body may delete the current object
                self.emit(Op::LoadLocal(*next_tmp));
                self.emit(Op::After);
                self.emit(Op::StoreLocal(*next_tmp));
                self.loops.push(Loop { breaks: vec![jexit], continues: vec![], try_depth: self.try_depth });
                self.block(body);
                self.emit(Op::Jump(top));
                self.end_loop(top);
            }
            HStmtKind::ForVector { var, vec, vec_tmp, idx_tmp, body } => {
                self.expr(vec);
                self.emit(Op::StoreLocal(*vec_tmp));
                self.emit(Op::PushInt(0));
                self.emit(Op::StoreLocal(*idx_tmp));
                let top = self.here();
                self.emit(Op::LoadLocal(*idx_tmp));
                self.emit(Op::LoadLocal(*vec_tmp));
                self.emit(Op::VectorLen);
                self.emit(Op::CmpI(Cmp::Lt));
                let jexit = self.emit(Op::JumpIfZero(0));
                self.emit(Op::LoadLocal(*vec_tmp));
                self.emit(Op::LoadLocal(*idx_tmp));
                self.emit(Op::LoadVector);
                self.store_top(var);
                self.loops.push(Loop { breaks: vec![jexit], continues: vec![], try_depth: self.try_depth });
                self.block(body);
                let cont = self.here();
                self.emit(Op::IncLocalI(*idx_tmp, 1));
                self.emit(Op::Jump(top));
                self.end_loop(cont);
            }
            HStmtKind::Select { tmp, value, cases, default } => {
                self.expr(value);
                self.emit(Op::StoreLocal(*tmp));
                let mut ends = Vec::new();
                for (conds, body) in cases {
                    let mut hits = Vec::new();
                    for c in conds {
                        self.expr(c);
                        hits.push(self.emit(Op::JumpIfNotZero(0)));
                    }
                    let skip = self.emit(Op::Jump(0));
                    let h = self.here();
                    for j in hits {
                        self.patch(j, h);
                    }
                    self.block(body);
                    ends.push(self.emit(Op::Jump(0)));
                    let h = self.here();
                    self.patch(skip, h);
                }
                self.block(default);
                let h = self.here();
                for j in ends {
                    self.patch(j, h);
                }
            }
            HStmtKind::Exit | HStmtKind::Continue => {
                let lp = self.loops.last().expect("sema checked loop depth");
                for _ in lp.try_depth..self.try_depth {
                    self.emit(Op::TryEnd);
                }
                let j = self.emit(Op::Jump(0));
                let lp = self.loops.last_mut().unwrap();
                if matches!(s.kind, HStmtKind::Exit) {
                    lp.breaks.push(j);
                } else {
                    lp.continues.push(j);
                }
            }
            HStmtKind::Return(e) => {
                match e {
                    Some(e) => self.expr(e),
                    None => {
                        let r = self.ret.clone();
                        self.push_default(&r);
                    }
                }
                self.emit(Op::Return);
            }
            HStmtKind::GosubReturn => {
                debug_assert!(self.is_main);
                self.emit(Op::GosubReturn);
            }
            HStmtKind::Goto(l) => {
                if self.try_depth > 0 {
                    let at = self.emit(Op::TryLeave(0));
                    self.try_leaves.push((at, self.try_depth, *l));
                }
                let j = self.emit(Op::Jump(0));
                self.label_fixups.push((j, *l));
            }
            HStmtKind::Gosub(l) => {
                let j = self.emit(Op::Gosub(0));
                self.label_fixups.push((j, *l));
            }
            HStmtKind::Label(l) => {
                self.labels[*l as usize] = Some(self.here());
                self.label_try_depth[*l as usize] = self.try_depth;
            }
            HStmtKind::Delete(e) => {
                self.expr(e);
                self.emit(Op::Delete);
            }
            HStmtKind::DeleteEach(t) => {
                self.emit(Op::DeleteEach(*t));
            }
            HStmtKind::Insert { obj, before, target } => {
                self.expr(obj);
                self.expr(target);
                self.emit(Op::Insert { before: *before });
            }
            HStmtKind::Read(p, k) => {
                self.with_place_prefix(p, |g| {
                    g.emit(Op::Read(*k));
                });
            }
            HStmtKind::Restore(i) => {
                self.emit(Op::Restore(*i));
            }
            HStmtKind::Dim { array, dims } => {
                for d in dims {
                    self.expr(d);
                }
                self.emit(Op::DimArray { array: *array, dims: dims.len() as u8 });
            }
            HStmtKind::NewVector(p, len, k) => {
                let (len, k) = (*len, *k);
                self.with_place_prefix(p, |g| {
                    g.emit(Op::NewVector { len, elem: k });
                });
            }
            HStmtKind::Try { body, catch, handler } => {
                let tb = self.emit(Op::TryBegin { handler: 0 });
                self.try_depth += 1;
                self.block(body);
                self.try_depth -= 1;
                self.emit(Op::TryEnd);
                let jend = self.emit(Op::Jump(0));
                let h = self.here();
                self.patch(tb, h);
                // VM pushes the error message
                match catch {
                    Some(p) => self.store_top(p),
                    None => {
                        self.emit(Op::Pop);
                    }
                }
                self.block(handler);
                let h = self.here();
                self.patch(jend, h);
            }
            HStmtKind::Throw(e) => {
                self.expr(e);
                self.emit(Op::Throw);
            }
            HStmtKind::End => {
                self.emit(Op::End);
            }
            HStmtKind::Stop => {
                self.emit(Op::Stop);
            }
        }
    }

    fn end_loop(&mut self, cont: u32) {
        let lp = self.loops.pop().unwrap();
        let end = self.here();
        for j in lp.breaks {
            self.patch(j, end);
        }
        for j in lp.continues {
            self.patch(j, cont);
        }
    }

    /// Pushes the place's address operands, runs `value` (which must push one
    /// value) and emits the store.
    fn with_place_prefix(&mut self, p: &Place, value: impl FnOnce(&mut Self)) {
        match p {
            Place::Local(s) => {
                value(self);
                self.emit(Op::StoreLocal(*s));
            }
            Place::Global(g) => {
                value(self);
                self.emit(Op::StoreGlobal(*g));
            }
            Place::Field { obj, index } => {
                self.expr(obj);
                value(self);
                self.emit(Op::StoreField(*index));
            }
            Place::Array { array, index } => {
                for i in index {
                    self.expr(i);
                }
                value(self);
                self.emit(Op::StoreArray { array: *array, dims: index.len() as u8 });
            }
            Place::Vector { vec, index } => {
                self.expr(vec);
                self.expr(index);
                value(self);
                self.emit(Op::StoreVector);
            }
        }
    }

    fn store(&mut self, p: &Place, e: &HExpr) {
        self.with_place_prefix(p, |g| g.expr(e));
    }

    /// Stores the value currently on top of the stack. Complex places spill
    /// the value to a scratch local so their address operands can be pushed first.
    fn store_top(&mut self, p: &Place) {
        match p {
            Place::Local(s) => {
                self.emit(Op::StoreLocal(*s));
            }
            Place::Global(g) => {
                self.emit(Op::StoreGlobal(*g));
            }
            _ => {
                let scratch = self.scratch_slot();
                self.emit(Op::StoreLocal(scratch));
                self.with_place_prefix(p, |g| {
                    g.emit(Op::LoadLocal(scratch));
                });
            }
        }
    }

    /// A Variant scratch local, allocated lazily at the end of the frame.
    fn scratch_slot(&mut self) -> u32 {
        if let Some(i) = self.scratch {
            return i;
        }
        self.local_kinds.push(ValKind::Variant);
        let i = (self.local_kinds.len() - 1) as u32;
        self.scratch = Some(i);
        i
    }

    fn load(&mut self, p: &Place) {
        match p {
            Place::Local(s) => {
                self.emit(Op::LoadLocal(*s));
            }
            Place::Global(g) => {
                self.emit(Op::LoadGlobal(*g));
            }
            Place::Field { obj, index } => {
                self.expr(obj);
                self.emit(Op::LoadField(*index));
            }
            Place::Array { array, index } => {
                for i in index {
                    self.expr(i);
                }
                self.emit(Op::LoadArray { array: *array, dims: index.len() as u8 });
            }
            Place::Vector { vec, index } => {
                self.expr(vec);
                self.expr(index);
                self.emit(Op::LoadVector);
            }
        }
    }

    fn expr(&mut self, e: &HExpr) {
        match &e.kind {
            HKind::Int(v) => {
                self.emit(Op::PushInt(*v));
            }
            HKind::Float(v) => {
                self.emit(Op::PushFloat(*v));
            }
            HKind::Str(s) => {
                let i = self.str_const(s);
                self.emit(Op::PushStr(i));
            }
            HKind::Null => {
                self.emit(Op::PushNull);
            }
            HKind::Load(p) => self.load(p),
            HKind::Unary(op, k, x) => {
                self.expr(x);
                let o = match (op, k) {
                    (HUnary::Neg, NumKind::I) => Op::NegI,
                    (HUnary::Neg, NumKind::F) => Op::NegF,
                    (HUnary::Neg, _) => Op::NegV,
                    (HUnary::Not, NumKind::I) => Op::NotI,
                    (HUnary::Not, NumKind::F) => Op::NotF,
                    (HUnary::Not, NumKind::S) => Op::NotS,
                    (HUnary::Not, NumKind::O) => Op::NotO,
                    (HUnary::Not, NumKind::V) => Op::NotV,
                    (HUnary::BitNot, _) => Op::BitNotI,
                    (HUnary::Abs, NumKind::F) => Op::AbsF,
                    (HUnary::Abs, _) => Op::AbsI,
                    (HUnary::Sgn, NumKind::F) => Op::SgnF,
                    (HUnary::Sgn, _) => Op::SgnI,
                };
                self.emit(o);
            }
            HKind::Binary(op, k, l, r) => {
                self.expr(l);
                self.expr(r);
                use BinOp::*;
                let o = match (k, op) {
                    (NumKind::I, Add) => Op::AddI,
                    (NumKind::I, Sub) => Op::SubI,
                    (NumKind::I, Mul) => Op::MulI,
                    (NumKind::I, Div) => Op::DivI,
                    (NumKind::I, Mod) => Op::ModI,
                    (NumKind::I, And) => Op::AndI,
                    (NumKind::I, Or) => Op::OrI,
                    (NumKind::I, Xor) => Op::XorI,
                    (NumKind::I, Shl) => Op::ShlI,
                    (NumKind::I, Shr) => Op::ShrI,
                    (NumKind::I, Sar) => Op::SarI,
                    (NumKind::F, Add) => Op::AddF,
                    (NumKind::F, Sub) => Op::SubF,
                    (NumKind::F, Mul) => Op::MulF,
                    (NumKind::F, Div) => Op::DivF,
                    (NumKind::F, Mod) => Op::ModF,
                    (NumKind::F, Pow) => Op::PowF,
                    (NumKind::S, Add) => Op::Concat,
                    (NumKind::V, Add) => Op::BinV(VarOp::Add),
                    (NumKind::V, Sub) => Op::BinV(VarOp::Sub),
                    (NumKind::V, Mul) => Op::BinV(VarOp::Mul),
                    (NumKind::V, Div) => Op::BinV(VarOp::Div),
                    (NumKind::V, Mod) => Op::BinV(VarOp::Mod),
                    (NumKind::V, Pow) => Op::BinV(VarOp::Pow),
                    (k, op) => unreachable!("no instruction for {op:?} on {k:?}"),
                };
                self.emit(o);
            }
            HKind::Compare(c, k, l, r) => {
                self.expr(l);
                self.expr(r);
                let o = match k {
                    NumKind::I => Op::CmpI(*c),
                    NumKind::F => Op::CmpF(*c),
                    NumKind::S => Op::CmpS(*c),
                    NumKind::O => Op::CmpO(*c == Cmp::Eq),
                    NumKind::V => Op::CmpV(*c),
                };
                self.emit(o);
            }
            HKind::AndAlso(l, r) | HKind::OrElse(l, r) => {
                let and = matches!(e.kind, HKind::AndAlso(..));
                self.expr(l);
                let j = self.emit(if and { Op::JumpIfZero(0) } else { Op::JumpIfNotZero(0) });
                self.expr(r);
                // normalize to 0/1
                self.emit(Op::NotI);
                self.emit(Op::NotI);
                let jend = self.emit(Op::Jump(0));
                let h = self.here();
                self.patch(j, h);
                self.emit(Op::PushInt(if and { 0 } else { 1 }));
                let h = self.here();
                self.patch(jend, h);
            }
            HKind::Convert(x) => {
                self.expr(x);
                let o = match (&x.ty, &e.ty) {
                    (_, Ty::Variant) => None,
                    (Ty::Variant, t) => Some(Op::FromVariant(t.kind())),
                    (Ty::Int, Ty::Float) => Some(Op::IToF),
                    (Ty::Float, Ty::Int) => Some(Op::FToI),
                    (Ty::Int, Ty::Str) => Some(Op::IToS),
                    (Ty::Float, Ty::Str) => Some(Op::FToS),
                    (Ty::Str, Ty::Int) => Some(Op::SToI),
                    (Ty::Str, Ty::Float) => Some(Op::SToF),
                    (a, b) => unreachable!("no conversion {a:?} -> {b:?}"),
                };
                if let Some(o) = o {
                    self.emit(o);
                }
            }
            HKind::Call(f, args) => {
                for a in args {
                    self.expr(a);
                }
                self.emit(Op::Call(*f));
            }
            HKind::Native(n, args) => {
                for a in args {
                    self.expr(a);
                }
                self.emit(Op::CallNative { id: *n, argc: args.len() as u16 });
            }
            HKind::New(t) => {
                self.emit(Op::New(*t));
            }
            HKind::First(t) => {
                self.emit(Op::First(*t));
            }
            HKind::Last(t) => {
                self.emit(Op::Last(*t));
            }
            HKind::After(x) => {
                self.expr(x);
                self.emit(Op::After);
            }
            HKind::Before(x) => {
                self.expr(x);
                self.emit(Op::Before);
            }
            HKind::Handle(x) => {
                self.expr(x);
                self.emit(Op::ToHandle);
            }
            HKind::Object(t, x) => {
                self.expr(x);
                self.emit(Op::FromHandle(*t));
            }
            HKind::IfElse(c, a, b) => {
                self.expr(c);
                let jf = self.emit(Op::JumpIfZero(0));
                self.expr(a);
                let je = self.emit(Op::Jump(0));
                let h = self.here();
                self.patch(jf, h);
                self.expr(b);
                let h = self.here();
                self.patch(je, h);
            }
        }
    }
}
