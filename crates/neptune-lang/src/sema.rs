//! Semantic analysis and type checking: AST → typed IR.
//!
//! Responsibilities:
//! * resolve names (locals, globals, constants, arrays, functions, commands, types)
//! * infer the types of untagged variables and function results ("smart
//!   variables") unless `Classic` mode is requested
//! * check and insert implicit conversions, fold constants
//! * validate control flow (`Exit` outside loops, unknown labels, ...)

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{self, BinOp, CastTo, DeclKind, Expr, ExprKind, Stmt, StmtKind, TypeAnn, UnOp, Var};
use crate::bytecode::{Cmp, FieldInit};
use crate::commands::{CommandSig, ConstVal, NativeTy};
use crate::diag::{Diagnostics, Span};
use crate::ir::*;

/// What the host makes available to programs.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub commands: Vec<CommandSig>,
    pub constants: Vec<(String, ConstVal)>,
}

struct FuncInfo {
    display: String,
    params: Vec<(String, Ty, Option<ConstVal>)>,
    ret: Ty,
    /// Return type still to be inferred from `Return` statements.
    ret_inferred: bool,
    owner: Option<TypeId>,
}

struct TypeDef {
    name: String,
    fields: Vec<(String, Ty, FieldInit)>,
    methods: HashMap<String, FuncId>,
}

#[derive(Default)]
struct FnCtx {
    locals: Vec<(String, Ty)>,
    map: HashMap<String, u32>,
    ret: Ty,
    is_main: bool,
    self_ty: Option<TypeId>,
    labels: HashMap<String, u32>,
    label_defined: Vec<bool>,
    label_spans: Vec<Span>,
    loop_depth: u32,
    inferred: HashMap<String, Ty>,
    /// Plain variables assigned somewhere in the body (for the
    /// "used but never assigned" warning).
    assigned: std::collections::HashSet<String>,
    n_tmp: u32,
}

struct Sema<'a> {
    prog: &'a ast::Program,
    diags: &'a mut Diagnostics,
    classic: bool,
    strict: bool,
    types: Vec<TypeDef>,
    type_map: HashMap<String, TypeId>,
    funcs: Vec<FuncInfo>,
    func_map: HashMap<String, FuncId>,
    natives: &'a [CommandSig],
    native_map: HashMap<String, u32>,
    consts: HashMap<String, ConstVal>,
    globals: Vec<(String, Ty)>,
    global_map: HashMap<String, u32>,
    arrays: Vec<(String, Ty, u8)>,
    array_map: HashMap<String, u32>,
    data_labels: HashMap<String, u32>,
    enums: HashMap<String, HashMap<String, i32>>,
    f: FnCtx,
}

/// Runs semantic analysis on a parsed program.
pub fn analyze(prog: &ast::Program, env: &Env, diags: &mut Diagnostics) -> HProgram {
    let mut s = Sema {
        prog,
        diags,
        classic: prog.classic,
        strict: prog.strict,
        types: Vec::new(),
        type_map: HashMap::new(),
        funcs: Vec::new(),
        func_map: HashMap::new(),
        natives: &env.commands,
        native_map: env.commands.iter().enumerate().map(|(i, c)| (c.name.clone(), i as u32)).collect(),
        consts: env.constants.iter().map(|(n, v)| (n.to_ascii_lowercase(), v.clone())).collect(),
        globals: Vec::new(),
        global_map: HashMap::new(),
        arrays: Vec::new(),
        array_map: HashMap::new(),
        data_labels: prog.data_labels.iter().map(|(n, i)| (n.clone(), *i as u32)).collect(),
        enums: HashMap::new(),
        f: FnCtx::default(),
    };
    s.run()
}

fn ty_name(t: &Ty, types: &[TypeDef]) -> String {
    match t {
        Ty::Int => "Int".into(),
        Ty::Float => "Float".into(),
        Ty::Str => "String".into(),
        Ty::Obj(id) => types[*id as usize].name.clone(),
        Ty::Null => "Null".into(),
        Ty::Variant => "Variant".into(),
        Ty::Vector(e) => format!("{}[]", ty_name(e, types)),
        Ty::Void => "nothing".into(),
    }
}

fn num_kind(t: &Ty) -> NumKind {
    match t {
        Ty::Int | Ty::Void => NumKind::I,
        Ty::Float => NumKind::F,
        Ty::Str => NumKind::S,
        Ty::Obj(_) | Ty::Null | Ty::Vector(_) => NumKind::O,
        Ty::Variant => NumKind::V,
    }
}

fn native_ty(t: NativeTy) -> Ty {
    match t {
        NativeTy::Int => Ty::Int,
        NativeTy::Float => Ty::Float,
        NativeTy::Str => Ty::Str,
        NativeTy::Any => Ty::Variant,
    }
}

fn const_ty(c: &ConstVal) -> Ty {
    match c {
        ConstVal::Int(_) => Ty::Int,
        ConstVal::Float(_) => Ty::Float,
        ConstVal::Str(_) => Ty::Str,
    }
}

fn const_expr(c: &ConstVal, span: Span) -> HExpr {
    let (ty, kind) = match c {
        ConstVal::Int(v) => (Ty::Int, HKind::Int(*v)),
        ConstVal::Float(v) => (Ty::Float, HKind::Float(*v)),
        ConstVal::Str(s) => (Ty::Str, HKind::Str(s.clone())),
    };
    HExpr { ty, kind, span }
}

/// Unifies two inferred types: Int+Float widens to Float, Null joins objects.
fn unify(a: Option<&Ty>, b: Ty) -> Ty {
    match (a, b) {
        (None, b) => b,
        (Some(a), b) if *a == b => b,
        (Some(Ty::Int), Ty::Float) | (Some(Ty::Float), Ty::Int) => Ty::Float,
        (Some(Ty::Obj(t)), Ty::Null) => Ty::Obj(*t),
        (Some(Ty::Null), Ty::Obj(t)) => Ty::Obj(t),
        (Some(a), _) => a.clone(),
    }
}

/// Blitz-compatible float → int conversion (round half to even, like `fistp`).
pub fn float_to_int(f: f32) -> i32 {
    if f.is_nan() {
        0
    } else {
        f.round_ties_even() as i32
    }
}

/// Blitz-compatible string → number parse: leading whitespace, optional sign,
/// digits; trailing garbage is ignored.
pub fn str_to_float(s: &str) -> f32 {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut end = 0;
    if end < b.len() && (b[end] == b'+' || b[end] == b'-') {
        end += 1;
    }
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    if end < b.len() && b[end] == b'.' {
        end += 1;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
    }
    if end < b.len() && (b[end] == b'e' || b[end] == b'E') {
        let mut j = end + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            end = j;
        }
    }
    t[..end].parse().unwrap_or(0.0)
}

pub fn str_to_int(s: &str) -> i32 {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut end = 0;
    if end < b.len() && (b[end] == b'+' || b[end] == b'-') {
        end += 1;
    }
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    t[..end].parse::<i64>().map(|v| v as i32).unwrap_or(0)
}

/// Blitz-style float formatting: up to 6 decimals, trailing zeros trimmed,
/// always at least one decimal digit.
pub fn format_float(f: f32) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let mut s = if f.abs() >= 1e9 || (f != 0.0 && f.abs() < 1e-5) {
        format!("{f:e}")
    } else {
        format!("{f:.6}")
    };
    if s.contains('.') && !s.contains('e') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.push('0');
        }
    }
    if s == "-0.0" {
        s = "0.0".into();
    }
    s
}

impl<'a> Sema<'a> {
    fn err(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.error(span, msg);
    }
    fn tn(&self, t: &Ty) -> String {
        ty_name(t, &self.types)
    }

    fn resolve_ann(&mut self, ann: &TypeAnn, span: Span) -> Ty {
        match ann {
            TypeAnn::Int => Ty::Int,
            TypeAnn::Float => Ty::Float,
            TypeAnn::Str => Ty::Str,
            TypeAnn::Variant => Ty::Variant,
            TypeAnn::Obj(n) if self.enums.contains_key(n) => Ty::Int,
            TypeAnn::Obj(n) => match self.type_map.get(n) {
                Some(&t) => Ty::Obj(t),
                None => {
                    self.err(span, format!("type '{n}' not found"));
                    Ty::Int
                }
            },
        }
    }

    // ------------------------------------------------------------------
    // driver

    fn run(&mut self) -> HProgram {
        let prog = self.prog;
        // enums: members are Int constants accessed as `Name.Member`
        for en in &prog.enums {
            if self.enums.contains_key(&en.name.name) {
                self.err(en.name.span, format!("duplicate enum '{}'", en.name.name));
            }
            self.enums.insert(en.name.name.clone(), HashMap::new());
            let mut next = 0i32;
            for (m, v) in &en.members {
                self.f = FnCtx { is_main: true, ..Default::default() };
                if let Some(v) = v {
                    let h = self.expr(v);
                    let h = self.convert(h, &Ty::Int, v.span);
                    match h.kind {
                        HKind::Int(n) => next = n,
                        _ => self.err(v.span, "enum value must be an integer constant"),
                    }
                }
                let members = self.enums.get_mut(&en.name.name).unwrap();
                if members.insert(m.name.clone(), next).is_some() {
                    self.err(m.span, format!("duplicate enum member '{}'", m.name));
                }
                next = next.wrapping_add(1);
            }
        }
        // constants (in declaration order)
        for c in &prog.consts {
            let Some(init) = &c.init else { continue };
            self.f = FnCtx { is_main: true, ..Default::default() };
            let mut e = self.expr(init);
            if let Some(ann) = &c.ty {
                let t = self.resolve_ann(ann, c.name.span);
                e = self.convert(e, &t, init.span);
            }
            match e.as_const() {
                Some(v) => {
                    if self.consts.insert(c.name.name.clone(), v).is_some() {
                        self.err(c.name.span, format!("duplicate constant '{}'", c.name.name));
                    }
                }
                None => self.err(init.span, "constant initializer must be a constant expression"),
            }
        }
        // type names, then fields and methods
        for (i, t) in prog.types.iter().enumerate() {
            if self.type_map.insert(t.name.name.clone(), i as TypeId).is_some() {
                self.err(t.name.span, format!("duplicate type '{}'", t.name.name));
            }
            self.types.push(TypeDef { name: t.name.name.clone(), fields: Vec::new(), methods: HashMap::new() });
        }
        for (i, t) in prog.types.iter().enumerate() {
            let mut fields: Vec<(String, Ty, FieldInit)> = Vec::new();
            for fd in &t.fields {
                if fields.iter().any(|f| f.0 == fd.name.name) {
                    self.err(fd.name.span, format!("duplicate field '{}'", fd.name.name));
                }
                let (ty, init) = self.field_decl(fd);
                fields.push((fd.name.name.clone(), ty, init));
            }
            self.types[i].fields = fields;
        }
        // function signatures (functions first so ids are stable, then methods)
        for f in &prog.funcs {
            let id = self.declare_func(f, None);
            if self.native_map.contains_key(&f.name.name) {
                self.err(f.name.span, format!("function '{}' conflicts with a built-in command", f.name.name));
            }
            if self.func_map.insert(f.name.name.clone(), id).is_some() {
                self.err(f.name.span, format!("duplicate function '{}'", f.name.name));
            }
        }
        for (ti, t) in prog.types.iter().enumerate() {
            for m in &t.methods {
                let id = self.declare_func(m, Some(ti as TypeId));
                if self.types[ti].methods.insert(m.name.name.clone(), id).is_some() {
                    self.err(m.name.span, format!("duplicate method '{}'", m.name.name));
                }
            }
        }
        // globals and arrays
        self.f = FnCtx { is_main: true, ..Default::default() };
        self.collect_globals(&prog.main);
        self.collect_arrays(&prog.main);
        for f in prog.funcs.iter().chain(prog.types.iter().flat_map(|t| t.methods.iter())) {
            self.collect_arrays(&f.body);
        }

        // smart variable inference
        let all_funcs: Vec<&ast::FuncDecl> =
            prog.funcs.iter().chain(prog.types.iter().flat_map(|t| t.methods.iter())).collect();
        let mut inferred: Vec<HashMap<String, Ty>> = vec![HashMap::new(); all_funcs.len()];
        let mut main_inferred = HashMap::new();
        if !self.classic {
            for _round in 0..8 {
                let mut changed = false;
                for (i, f) in all_funcs.iter().enumerate() {
                    let (locals, ret) = self.infer_function(i as FuncId, f);
                    if locals != inferred[i] {
                        inferred[i] = locals;
                        changed = true;
                    }
                    if let Some(r) = ret {
                        let fi = &mut self.funcs[i];
                        if fi.ret_inferred && fi.ret != r {
                            fi.ret = r;
                            changed = true;
                        }
                    }
                }
                let (m, _) = self.infer_body(&prog.main, HashMap::new(), None, None);
                if m != main_inferred {
                    main_inferred = m;
                    changed = true;
                }
                if !changed {
                    break;
                }
            }
        }

        // bodies
        let mut hfuncs = Vec::new();
        for (i, f) in all_funcs.iter().enumerate() {
            hfuncs.push(self.func_body(i as FuncId, f, std::mem::take(&mut inferred[i])));
        }
        self.f = FnCtx { is_main: true, ret: Ty::Int, inferred: main_inferred, assigned: assigned_names(&prog.main), ..Default::default() };
        let mut body = Vec::new();
        self.block(&prog.main, &mut body);
        self.check_labels();
        let f = std::mem::take(&mut self.f);
        hfuncs.push(HFunc {
            name: "<main>".into(),
            n_params: 0,
            locals: f.locals,
            ret: Ty::Int,
            body,
            n_labels: f.label_defined.len() as u32,
            is_main: true,
            span: Span::default(),
        });

        // data
        self.f = FnCtx { is_main: true, ..Default::default() };
        let mut data = Vec::new();
        for d in &prog.data {
            let e = self.expr(&d.expr);
            match e.as_const() {
                Some(v) => data.push(v),
                None => self.err(d.expr.span, "Data values must be constant"),
            }
        }

        HProgram {
            main: (hfuncs.len() - 1) as FuncId,
            funcs: hfuncs,
            types: self.types.iter().map(|t| HType { name: t.name.clone(), fields: t.fields.clone() }).collect(),
            globals: self.globals.clone(),
            arrays: self.arrays.clone(),
            data,
            n_natives: self.natives.len() as u32,
        }
    }

    fn field_decl(&mut self, fd: &ast::VarDecl) -> (Ty, FieldInit) {
        let explicit = fd.ty.as_ref().map(|a| self.resolve_ann(a, fd.name.span));
        if let Some(n) = &fd.vector {
            let elem = explicit.unwrap_or(Ty::Int);
            let len = self.const_len(n);
            let k = elem.kind();
            return (Ty::Vector(Box::new(elem)), FieldInit::Vector { len, elem: k });
        }
        self.f = FnCtx { is_main: true, ..Default::default() };
        let init = fd.init.as_ref().map(|e| self.expr(e));
        let ty = match (explicit, &init) {
            (Some(t), _) => t,
            (None, Some(e)) if !self.classic && e.ty != Ty::Null => e.ty.clone(),
            _ => Ty::Int,
        };
        let finit = match init {
            Some(e) => {
                let e = self.convert(e, &ty, fd.span);
                match (&e.kind, e.as_const()) {
                    (_, Some(c)) => FieldInit::Value(c),
                    (HKind::Null, _) => FieldInit::Null,
                    _ => {
                        self.err(fd.span, "field initializer must be a constant expression");
                        FieldInit::Null
                    }
                }
            }
            None => self.default_field(&ty),
        };
        (ty, finit)
    }

    fn default_field(&self, ty: &Ty) -> FieldInit {
        match ty {
            Ty::Int => FieldInit::Value(ConstVal::Int(0)),
            Ty::Float => FieldInit::Value(ConstVal::Float(0.0)),
            Ty::Str => FieldInit::Value(ConstVal::Str("".into())),
            Ty::Variant => FieldInit::Variant,
            _ => FieldInit::Null,
        }
    }

    fn const_len(&mut self, e: &Expr) -> u32 {
        self.f = FnCtx { is_main: true, ..Default::default() };
        let h = self.expr(e);
        let h = self.convert(h, &Ty::Int, e.span);
        match h.kind {
            HKind::Int(n) if n >= 0 => n as u32 + 1,
            _ => {
                self.err(e.span, "array size must be a non-negative integer constant");
                1
            }
        }
    }

    fn declare_func(&mut self, f: &ast::FuncDecl, owner: Option<TypeId>) -> FuncId {
        let mut params = Vec::new();
        if let Some(t) = owner {
            params.push(("self".to_string(), Ty::Obj(t), None));
        }
        for p in &f.params {
            self.f = FnCtx { is_main: true, ..Default::default() };
            let explicit = p.ty.as_ref().map(|a| self.resolve_ann(a, p.name.span));
            let def = p.init.as_ref().map(|e| self.expr(e));
            let ty = match (&explicit, &def) {
                (Some(t), _) => t.clone(),
                (None, Some(d)) if !self.classic && d.ty.is_numeric() || d_is_str(&def) && !self.classic => {
                    def.as_ref().unwrap().ty.clone()
                }
                _ => Ty::Int,
            };
            let default = def.map(|d| {
                let d = self.convert(d, &ty, p.span);
                match (d.as_const(), &d.kind) {
                    (Some(c), _) => Some(c),
                    (None, HKind::Null) => None,
                    _ => {
                        self.err(p.span, "default parameter value must be constant");
                        None
                    }
                }
            });
            if params.iter().any(|(n, _, _)| *n == p.name.name) {
                self.err(p.name.span, format!("duplicate parameter '{}'", p.name.name));
            }
            // `Null` defaults are represented with an Int 0 marker and fixed up at call sites
            let default = match default {
                Some(Some(c)) => Some(c),
                Some(None) => Some(ConstVal::Int(0)),
                None => None,
            };
            params.push((p.name.name.clone(), ty, default));
        }
        let (ret, ret_inferred) = match &f.ret {
            Some(a) => (self.resolve_ann(a, f.name.span), false),
            None => (Ty::Int, !self.classic),
        };
        let display = match owner {
            Some(t) => format!("{}\\{}", self.types[t as usize].name, f.name.name),
            None => f.name.name.clone(),
        };
        self.funcs.push(FuncInfo { display, params, ret, ret_inferred, owner });
        (self.funcs.len() - 1) as FuncId
    }

    fn collect_globals(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            if let StmtKind::Decl(DeclKind::Global, d) = &s.kind {
                let explicit = d.ty.as_ref().map(|a| self.resolve_ann(a, d.name.span));
                let ty = if d.vector.is_some() {
                    Ty::Vector(Box::new(explicit.unwrap_or(Ty::Int)))
                } else {
                    match explicit {
                        Some(t) => t,
                        None if !self.classic => d
                            .init
                            .as_ref()
                            .and_then(|e| self.infer_ty(e, &HashMap::new(), None))
                            .filter(|t| *t != Ty::Null && *t != Ty::Void)
                            .unwrap_or(Ty::Int),
                        None => Ty::Int,
                    }
                };
                if self.global_map.contains_key(&d.name.name) {
                    self.err(d.name.span, format!("duplicate global '{}'", d.name.name));
                    continue;
                }
                if self.consts.contains_key(&d.name.name) {
                    self.err(d.name.span, format!("'{}' is already a constant", d.name.name));
                }
                self.global_map.insert(d.name.name.clone(), self.globals.len() as u32);
                self.globals.push((d.name.name.clone(), ty));
            }
        }
    }

    fn collect_arrays(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            match &s.kind {
                StmtKind::Dim { name, tag, dims } => {
                    if !self.array_map.contains_key(&name.name) {
                        let ty = tag.as_ref().map(|a| self.resolve_ann(a, name.span)).unwrap_or(Ty::Int);
                        self.array_map.insert(name.name.clone(), self.arrays.len() as u32);
                        self.arrays.push((name.name.clone(), ty, dims.len() as u8));
                    }
                }
                _ => for_each_block(&s.kind, |b| self.collect_arrays(b)),
            }
        }
    }

    // ------------------------------------------------------------------
    // smart variable inference

    fn infer_function(&mut self, id: FuncId, f: &ast::FuncDecl) -> (HashMap<String, Ty>, Option<Ty>) {
        let info = &self.funcs[id as usize];
        let mut fixed = HashMap::new();
        for (n, t, _) in &info.params {
            fixed.insert(n.clone(), t.clone());
        }
        let ret_acc = if info.ret_inferred { Some(None) } else { None };
        let owner = info.owner;
        let (map, ret) = self.infer_body(&f.body, fixed, ret_acc, owner);
        (map, ret.flatten())
    }

    /// Walks a body collecting types for untagged, undeclared-type locals.
    fn infer_body(
        &self,
        body: &[Stmt],
        fixed: HashMap<String, Ty>,
        mut ret: Option<Option<Ty>>,
        self_ty: Option<TypeId>,
    ) -> (HashMap<String, Ty>, Option<Option<Ty>>) {
        let mut st = InferState { vars: fixed.clone(), fixed: fixed.keys().cloned().collect(), self_ty };
        self.infer_stmts(body, &mut st, &mut ret);
        for k in &st.fixed {
            if !fixed.contains_key(k) {
                continue;
            }
            st.vars.remove(k);
        }
        (st.vars, ret)
    }

    fn infer_assign(&self, var: &Var, rhs: Option<Ty>, st: &mut InferState) {
        if let Var::Ident { name, tag } = var {
            if let Some(tag) = tag {
                if !st.vars.contains_key(&name.name) {
                    if let Some(t) = self.ann_quiet(tag) {
                        st.vars.insert(name.name.clone(), t);
                        st.fixed.insert(name.name.clone());
                    }
                }
                return;
            }
            if st.fixed.contains(&name.name) {
                return;
            }
            if !st.vars.contains_key(&name.name)
                && (self.global_map.contains_key(&name.name) || self.consts.contains_key(&name.name))
            {
                return;
            }
            if let Some(t) = rhs.filter(|t| *t != Ty::Void) {
                let u = unify(st.vars.get(&name.name), t);
                if u != Ty::Null {
                    st.vars.insert(name.name.clone(), u);
                }
            }
        }
    }

    fn infer_stmts(&self, body: &[Stmt], st: &mut InferState, ret: &mut Option<Option<Ty>>) {
        for s in body {
            match &s.kind {
                StmtKind::Assign(v, e) => {
                    let t = self.infer_ty(e, &st.vars, st.self_ty);
                    self.infer_assign(v, t, st);
                }
                StmtKind::CompoundAssign(v, op, e) => {
                    let cur = self.infer_ty(&Expr { kind: ExprKind::Var(Box::new(v.clone())), span: s.span }, &st.vars, st.self_ty);
                    let r = self.infer_ty(e, &st.vars, st.self_ty);
                    let t = infer_binary(*op, cur, r);
                    self.infer_assign(v, t, st);
                }
                StmtKind::Decl(DeclKind::Local, d) => {
                    if let Some(a) = &d.ty {
                        if let Some(t) = self.ann_quiet(a) {
                            let t = if d.vector.is_some() { Ty::Vector(Box::new(t)) } else { t };
                            st.vars.insert(d.name.name.clone(), t);
                            st.fixed.insert(d.name.name.clone());
                        }
                    } else if d.vector.is_some() {
                        st.vars.insert(d.name.name.clone(), Ty::Vector(Box::new(Ty::Int)));
                        st.fixed.insert(d.name.name.clone());
                    } else if let Some(e) = &d.init {
                        let t = self.infer_ty(e, &st.vars, st.self_ty);
                        st.vars.entry(d.name.name.clone()).or_insert(Ty::Int);
                        if let Some(t) = t.filter(|t| *t != Ty::Null && *t != Ty::Void) {
                            // a fresh Local takes the initializer type, later assignments may widen it
                            let cur = st.vars.get(&d.name.name).cloned();
                            let u = if cur == Some(Ty::Int) && t != Ty::Int && t != Ty::Float { t } else { unify(cur.as_ref(), t) };
                            st.vars.insert(d.name.name.clone(), u);
                        }
                    } else {
                        st.vars.entry(d.name.name.clone()).or_insert(Ty::Int);
                    }
                }
                StmtKind::For { var, from, to, step, body } => {
                    let mut t = self.infer_ty(from, &st.vars, st.self_ty);
                    for e in std::iter::once(to).chain(step.iter()) {
                        if let Some(x) = self.infer_ty(e, &st.vars, st.self_ty) {
                            t = Some(unify(t.as_ref(), x));
                        }
                    }
                    self.infer_assign(var, t, st);
                    self.infer_stmts(body, st, ret);
                }
                StmtKind::ForEach { var, ty, body } => {
                    if let Some(&t) = self.type_map.get(&ty.name) {
                        self.infer_assign(var, Some(Ty::Obj(t)), st);
                    }
                    self.infer_stmts(body, st, ret);
                }
                StmtKind::ForIn { var, iter, body } => {
                    let t = match self.infer_ty(iter, &st.vars, st.self_ty) {
                        Some(Ty::Vector(e)) => Some(*e),
                        _ => match &iter.kind {
                            ExprKind::Var(v) => match &**v {
                                Var::Ident { name, tag: None } => self.type_map.get(&name.name).map(|&t| Ty::Obj(t)),
                                _ => None,
                            },
                            _ => None,
                        },
                    };
                    self.infer_assign(&Var::Ident { name: var.clone(), tag: None }, t, st);
                    self.infer_stmts(body, st, ret);
                }
                StmtKind::Try { body, var, handler } => {
                    self.infer_stmts(body, st, ret);
                    if let Some(v) = var {
                        self.infer_assign(v, Some(Ty::Str), st);
                    }
                    self.infer_stmts(handler, st, ret);
                }
                StmtKind::Return(Some(e)) => {
                    if let Some(r) = ret.as_mut() {
                        if let Some(t) = self.infer_ty(e, &st.vars, st.self_ty).filter(|t| *t != Ty::Void) {
                            *r = Some(unify(r.as_ref(), t));
                        }
                    }
                }
                k => for_each_block(k, |b| self.infer_stmts(b, st, ret)),
            }
        }
    }

    fn ann_quiet(&self, a: &TypeAnn) -> Option<Ty> {
        Some(match a {
            TypeAnn::Int => Ty::Int,
            TypeAnn::Float => Ty::Float,
            TypeAnn::Str => Ty::Str,
            TypeAnn::Variant => Ty::Variant,
            TypeAnn::Obj(n) if self.enums.contains_key(n) => Ty::Int,
            TypeAnn::Obj(n) => Ty::Obj(*self.type_map.get(n)?),
        })
    }

    /// Best-effort static type of an expression, without diagnostics.
    fn infer_ty(&self, e: &Expr, vars: &HashMap<String, Ty>, self_ty: Option<TypeId>) -> Option<Ty> {
        match &e.kind {
            ExprKind::Int(_) => Some(Ty::Int),
            ExprKind::Float(_) => Some(Ty::Float),
            ExprKind::Str(_) | ExprKind::Interp(_) => Some(Ty::Str),
            ExprKind::Null => Some(Ty::Null),
            ExprKind::SelfRef => self_ty.map(Ty::Obj),
            ExprKind::Var(v) => match &**v {
                Var::Ident { name, tag } => {
                    if self.enum_member(&name.name, tag.as_ref(), vars.contains_key(&name.name)).is_some() {
                        return Some(Ty::Int);
                    }
                    if let Some(t) = tag {
                        return self.ann_quiet(t);
                    }
                    if let Some(t) = vars.get(&name.name) {
                        return Some(t.clone());
                    }
                    if let Some(&g) = self.global_map.get(&name.name) {
                        return Some(self.globals[g as usize].1.clone());
                    }
                    self.consts.get(&name.name).map(const_ty)
                }
                Var::Array { name, .. } => self.array_map.get(&name.name).map(|&a| self.arrays[a as usize].1.clone()),
                Var::Field { target, field, .. } => match self.infer_ty(target, vars, self_ty)? {
                    Ty::Obj(t) => self.types[t as usize].fields.iter().find(|f| f.0 == field.name).map(|f| f.1.clone()),
                    _ => None,
                },
                Var::Vector { target, .. } => match self.infer_ty(target, vars, self_ty)? {
                    Ty::Vector(e) => Some(*e),
                    _ => None,
                },
            },
            ExprKind::Call { name, .. } => {
                if let Some(&f) = self.func_map.get(&name.name) {
                    Some(self.funcs[f as usize].ret.clone())
                } else if let Some(&n) = self.native_map.get(&name.name) {
                    Some(self.natives[n as usize].ret.map(native_ty).unwrap_or(Ty::Void))
                } else {
                    None
                }
            }
            ExprKind::MethodCall { target, method, .. } => match self.infer_ty(target, vars, self_ty)? {
                Ty::Obj(t) => self.types[t as usize].methods.get(&method.name).map(|&f| self.funcs[f as usize].ret.clone()),
                _ => None,
            },
            ExprKind::Unary(op, x) => match op {
                UnOp::Not | UnOp::BitNot => Some(Ty::Int),
                _ => self.infer_ty(x, vars, self_ty),
            },
            ExprKind::Binary(op, l, r) => {
                infer_binary(*op, self.infer_ty(l, vars, self_ty), self.infer_ty(r, vars, self_ty))
            }
            ExprKind::Cast(c, _) => Some(match c {
                CastTo::Int => Ty::Int,
                CastTo::Float => Ty::Float,
                CastTo::Str => Ty::Str,
            }),
            ExprKind::New(t) | ExprKind::First(t) | ExprKind::Last(t) | ExprKind::Object(t, _) => {
                self.type_map.get(&t.name).map(|&t| Ty::Obj(t))
            }
            ExprKind::After(x) | ExprKind::Before(x) => self.infer_ty(x, vars, self_ty),
            ExprKind::Handle(_) => Some(Ty::Int),
        }
    }

    // ------------------------------------------------------------------
    // function bodies

    fn func_body(&mut self, id: FuncId, f: &ast::FuncDecl, inferred: HashMap<String, Ty>) -> HFunc {
        let info = &self.funcs[id as usize];
        let mut ctx = FnCtx { ret: info.ret.clone(), self_ty: info.owner, inferred, assigned: assigned_names(&f.body), ..Default::default() };
        for (n, t, _) in &info.params {
            ctx.map.insert(n.clone(), ctx.locals.len() as u32);
            ctx.locals.push((n.clone(), t.clone()));
        }
        let n_params = info.params.len() as u32;
        let display = info.display.clone();
        let ret = info.ret.clone();
        self.f = ctx;
        let mut body = Vec::new();
        self.block(&f.body, &mut body);
        self.check_labels();
        let ctx = std::mem::take(&mut self.f);
        HFunc {
            name: display,
            n_params,
            locals: ctx.locals,
            ret,
            body,
            n_labels: ctx.label_defined.len() as u32,
            is_main: false,
            span: f.span,
        }
    }

    fn check_labels(&mut self) {
        let mut missing = Vec::new();
        for (name, &id) in &self.f.labels {
            if !self.f.label_defined[id as usize] {
                missing.push((self.f.label_spans[id as usize], name.clone()));
            }
        }
        for (sp, n) in missing {
            self.err(sp, format!("label '{n}' not found"));
        }
    }

    fn label(&mut self, name: &str, span: Span) -> u32 {
        if let Some(&l) = self.f.labels.get(name) {
            return l;
        }
        let id = self.f.label_defined.len() as u32;
        self.f.labels.insert(name.to_string(), id);
        self.f.label_defined.push(false);
        self.f.label_spans.push(span);
        id
    }

    fn new_tmp(&mut self, ty: Ty) -> u32 {
        self.f.n_tmp += 1;
        let name = format!("\0tmp{}", self.f.n_tmp);
        self.f.locals.push((name, ty));
        (self.f.locals.len() - 1) as u32
    }

    fn declare_local(&mut self, name: &str, ty: Ty) -> u32 {
        let slot = self.f.locals.len() as u32;
        self.f.locals.push((name.to_string(), ty));
        self.f.map.insert(name.to_string(), slot);
        slot
    }

    fn block(&mut self, stmts: &[Stmt], out: &mut Vec<HStmt>) {
        for s in stmts {
            self.stmt(s, out);
        }
    }

    fn sub_block(&mut self, stmts: &[Stmt]) -> Vec<HStmt> {
        let mut v = Vec::new();
        self.block(stmts, &mut v);
        v
    }

    fn loop_block(&mut self, stmts: &[Stmt]) -> Vec<HStmt> {
        self.f.loop_depth += 1;
        let b = self.sub_block(stmts);
        self.f.loop_depth -= 1;
        b
    }

    fn default_value(&self, ty: &Ty, span: Span) -> HExpr {
        match ty {
            Ty::Float => HExpr { ty: Ty::Float, kind: HKind::Float(0.0), span },
            Ty::Str => HExpr { ty: Ty::Str, kind: HKind::Str("".into()), span },
            Ty::Obj(_) | Ty::Null | Ty::Vector(_) => HExpr { ty: ty.clone(), kind: HKind::Null, span },
            Ty::Variant => HExpr { ty: Ty::Variant, kind: HKind::Convert(Box::new(HExpr::int(0, span))), span },
            _ => HExpr::int(0, span),
        }
    }

    fn stmt(&mut self, s: &Stmt, out: &mut Vec<HStmt>) {
        let span = s.span;
        let kind = match &s.kind {
            StmtKind::Expr(e) => {
                let h = match &e.kind {
                    ExprKind::Call { .. } | ExprKind::MethodCall { .. } => self.expr(e),
                    _ => {
                        self.err(e.span, "expression statement must be a call");
                        return;
                    }
                };
                let pop = h.ty != Ty::Void;
                HStmtKind::Expr(h, pop)
            }
            StmtKind::Assign(v, rhs) => {
                let r = self.expr(rhs);
                let (place, ty) = match self.place(v, Some(&r.ty)) {
                    Some(p) => p,
                    None => return,
                };
                let r = self.convert(r, &ty, rhs.span);
                HStmtKind::Store(place, r)
            }
            StmtKind::CompoundAssign(v, op, rhs) => {
                let Some((place, ty)) = self.place(v, None) else { return };
                let cur = HExpr { ty: ty.clone(), kind: HKind::Load(place.clone()), span: v.span() };
                let r = self.expr(rhs);
                let val = self.binary(*op, cur, r, span);
                let val = self.convert(val, &ty, span);
                HStmtKind::Store(place, val)
            }
            StmtKind::Decl(kind, d) => {
                self.decl(*kind, d, out);
                return;
            }
            StmtKind::Dim { name, dims, .. } => {
                let Some(&a) = self.array_map.get(&name.name) else { return };
                if dims.len() as u8 != self.arrays[a as usize].2 {
                    self.err(span, format!("array '{}' redimensioned with a different number of dimensions", name.name));
                }
                let dims = dims
                    .iter()
                    .map(|d| {
                        let h = self.expr(d);
                        self.convert(h, &Ty::Int, d.span)
                    })
                    .collect();
                HStmtKind::Dim { array: a, dims }
            }
            StmtKind::If { cond, then, els } => {
                let c = self.cond(cond);
                let t = self.sub_block(then);
                let e = els.as_ref().map(|e| self.sub_block(e)).unwrap_or_default();
                HStmtKind::If(c, t, e)
            }
            StmtKind::While { cond, body } => {
                let c = self.cond(cond);
                HStmtKind::While(c, self.loop_block(body))
            }
            StmtKind::Repeat { body, until } => {
                let b = self.loop_block(body);
                let u = until.as_ref().map(|c| self.cond(c));
                HStmtKind::Repeat(b, u)
            }
            StmtKind::For { var, from, to, step, body } => {
                let f = self.expr(from);
                let Some((place, ty)) = self.place(var, Some(&f.ty)) else { return };
                if !matches!(place, Place::Local(_) | Place::Global(_)) {
                    self.err(var.span(), "For loop variable must be a simple variable");
                    return;
                }
                let kind = match ty {
                    Ty::Int => NumKind::I,
                    Ty::Float => NumKind::F,
                    _ => {
                        let n = self.tn(&ty);
                        self.err(var.span(), format!("For loop variable must be numeric, not {n}"));
                        return;
                    }
                };
                let f = self.convert(f, &ty, from.span);
                let t = self.expr(to);
                let t = self.convert(t, &ty, to.span);
                let step_e = match step {
                    Some(st) => {
                        let h = self.expr(st);
                        self.convert(h, &ty, st.span)
                    }
                    None => self.convert(HExpr::int(1, span), &ty, span),
                };
                let step = match step_e.kind {
                    HKind::Int(v) => ForStep::Const(step_e.clone(), v >= 0),
                    HKind::Float(v) => ForStep::Const(step_e.clone(), v >= 0.0),
                    _ => {
                        let tmp = self.new_tmp(ty.clone());
                        ForStep::Dynamic(step_e, tmp)
                    }
                };
                let to_tmp = self.new_tmp(ty.clone());
                let body = self.loop_block(body);
                HStmtKind::For { var: place, kind, from: f, to: t, to_tmp, step, body }
            }
            StmtKind::ForEach { var, ty, body } => {
                let Some(&tid) = self.type_map.get(&ty.name) else {
                    self.err(ty.span, format!("type '{}' not found", ty.name));
                    return;
                };
                let Some((place, vty)) = self.place(var, Some(&Ty::Obj(tid))) else { return };
                if vty != Ty::Obj(tid) {
                    let n = self.tn(&vty);
                    self.err(var.span(), format!("For Each variable has type {n}, expected {}", ty.name));
                    return;
                }
                let next_tmp = self.new_tmp(Ty::Obj(tid));
                let body = self.loop_block(body);
                HStmtKind::ForEach { var: place, ty: tid, next_tmp, body }
            }
            StmtKind::ForIn { var, iter, body } => {
                // a Type name iterates its object list, anything else must be a Blitz array
                if let ExprKind::Var(v) = &iter.kind {
                    if let Var::Ident { name, tag: None } = &**v {
                        let is_var = self.f.map.contains_key(&name.name) || self.global_map.contains_key(&name.name);
                        if let (false, Some(&tid)) = (is_var, self.type_map.get(&name.name)) {
                            let target = Var::Ident { name: var.clone(), tag: None };
                            let Some((place, vty)) = self.place(&target, Some(&Ty::Obj(tid))) else { return };
                            if vty != Ty::Obj(tid) {
                                let n = self.tn(&vty);
                                self.err(var.span, format!("For Each variable has type {n}, expected {}", name.name));
                                return;
                            }
                            let next_tmp = self.new_tmp(Ty::Obj(tid));
                            let body = self.loop_block(body);
                            out.push(HStmt { kind: HStmtKind::ForEach { var: place, ty: tid, next_tmp, body }, span });
                            return;
                        }
                    }
                }
                let vec = self.expr(iter);
                let Ty::Vector(elem) = vec.ty.clone() else {
                    let n = self.tn(&vec.ty);
                    self.err(iter.span, format!("For Each ... In expects a Type name or a Blitz array, found {n}"));
                    return;
                };
                let target = Var::Ident { name: var.clone(), tag: None };
                let Some((place, vty)) = self.place(&target, Some(&elem)) else { return };
                if vty != *elem {
                    let (a, b) = (self.tn(&vty), self.tn(&elem));
                    self.err(var.span, format!("For Each variable has type {a}, array elements are {b}"));
                    return;
                }
                let vec_tmp = self.new_tmp(vec.ty.clone());
                let idx_tmp = self.new_tmp(Ty::Int);
                let body = self.loop_block(body);
                HStmtKind::ForVector { var: place, vec, vec_tmp, idx_tmp, body }
            }
            StmtKind::Select { expr, cases, default } => {
                let v = self.expr(expr);
                let tmp = self.new_tmp(v.ty.clone());
                let tmp_ty = v.ty.clone();
                let mut hcases = Vec::new();
                for (vals, body) in cases {
                    let mut conds = Vec::new();
                    for c in vals {
                        let lhs = HExpr { ty: tmp_ty.clone(), kind: HKind::Load(Place::Local(tmp)), span: c.span };
                        let rhs = self.expr(c);
                        conds.push(self.binary(BinOp::Eq, lhs, rhs, c.span));
                    }
                    let b = self.sub_block(body);
                    hcases.push((conds, b));
                }
                let d = default.as_ref().map(|d| self.sub_block(d)).unwrap_or_default();
                HStmtKind::Select { tmp, value: v, cases: hcases, default: d }
            }
            StmtKind::Exit | StmtKind::Continue => {
                if self.f.loop_depth == 0 {
                    let w = if matches!(s.kind, StmtKind::Exit) { "Exit" } else { "Continue" };
                    self.err(span, format!("'{w}' must appear inside a loop"));
                    return;
                }
                if matches!(s.kind, StmtKind::Exit) { HStmtKind::Exit } else { HStmtKind::Continue }
            }
            StmtKind::Return(e) => {
                if self.f.is_main {
                    if let Some(e) = e {
                        self.err(e.span, "'Return' in the main program cannot return a value");
                    }
                    HStmtKind::GosubReturn
                } else {
                    let ret = self.f.ret.clone();
                    match e {
                        Some(e) => {
                            let h = self.expr(e);
                            HStmtKind::Return(Some(self.convert(h, &ret, e.span)))
                        }
                        None => HStmtKind::Return(None),
                    }
                }
            }
            StmtKind::Goto(l) => HStmtKind::Goto(self.label(&l.name, l.span)),
            StmtKind::Gosub(l) => HStmtKind::Gosub(self.label(&l.name, l.span)),
            StmtKind::Label(l) => {
                let id = self.label(&l.name, l.span);
                if self.f.label_defined[id as usize] {
                    self.err(l.span, format!("duplicate label '{}'", l.name));
                }
                self.f.label_defined[id as usize] = true;
                HStmtKind::Label(id)
            }
            StmtKind::Delete(e) => {
                let h = self.expr(e);
                if !matches!(h.ty, Ty::Obj(_)) {
                    let n = self.tn(&h.ty);
                    self.err(e.span, format!("'Delete' expects an object, found {n}"));
                    return;
                }
                HStmtKind::Delete(h)
            }
            StmtKind::DeleteEach(t) => match self.type_map.get(&t.name) {
                Some(&id) => HStmtKind::DeleteEach(id),
                None => {
                    self.err(t.span, format!("type '{}' not found", t.name));
                    return;
                }
            },
            StmtKind::Insert { obj, before, target } => {
                let o = self.expr(obj);
                let t = self.expr(target);
                if !matches!(o.ty, Ty::Obj(_)) || o.ty != t.ty {
                    self.err(span, "'Insert' requires two objects of the same type");
                    return;
                }
                HStmtKind::Insert { obj: o, before: *before, target: t }
            }
            StmtKind::Read(vars) => {
                for v in vars {
                    if let Some((p, ty)) = self.place(v, None) {
                        if !matches!(ty, Ty::Int | Ty::Float | Ty::Str | Ty::Variant) {
                            self.err(v.span(), "'Read' requires a numeric or string variable");
                            continue;
                        }
                        out.push(HStmt { kind: HStmtKind::Read(p, ty.kind()), span });
                    }
                }
                return;
            }
            StmtKind::Restore(l) => match l {
                None => HStmtKind::Restore(0),
                Some(l) => match self.data_labels.get(&l.name) {
                    Some(&i) => HStmtKind::Restore(i),
                    None => {
                        self.err(l.span, format!("data label '{}' not found", l.name));
                        return;
                    }
                },
            },
            StmtKind::Try { body, var, handler } => {
                let b = self.sub_block(body);
                let catch = match var {
                    Some(v) => match self.place(v, Some(&Ty::Str)) {
                        Some((p, Ty::Str | Ty::Variant)) => Some(p),
                        Some(_) => {
                            self.err(v.span(), "Catch variable must be a String");
                            None
                        }
                        None => None,
                    },
                    None => None,
                };
                let h = self.sub_block(handler);
                HStmtKind::Try { body: b, catch, handler: h }
            }
            StmtKind::Throw(e) => {
                let h = self.expr(e);
                HStmtKind::Throw(self.convert(h, &Ty::Str, e.span))
            }
            StmtKind::End => HStmtKind::End,
            StmtKind::Stop => HStmtKind::Stop,
        };
        out.push(HStmt { kind, span });
    }

    fn decl(&mut self, kind: DeclKind, d: &ast::VarDecl, out: &mut Vec<HStmt>) {
        let span = d.span;
        let explicit = d.ty.as_ref().map(|a| self.resolve_ann(a, d.name.span));
        match kind {
            DeclKind::Global => {
                let Some(&g) = self.global_map.get(&d.name.name) else { return };
                let ty = self.globals[g as usize].1.clone();
                if let (Some(n), Ty::Vector(e)) = (&d.vector, &ty) {
                    let len = self.const_len_here(n);
                    out.push(HStmt { kind: HStmtKind::NewVector(Place::Global(g), len, e.kind()), span });
                } else if let Some(init) = &d.init {
                    let h = self.expr(init);
                    let h = self.convert(h, &ty, init.span);
                    out.push(HStmt { kind: HStmtKind::Store(Place::Global(g), h), span });
                }
            }
            DeclKind::Local => {
                if self.f.map.contains_key(&d.name.name) {
                    self.err(d.name.span, format!("duplicate variable '{}'", d.name.name));
                    return;
                }
                if let Some(n) = &d.vector {
                    let elem = explicit.unwrap_or(Ty::Int);
                    let len = self.const_len_here(n);
                    let k = elem.kind();
                    let slot = self.declare_local(&d.name.name, Ty::Vector(Box::new(elem)));
                    out.push(HStmt { kind: HStmtKind::NewVector(Place::Local(slot), len, k), span });
                    return;
                }
                let init = d.init.as_ref().map(|e| self.expr(e));
                let ty = match (explicit, &init) {
                    (Some(t), _) => t,
                    _ if !self.classic && self.f.inferred.contains_key(&d.name.name) => self.f.inferred[&d.name.name].clone(),
                    (None, Some(e)) if !self.classic && !matches!(e.ty, Ty::Null | Ty::Void) => e.ty.clone(),
                    _ => Ty::Int,
                };
                let slot = self.declare_local(&d.name.name, ty.clone());
                let value = match init {
                    Some(e) => self.convert(e, &ty, span),
                    None => self.default_value(&ty, span),
                };
                out.push(HStmt { kind: HStmtKind::Store(Place::Local(slot), value), span });
            }
            _ => {}
        }
    }

    fn const_len_here(&mut self, e: &Expr) -> u32 {
        let h = self.expr(e);
        let h = self.convert(h, &Ty::Int, e.span);
        match h.kind {
            HKind::Int(n) if n >= 0 => n as u32 + 1,
            _ => {
                self.err(e.span, "array size must be a non-negative integer constant");
                1
            }
        }
    }

    /// Converts a condition to an Int truth value.
    fn cond(&mut self, e: &Expr) -> HExpr {
        let h = self.expr(e);
        self.truthy(h)
    }

    fn truthy(&mut self, h: HExpr) -> HExpr {
        let span = h.span;
        match h.ty {
            Ty::Int => h,
            Ty::Void => {
                self.err(span, "expression does not produce a value");
                HExpr::int(0, span)
            }
            ref t => {
                let k = num_kind(t);
                // x <> 0  ==  Not(Not x)
                let not = HExpr { ty: Ty::Int, kind: HKind::Unary(HUnary::Not, k, Box::new(h)), span };
                HExpr { ty: Ty::Int, kind: HKind::Unary(HUnary::Not, NumKind::I, Box::new(not)), span }
            }
        }
    }

    // ------------------------------------------------------------------
    // places

    /// Resolves a variable for reading or writing. `hint` is the type of the
    /// value being assigned (used when the variable is implicitly declared).
    fn place(&mut self, v: &Var, hint: Option<&Ty>) -> Option<(Place, Ty)> {
        match v {
            Var::Ident { name, tag } => {
                let shadowed = self.f.map.contains_key(&name.name) || self.global_map.contains_key(&name.name);
                if self.enum_member(&name.name, tag.as_ref(), shadowed).is_some() {
                    self.err(name.span, format!("cannot assign to enum member of '{}'", name.name));
                    return None;
                }
                let tagty = tag.as_ref().map(|t| self.resolve_ann(t, name.span));
                let (p, ty) = if let Some(&slot) = self.f.map.get(&name.name) {
                    (Place::Local(slot), self.f.locals[slot as usize].1.clone())
                } else if let Some(&g) = self.global_map.get(&name.name) {
                    (Place::Global(g), self.globals[g as usize].1.clone())
                } else if self.consts.contains_key(&name.name) {
                    self.err(name.span, format!("cannot assign to constant '{}'", name.name));
                    return None;
                } else {
                    if self.strict {
                        self.err(name.span, format!("variable '{}' is not declared (Strict mode)", name.name));
                    }
                    let ty = match &tagty {
                        Some(t) => t.clone(),
                        None if self.classic => Ty::Int,
                        None => self
                            .f
                            .inferred
                            .get(&name.name)
                            .cloned()
                            .or_else(|| hint.filter(|h| !matches!(h, Ty::Null | Ty::Void)).cloned())
                            .unwrap_or(Ty::Int),
                    };
                    let slot = self.declare_local(&name.name, ty.clone());
                    (Place::Local(slot), ty)
                };
                if let Some(t) = tagty {
                    if t != ty {
                        let (a, b) = (self.tn(&ty), self.tn(&t));
                        self.err(name.span, format!("variable '{}' is {a}, but used with a {b} type tag", name.name));
                    }
                }
                Some((p, ty))
            }
            Var::Array { name, index, .. } => {
                let Some(&a) = self.array_map.get(&name.name) else {
                    self.err(name.span, format!("array '{}' not found", name.name));
                    return None;
                };
                let (_, elem, dims) = self.arrays[a as usize].clone();
                if index.len() != dims as usize {
                    self.err(v.span(), format!("array '{}' has {dims} dimension(s), {} index(es) given", name.name, index.len()));
                    return None;
                }
                let idx = index
                    .iter()
                    .map(|e| {
                        let h = self.expr(e);
                        self.convert(h, &Ty::Int, e.span)
                    })
                    .collect();
                Some((Place::Array { array: a, index: idx }, elem))
            }
            Var::Field { target, field, tag } => {
                let obj = self.expr(target);
                let Ty::Obj(t) = obj.ty else {
                    let n = self.tn(&obj.ty);
                    self.err(target.span, format!("'\\' field access requires an object, found {n}"));
                    return None;
                };
                let tdef = &self.types[t as usize];
                let Some(i) = tdef.fields.iter().position(|f| f.0 == field.name) else {
                    let tn = tdef.name.clone();
                    self.err(field.span, format!("type '{tn}' has no field '{}'", field.name));
                    return None;
                };
                let fty = tdef.fields[i].1.clone();
                if let Some(tag) = tag {
                    let tt = self.resolve_ann(tag, field.span);
                    if tt != fty {
                        self.err(field.span, format!("field '{}' type tag mismatch", field.name));
                    }
                }
                Some((Place::Field { obj: Box::new(obj), index: i as u32 }, fty))
            }
            Var::Vector { target, index } => {
                let vec = self.expr(target);
                let Ty::Vector(elem) = vec.ty.clone() else {
                    let n = self.tn(&vec.ty);
                    self.err(target.span, format!("'[ ]' indexing requires a Blitz array, found {n}"));
                    return None;
                };
                let i = self.expr(index);
                let i = self.convert(i, &Ty::Int, index.span);
                Some((Place::Vector { vec: Box::new(vec), index: Box::new(i) }, *elem))
            }
        }
    }

    // ------------------------------------------------------------------
    // expressions

    fn expr(&mut self, e: &Expr) -> HExpr {
        let span = e.span;
        let mk = |ty: Ty, kind: HKind| HExpr { ty, kind, span };
        match &e.kind {
            ExprKind::Int(v) => mk(Ty::Int, HKind::Int(*v)),
            ExprKind::Float(v) => mk(Ty::Float, HKind::Float(*v)),
            ExprKind::Str(s) => mk(Ty::Str, HKind::Str(s.as_str().into())),
            ExprKind::Null => mk(Ty::Null, HKind::Null),
            ExprKind::SelfRef => match self.f.self_ty {
                Some(t) => mk(Ty::Obj(t), HKind::Load(Place::Local(0))),
                None => mk(Ty::Null, HKind::Null),
            },
            ExprKind::Var(v) => {
                if let Var::Ident { name, tag } = &**v {
                    let shadowed = self.f.map.contains_key(&name.name) || self.global_map.contains_key(&name.name);
                    if let Some(r) = self.enum_member(&name.name, tag.as_ref(), shadowed) {
                        return match r {
                            Ok(v) => HExpr::int(v, span),
                            Err(msg) => {
                                self.err(span, msg);
                                HExpr::int(0, span)
                            }
                        };
                    }
                }
                if let Var::Ident { name, tag: None } = &**v {
                    if !self.f.map.contains_key(&name.name) && !self.global_map.contains_key(&name.name) {
                        if let Some(c) = self.consts.get(&name.name) {
                            return const_expr(c, span);
                        }
                        if !self.classic && !self.strict && !self.f.inferred.contains_key(&name.name) && !self.f.assigned.contains(&name.name) {
                            self.diags.warning(span, format!("variable '{}' is used but never assigned", name.name));
                        }
                    }
                }
                match self.place(v, None) {
                    Some((p, ty)) => mk(ty, HKind::Load(p)),
                    None => mk(Ty::Int, HKind::Int(0)),
                }
            }
            ExprKind::Call { name, args, .. } => self.call(name, args, span),
            ExprKind::MethodCall { target, method, args } => {
                let obj = self.expr(target);
                let Ty::Obj(t) = obj.ty else {
                    let n = self.tn(&obj.ty);
                    self.err(target.span, format!("method call requires an object, found {n}"));
                    return mk(Ty::Int, HKind::Int(0));
                };
                let Some(&fid) = self.types[t as usize].methods.get(&method.name) else {
                    let tn = self.types[t as usize].name.clone();
                    self.err(method.span, format!("type '{tn}' has no method '{}'", method.name));
                    return mk(Ty::Int, HKind::Int(0));
                };
                let mut hargs = vec![obj];
                hargs.extend(args.iter().map(|a| self.expr(a)));
                self.user_call(fid, hargs, 1, span)
            }
            ExprKind::Unary(op, x) => {
                let h = self.expr(x);
                self.unary(*op, h, span)
            }
            ExprKind::Binary(op, l, r) => {
                let l = self.expr(l);
                let r = self.expr(r);
                self.binary(*op, l, r, span)
            }
            ExprKind::Cast(to, x) => {
                let h = self.expr(x);
                let ty = match to {
                    CastTo::Int => Ty::Int,
                    CastTo::Float => Ty::Float,
                    CastTo::Str => Ty::Str,
                };
                if matches!(h.ty, Ty::Obj(_) | Ty::Null | Ty::Vector(_)) {
                    let n = self.tn(&h.ty);
                    self.err(span, format!("cannot convert {n} to {}", self.tn(&ty)));
                    return self.default_value(&ty, span);
                }
                self.convert(h, &ty, span)
            }
            ExprKind::New(t) | ExprKind::First(t) | ExprKind::Last(t) => {
                let Some(&tid) = self.type_map.get(&t.name) else {
                    self.err(t.span, format!("type '{}' not found", t.name));
                    return mk(Ty::Null, HKind::Null);
                };
                let kind = match &e.kind {
                    ExprKind::New(_) => HKind::New(tid),
                    ExprKind::First(_) => HKind::First(tid),
                    _ => HKind::Last(tid),
                };
                mk(Ty::Obj(tid), kind)
            }
            ExprKind::After(x) | ExprKind::Before(x) => {
                let h = self.expr(x);
                if !matches!(h.ty, Ty::Obj(_)) {
                    self.err(span, "'After'/'Before' require an object");
                    return mk(Ty::Null, HKind::Null);
                }
                let ty = h.ty.clone();
                let kind = if matches!(e.kind, ExprKind::After(_)) { HKind::After(Box::new(h)) } else { HKind::Before(Box::new(h)) };
                mk(ty, kind)
            }
            ExprKind::Handle(x) => {
                let h = self.expr(x);
                if !matches!(h.ty, Ty::Obj(_) | Ty::Null) {
                    self.err(span, "'Handle' requires an object");
                }
                mk(Ty::Int, HKind::Handle(Box::new(h)))
            }
            ExprKind::Object(t, x) => {
                let Some(&tid) = self.type_map.get(&t.name) else {
                    self.err(t.span, format!("type '{}' not found", t.name));
                    return mk(Ty::Null, HKind::Null);
                };
                let h = self.expr(x);
                let h = self.convert(h, &Ty::Int, x.span);
                mk(Ty::Obj(tid), HKind::Object(tid, Box::new(h)))
            }
            ExprKind::Interp(parts) => {
                let mut acc: Option<HExpr> = None;
                for p in parts {
                    let h = match p {
                        ast::InterpPart::Lit(s) => mk(Ty::Str, HKind::Str(s.as_str().into())),
                        ast::InterpPart::Expr(x) => {
                            let h = self.expr(x);
                            if matches!(h.ty, Ty::Obj(_) | Ty::Null | Ty::Vector(_) | Ty::Void) {
                                let n = self.tn(&h.ty);
                                self.err(x.span, format!("cannot interpolate a value of type {n}"));
                                continue;
                            }
                            self.convert(h, &Ty::Str, x.span)
                        }
                    };
                    acc = Some(match acc {
                        None => h,
                        Some(a) => self.binary(BinOp::Add, a, h, span),
                    });
                }
                acc.unwrap_or_else(|| mk(Ty::Str, HKind::Str("".into())))
            }
        }
    }

    fn call(&mut self, name: &ast::Ident, args: &[Expr], span: Span) -> HExpr {
        if name.name == "iif" && !self.func_map.contains_key("iif") {
            return self.iif(args, span);
        }
        let hargs: Vec<HExpr> = args.iter().map(|a| self.expr(a)).collect();
        if let Some(&fid) = self.func_map.get(&name.name) {
            return self.user_call(fid, hargs, 0, span);
        }
        if let Some(&nid) = self.native_map.get(&name.name) {
            let sig = &self.natives[nid as usize];
            if hargs.len() > sig.params.len() || hargs.len() < sig.required() {
                let msg = if hargs.len() > sig.params.len() { "too many" } else { "not enough" };
                let d = sig.display.clone();
                self.err(span, format!("{msg} parameters for '{d}' (expects {}{})", sig.required(), if sig.params.len() > sig.required() { format!(" to {}", sig.params.len()) } else { String::new() }));
                return self.default_value(&sig.ret.map(native_ty).unwrap_or(Ty::Void), span);
            }
            let params: Vec<_> = sig.params.clone();
            let ret = sig.ret.map(native_ty).unwrap_or(Ty::Void);
            let mut out = Vec::with_capacity(params.len());
            let mut it = hargs.into_iter();
            for p in &params {
                let pty = native_ty(p.ty);
                let a = match it.next() {
                    Some(a) => {
                        if matches!(a.ty, Ty::Obj(_) | Ty::Null) && pty != Ty::Variant {
                            let n = self.tn(&a.ty);
                            self.err(a.span, format!("cannot pass {n} to parameter '{}' of '{}'", p.name, self.natives[nid as usize].display));
                        }
                        let sp = a.span;
                        self.convert(a, &pty, sp)
                    }
                    None => {
                        let c = const_expr(p.default.as_ref().unwrap(), span);
                        self.convert(c, &pty, span)
                    }
                };
                out.push(a);
            }
            return HExpr { ty: ret, kind: HKind::Native(nid, out), span };
        }
        if self.array_map.contains_key(&name.name) {
            self.err(name.span, format!("array '{}' must be declared with Dim before this use", name.name));
        } else {
            self.err(name.span, format!("function '{}' not found", name.name));
        }
        HExpr::int(0, span)
    }

    /// Resolves `Enum.Member` (parsed as an identifier with an object tag).
    fn enum_member(&self, name: &str, tag: Option<&TypeAnn>, shadowed: bool) -> Option<Result<i32, String>> {
        let Some(TypeAnn::Obj(member)) = tag else { return None };
        if shadowed {
            return None;
        }
        let members = self.enums.get(name)?;
        Some(members.get(member).copied().ok_or_else(|| format!("enum '{name}' has no member '{member}'")))
    }

    fn iif(&mut self, args: &[Expr], span: Span) -> HExpr {
        if args.len() != 3 {
            self.err(span, "IIf expects 3 arguments: IIf(condition, then_value, else_value)");
            return HExpr::int(0, span);
        }
        let c = self.cond(&args[0]);
        let a = self.expr(&args[1]);
        let b = self.expr(&args[2]);
        let ty = match (&a.ty, &b.ty) {
            (x, y) if x == y => x.clone(),
            (Ty::Variant, _) | (_, Ty::Variant) => Ty::Variant,
            (Ty::Str, _) | (_, Ty::Str) => Ty::Str,
            (Ty::Float, Ty::Int) | (Ty::Int, Ty::Float) => Ty::Float,
            (Ty::Obj(t), Ty::Null) | (Ty::Null, Ty::Obj(t)) => Ty::Obj(*t),
            (x, y) => {
                let (x, y) = (self.tn(x), self.tn(y));
                self.err(span, format!("IIf branches have incompatible types {x} and {y}"));
                return HExpr::int(0, span);
            }
        };
        if let HKind::Int(v) = c.kind {
            let pick = if v != 0 { a } else { b };
            return self.convert(pick, &ty, span);
        }
        let a = self.convert(a, &ty, span);
        let b = self.convert(b, &ty, span);
        HExpr { ty, kind: HKind::IfElse(Box::new(c), Box::new(a), Box::new(b)), span }
    }

    fn user_call(&mut self, fid: FuncId, hargs: Vec<HExpr>, implicit: usize, span: Span) -> HExpr {
        let info = &self.funcs[fid as usize];
        let params = info.params.clone();
        let ret = info.ret.clone();
        let display = info.display.clone();
        let required = params.iter().filter(|p| p.2.is_none()).count();
        if hargs.len() > params.len() || hargs.len() < required {
            self.err(span, format!("'{display}' expects {} argument(s), {} given", required - implicit, hargs.len() - implicit));
            return self.default_value(&ret, span);
        }
        let mut out = Vec::with_capacity(params.len());
        let mut it = hargs.into_iter();
        for (pname, pty, def) in &params {
            let a = match it.next() {
                Some(a) => {
                    let sp = a.span;
                    self.convert(a, pty, sp)
                }
                None => match (pty, def) {
                    (Ty::Obj(_), _) => HExpr { ty: pty.clone(), kind: HKind::Null, span },
                    (_, Some(c)) => {
                        let c = const_expr(c, span);
                        self.convert(c, pty, span)
                    }
                    _ => unreachable!("missing argument for {pname}"),
                },
            };
            out.push(a);
        }
        HExpr { ty: ret, kind: HKind::Call(fid, out), span }
    }

    fn unary(&mut self, op: UnOp, h: HExpr, span: Span) -> HExpr {
        let ty = h.ty.clone();
        if ty == Ty::Void {
            self.err(span, "expression does not produce a value");
            return HExpr::int(0, span);
        }
        let k = num_kind(&ty);
        match op {
            UnOp::Plus => {
                if !ty.is_numeric() && ty != Ty::Variant {
                    self.err(span, "unary '+' requires a number");
                }
                h
            }
            UnOp::Not => {
                if let HKind::Int(v) = h.kind {
                    return HExpr::int((v == 0) as i32, span);
                }
                HExpr { ty: Ty::Int, kind: HKind::Unary(HUnary::Not, k, Box::new(h)), span }
            }
            UnOp::BitNot => {
                let h = self.convert(h, &Ty::Int, span);
                if let HKind::Int(v) = h.kind {
                    return HExpr::int(!v, span);
                }
                HExpr { ty: Ty::Int, kind: HKind::Unary(HUnary::BitNot, NumKind::I, Box::new(h)), span }
            }
            UnOp::Neg | UnOp::Abs | UnOp::Sgn => {
                let (h, k, ty) = match k {
                    NumKind::I | NumKind::F => (h, k, ty),
                    NumKind::V if op == UnOp::Neg => (h, k, ty),
                    NumKind::V => (self.convert(h, &Ty::Float, span), NumKind::F, Ty::Float),
                    _ => {
                        let n = self.tn(&ty);
                        self.err(span, format!("arithmetic on a value of type {n}"));
                        return HExpr::int(0, span);
                    }
                };
                let hu = match op {
                    UnOp::Neg => HUnary::Neg,
                    UnOp::Abs => HUnary::Abs,
                    _ => HUnary::Sgn,
                };
                match (&h.kind, hu) {
                    (HKind::Int(v), HUnary::Neg) => return HExpr::int(v.wrapping_neg(), span),
                    (HKind::Float(v), HUnary::Neg) => return HExpr { ty, kind: HKind::Float(-v), span },
                    _ => {}
                }
                HExpr { ty, kind: HKind::Unary(hu, k, Box::new(h)), span }
            }
        }
    }

    fn binary(&mut self, op: BinOp, l: HExpr, r: HExpr, span: Span) -> HExpr {
        if l.ty == Ty::Void || r.ty == Ty::Void {
            self.err(span, "expression does not produce a value");
            return HExpr::int(0, span);
        }
        use BinOp::*;
        match op {
            AndAlso | OrElse => {
                let l = self.truthy(l);
                let r = self.truthy(r);
                let kind = if op == AndAlso { HKind::AndAlso(Box::new(l), Box::new(r)) } else { HKind::OrElse(Box::new(l), Box::new(r)) };
                HExpr { ty: Ty::Int, kind, span }
            }
            And | Or | Xor | Shl | Shr | Sar => {
                for x in [&l, &r] {
                    if !matches!(x.ty, Ty::Int | Ty::Float | Ty::Variant | Ty::Str) {
                        let n = self.tn(&x.ty);
                        self.err(x.span, format!("bitwise operator on a value of type {n}"));
                        return HExpr::int(0, span);
                    }
                }
                let l = self.convert(l, &Ty::Int, span);
                let r = self.convert(r, &Ty::Int, span);
                if let (HKind::Int(a), HKind::Int(b)) = (&l.kind, &r.kind) {
                    let (a, b) = (*a, *b);
                    let v = match op {
                        And => a & b,
                        Or => a | b,
                        Xor => a ^ b,
                        Shl => a.wrapping_shl(b as u32),
                        Shr => ((a as u32).wrapping_shr(b as u32)) as i32,
                        _ => a.wrapping_shr(b as u32),
                    };
                    return HExpr::int(v, span);
                }
                HExpr { ty: Ty::Int, kind: HKind::Binary(op, NumKind::I, Box::new(l), Box::new(r)), span }
            }
            Eq | Ne | Lt | Gt | Le | Ge => {
                let cmp = match op {
                    Eq => Cmp::Eq,
                    Ne => Cmp::Ne,
                    Lt => Cmp::Lt,
                    Gt => Cmp::Gt,
                    Le => Cmp::Le,
                    _ => Cmp::Ge,
                };
                let (lk, rk) = (num_kind(&l.ty), num_kind(&r.ty));
                let (kind, cty) = if lk == NumKind::V || rk == NumKind::V {
                    (NumKind::V, Ty::Variant)
                } else if lk == NumKind::O || rk == NumKind::O {
                    let ok = matches!((&l.ty, &r.ty), (Ty::Obj(a), Ty::Obj(b)) if a == b)
                        || matches!((&l.ty, &r.ty), (Ty::Obj(_) | Ty::Null, Ty::Null) | (Ty::Null, Ty::Obj(_)));
                    if !ok || !matches!(cmp, Cmp::Eq | Cmp::Ne) {
                        let (a, b) = (self.tn(&l.ty), self.tn(&r.ty));
                        self.err(span, format!("cannot compare {a} with {b}"));
                        return HExpr::int(0, span);
                    }
                    return HExpr { ty: Ty::Int, kind: HKind::Compare(cmp, NumKind::O, Box::new(l), Box::new(r)), span };
                } else if lk == NumKind::S || rk == NumKind::S {
                    (NumKind::S, Ty::Str)
                } else if lk == NumKind::F || rk == NumKind::F {
                    (NumKind::F, Ty::Float)
                } else {
                    (NumKind::I, Ty::Int)
                };
                let l = self.convert(l, &cty, span);
                let r = self.convert(r, &cty, span);
                if let Some(v) = fold_cmp(cmp, &l.kind, &r.kind) {
                    return HExpr::int(v as i32, span);
                }
                HExpr { ty: Ty::Int, kind: HKind::Compare(cmp, kind, Box::new(l), Box::new(r)), span }
            }
            Add | Sub | Mul | Div | Mod | Pow => {
                let (lk, rk) = (num_kind(&l.ty), num_kind(&r.ty));
                if lk == NumKind::O || rk == NumKind::O {
                    let n = self.tn(if lk == NumKind::O { &l.ty } else { &r.ty });
                    self.err(span, format!("arithmetic on a value of type {n}"));
                    return HExpr::int(0, span);
                }
                let (kind, ty) = if lk == NumKind::V || rk == NumKind::V {
                    (NumKind::V, Ty::Variant)
                } else if lk == NumKind::S || rk == NumKind::S {
                    if op != Add {
                        self.err(span, "only '+' can be used with strings");
                        return HExpr::int(0, span);
                    }
                    (NumKind::S, Ty::Str)
                } else if op == Pow || lk == NumKind::F || rk == NumKind::F {
                    (NumKind::F, Ty::Float)
                } else {
                    (NumKind::I, Ty::Int)
                };
                let l = self.convert(l, &ty, span);
                let r = self.convert(r, &ty, span);
                if let Some(k) = fold_arith(op, &l.kind, &r.kind) {
                    return HExpr { ty, kind: k, span };
                }
                HExpr { ty, kind: HKind::Binary(op, kind, Box::new(l), Box::new(r)), span }
            }
        }
    }

    /// Inserts a conversion from `e.ty` to `to`, folding constants.
    fn convert(&mut self, e: HExpr, to: &Ty, span: Span) -> HExpr {
        if e.ty == *to {
            return e;
        }
        let from = e.ty.clone();
        let bad = |s: &mut Self| {
            let (a, b) = (s.tn(&from), s.tn(to));
            s.err(span, format!("cannot convert {a} to {b}"));
            s.default_value(to, span)
        };
        match (&from, to) {
            (Ty::Void, _) => {
                self.err(span, "expression does not produce a value");
                self.default_value(to, span)
            }
            (_, Ty::Void) => e,
            (_, Ty::Variant) => HExpr { ty: Ty::Variant, kind: HKind::Convert(Box::new(e)), span },
            (Ty::Variant, Ty::Int | Ty::Float | Ty::Str) => {
                HExpr { ty: to.clone(), kind: HKind::Convert(Box::new(e)), span }
            }
            (Ty::Int | Ty::Float | Ty::Str, Ty::Int | Ty::Float | Ty::Str) => {
                let folded = match (&e.kind, to) {
                    (HKind::Int(v), Ty::Float) => Some(HKind::Float(*v as f32)),
                    (HKind::Int(v), Ty::Str) => Some(HKind::Str(v.to_string().into())),
                    (HKind::Float(v), Ty::Int) => Some(HKind::Int(float_to_int(*v))),
                    (HKind::Float(v), Ty::Str) => Some(HKind::Str(format_float(*v).into())),
                    (HKind::Str(s), Ty::Int) => Some(HKind::Int(str_to_int(s))),
                    (HKind::Str(s), Ty::Float) => Some(HKind::Float(str_to_float(s))),
                    _ => None,
                };
                let kind = folded.unwrap_or_else(|| HKind::Convert(Box::new(e)));
                HExpr { ty: to.clone(), kind, span }
            }
            (Ty::Null, Ty::Obj(_) | Ty::Vector(_)) => HExpr { ty: to.clone(), kind: HKind::Null, span },
            _ => bad(self),
        }
    }
}

fn d_is_str(d: &Option<HExpr>) -> bool {
    matches!(d, Some(HExpr { ty: Ty::Str, .. }))
}

struct InferState {
    vars: HashMap<String, Ty>,
    fixed: HashSet<String>,
    self_ty: Option<TypeId>,
}

fn infer_binary(op: BinOp, l: Option<Ty>, r: Option<Ty>) -> Option<Ty> {
    use BinOp::*;
    match op {
        Eq | Ne | Lt | Gt | Le | Ge | And | Or | Xor | Shl | Shr | Sar | AndAlso | OrElse => Some(Ty::Int),
        Pow => Some(Ty::Float),
        _ => match (l, r) {
            (Some(Ty::Variant), _) | (_, Some(Ty::Variant)) => Some(Ty::Variant),
            (Some(Ty::Str), _) | (_, Some(Ty::Str)) => Some(Ty::Str),
            (Some(Ty::Float), _) | (_, Some(Ty::Float)) => Some(Ty::Float),
            (Some(Ty::Int), Some(Ty::Int)) => Some(Ty::Int),
            (Some(Ty::Int), None) | (None, Some(Ty::Int)) => Some(Ty::Int),
            _ => None,
        },
    }
}

fn fold_cmp(cmp: Cmp, l: &HKind, r: &HKind) -> Option<bool> {
    fn ord<T: PartialOrd>(c: Cmp, a: T, b: T) -> bool {
        match c {
            Cmp::Eq => a == b,
            Cmp::Ne => a != b,
            Cmp::Lt => a < b,
            Cmp::Gt => a > b,
            Cmp::Le => a <= b,
            Cmp::Ge => a >= b,
        }
    }
    match (l, r) {
        (HKind::Int(a), HKind::Int(b)) => Some(ord(cmp, a, b)),
        (HKind::Float(a), HKind::Float(b)) => Some(ord(cmp, a, b)),
        (HKind::Str(a), HKind::Str(b)) => Some(ord(cmp, a, b)),
        _ => None,
    }
}

fn fold_arith(op: BinOp, l: &HKind, r: &HKind) -> Option<HKind> {
    use BinOp::*;
    match (l, r) {
        (HKind::Int(a), HKind::Int(b)) => {
            let (a, b) = (*a, *b);
            Some(HKind::Int(match op {
                Add => a.wrapping_add(b),
                Sub => a.wrapping_sub(b),
                Mul => a.wrapping_mul(b),
                Div if b != 0 => a.wrapping_div(b),
                Mod if b != 0 => a.wrapping_rem(b),
                _ => return None,
            }))
        }
        (HKind::Float(a), HKind::Float(b)) => Some(HKind::Float(match op {
            Add => a + b,
            Sub => a - b,
            Mul => a * b,
            Div => a / b,
            Mod => a % b,
            Pow => a.powf(*b),
            _ => return None,
        })),
        (HKind::Str(a), HKind::Str(b)) if op == Add => Some(HKind::Str(format!("{a}{b}").into())),
        _ => None,
    }
}

/// Calls `f` for each nested statement block of a statement.
/// Names of the plain variables a body assigns to anywhere.
fn assigned_names(body: &[Stmt]) -> std::collections::HashSet<String> {
    fn walk(body: &[Stmt], out: &mut std::collections::HashSet<String>) {
        fn add(v: &Var, out: &mut std::collections::HashSet<String>) {
            if let Var::Ident { name, .. } = v {
                out.insert(name.name.clone());
            }
        }
        for s in body {
            match &s.kind {
                StmtKind::Assign(v, _) | StmtKind::CompoundAssign(v, _, _) => add(v, out),
                StmtKind::For { var, .. } | StmtKind::ForEach { var, .. } => add(var, out),
                StmtKind::ForIn { var, .. } => {
                    out.insert(var.name.clone());
                }
                StmtKind::Read(vs) => vs.iter().for_each(|v| add(v, out)),
                StmtKind::Try { var: Some(v), .. } => add(v, out),
                _ => {}
            }
            for_each_block(&s.kind, |b| walk(b, out));
        }
    }
    let mut out = std::collections::HashSet::new();
    walk(body, &mut out);
    out
}

fn for_each_block(k: &StmtKind, mut f: impl FnMut(&[Stmt])) {
    match k {
        StmtKind::If { then, els, .. } => {
            f(then);
            if let Some(e) = els {
                f(e);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::Repeat { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::ForEach { body, .. }
        | StmtKind::ForIn { body, .. } => f(body),
        StmtKind::Select { cases, default, .. } => {
            for (_, b) in cases {
                f(b);
            }
            if let Some(d) = default {
                f(d);
            }
        }
        StmtKind::Try { body, handler, .. } => {
            f(body);
            f(handler);
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn rc(s: &str) -> Rc<str> {
    s.into()
}
