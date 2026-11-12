//! Runtime values and the object heap.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use neptune_lang::bytecode::{FieldInit, TypeInfo, ValKind};
use neptune_lang::commands::ConstVal;
use neptune_lang::sema::format_float;

/// Reference to an object in the [`Heap`]. Stale references (to deleted
/// objects) are detected by the generation counter and behave like `Null`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjRef {
    pub slot: u32,
    pub gen: u32,
}

/// Shared, mutable Blitz vector (`Local v[10]`).
pub type VectorRef = Rc<RefCell<Vec<Value>>>;

/// Characters copied into one block. A longer append shares the prefix and
/// grows only the last block, so `s = s + x` does not copy `s`.
const CHUNK: usize = 256;

/// A BASIC string. Copies share the text. Concatenation shares a long left
/// side and copies only its last block.
#[derive(Clone)]
pub struct NbStr(Rc<StrRep>);

struct StrRep {
    bytes: usize,
    chars: usize,
    kind: StrKind,
}

enum StrKind {
    Flat(Rc<str>),
    Branch {
        left: NbStr,
        right: NbStr,
        /// Full text, filled the first time the chain is read.
        cache: RefCell<Option<Rc<str>>>,
    },
}

impl NbStr {
    pub fn flat(&self) -> Rc<str> {
        match &self.0.kind {
            StrKind::Flat(s) => s.clone(),
            StrKind::Branch { cache, .. } => {
                if let Some(s) = cache.borrow().clone() {
                    return s;
                }
                let mut buf = String::with_capacity(self.0.bytes);
                self.write_to(&mut buf);
                let rc: Rc<str> = buf.into();
                *cache.borrow_mut() = Some(rc.clone());
                rc
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.chars == 0
    }

    pub fn char_count(&self) -> usize {
        self.0.chars
    }

    pub fn byte_len(&self) -> usize {
        self.0.bytes
    }

    /// Joins two strings. A short result is one block. A longer one shares the
    /// left-hand prefix and copies only the last block.
    pub fn concat(left: NbStr, right: NbStr) -> NbStr {
        if right.0.chars == 0 {
            return left;
        }
        if left.0.chars == 0 {
            return right;
        }
        // a chain that was already read has its whole text: start from that,
        // so the chain under it can be freed
        let left = left.read_text().unwrap_or(left);
        let bytes = left.0.bytes + right.0.bytes;
        let chars = left.0.chars + right.0.chars;
        if bytes <= CHUNK {
            return NbStr::copied(&left, &right, bytes, chars);
        }
        if let StrKind::Branch { left: prefix, right: leaf, .. } = &left.0.kind {
            if matches!(leaf.0.kind, StrKind::Flat(_))
                && matches!(right.0.kind, StrKind::Flat(_))
                && leaf.0.bytes + right.0.bytes <= CHUNK
            {
                let prefix = prefix.clone();
                let leaf = leaf.clone();
                let new_leaf = NbStr::copied(&leaf, &right, leaf.0.bytes + right.0.bytes, leaf.0.chars + right.0.chars);
                return NbStr::branch(prefix, new_leaf);
            }
        }
        NbStr::branch(left, right)
    }

    /// The full text of a chain that has been read, as one block.
    fn read_text(&self) -> Option<NbStr> {
        let StrKind::Branch { cache, .. } = &self.0.kind else { return None };
        let text = cache.borrow().clone()?;
        Some(NbStr(Rc::new(StrRep { bytes: self.0.bytes, chars: self.0.chars, kind: StrKind::Flat(text) })))
    }

    fn branch(left: NbStr, right: NbStr) -> NbStr {
        NbStr(Rc::new(StrRep {
            bytes: left.0.bytes + right.0.bytes,
            chars: left.0.chars + right.0.chars,
            kind: StrKind::Branch { left, right, cache: RefCell::new(None) },
        }))
    }

    fn copied(left: &NbStr, right: &NbStr, bytes: usize, chars: usize) -> NbStr {
        let mut buf = String::with_capacity(bytes);
        left.write_to(&mut buf);
        right.write_to(&mut buf);
        NbStr::from_counted(buf, chars)
    }

    fn from_counted(s: String, chars: usize) -> NbStr {
        if chars == 0 {
            return empty_str();
        }
        NbStr(Rc::new(StrRep {
            bytes: s.len(),
            chars,
            kind: StrKind::Flat(s.into()),
        }))
    }

    fn write_to(&self, out: &mut String) {
        let mut stack = vec![self.clone()];
        while let Some(n) = stack.pop() {
            let cached = match &n.0.kind {
                StrKind::Flat(s) => {
                    out.push_str(s);
                    None
                }
                StrKind::Branch { cache, left, right, .. } => {
                    if let Some(s) = cache.borrow().clone() {
                        Some(s)
                    } else {
                        stack.push(right.clone());
                        stack.push(left.clone());
                        None
                    }
                }
            };
            if let Some(s) = cached {
                out.push_str(&s);
            }
        }
    }
}

/// A chain of appends is one level deep per block, so freeing it level by
/// level would overflow the stack for a long string. Free it with a list.
impl Drop for StrRep {
    fn drop(&mut self) {
        if !matches!(self.kind, StrKind::Branch { .. }) {
            return;
        }
        let mut pending = Vec::new();
        let take = |kind: &mut StrKind, pending: &mut Vec<NbStr>| {
            if let StrKind::Branch { left, right, .. } = std::mem::replace(kind, StrKind::Flat(Rc::from(""))) {
                pending.push(left);
                pending.push(right);
            }
        };
        take(&mut self.kind, &mut pending);
        // only a part nothing else shares is freed here; a shared part just
        // loses one reference
        while let Some(part) = pending.pop() {
            if let Ok(mut rep) = Rc::try_unwrap(part.0) {
                if matches!(rep.kind, StrKind::Branch { .. }) {
                    take(&mut rep.kind, &mut pending);
                }
            }
        }
    }
}

impl From<&str> for NbStr {
    fn from(s: &str) -> NbStr {
        if s.is_empty() {
            return empty_str();
        }
        let chars = if s.is_ascii() { s.len() } else { s.chars().count() };
        NbStr::from_counted(s.to_string(), chars)
    }
}

impl From<String> for NbStr {
    fn from(s: String) -> NbStr {
        if s.is_empty() {
            return empty_str();
        }
        let chars = if s.is_ascii() { s.len() } else { s.chars().count() };
        NbStr::from_counted(s, chars)
    }
}

impl From<Rc<str>> for NbStr {
    fn from(s: Rc<str>) -> NbStr {
        if s.is_empty() {
            return empty_str();
        }
        let chars = if s.is_ascii() { s.len() } else { s.chars().count() };
        NbStr(Rc::new(StrRep {
            bytes: s.len(),
            chars,
            kind: StrKind::Flat(s),
        }))
    }
}

impl fmt::Display for NbStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.flat())
    }
}

impl fmt::Debug for NbStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.flat(), f)
    }
}

#[derive(Clone, Debug, Default)]
pub enum Value {
    Int(i32),
    Float(f32),
    Str(NbStr),
    Obj(ObjRef),
    #[default]
    Null,
    Vector(VectorRef),
}

impl Value {
    pub fn default_for(kind: ValKind) -> Value {
        match kind {
            ValKind::Int | ValKind::Variant => Value::Int(0),
            ValKind::Float => Value::Float(0.0),
            ValKind::Str => Value::Str(empty_str()), // shared empty string
            ValKind::Obj | ValKind::Vector => Value::Null,
        }
    }

    pub fn from_const(c: &ConstVal) -> Value {
        match c {
            ConstVal::Int(v) => Value::Int(*v),
            ConstVal::Float(v) => Value::Float(*v),
            ConstVal::Str(s) => Value::Str(NbStr::from(s.clone())),
        }
    }

    /// Integer view (natives receive values already converted by the compiler).
    #[inline]
    pub fn int(&self) -> i32 {
        match self {
            Value::Int(v) => *v,
            Value::Float(f) => neptune_lang::sema::float_to_int(*f),
            Value::Str(s) => neptune_lang::sema::str_to_int(&s.flat()),
            _ => 0,
        }
    }

    #[inline]
    pub fn float(&self) -> f32 {
        match self {
            Value::Int(v) => *v as f32,
            Value::Float(f) => *f,
            Value::Str(s) => neptune_lang::sema::str_to_float(&s.flat()),
            _ => 0.0,
        }
    }

    pub fn str(&self) -> Rc<str> {
        match self {
            Value::Str(s) => s.flat(),
            other => other.to_string().into(),
        }
    }

    /// The string as stored, without copying a chain into one block.
    pub fn text(&self) -> NbStr {
        match self {
            Value::Str(s) => s.clone(),
            other => NbStr::from(other.to_string()),
        }
    }

    /// Character length. For a string this does not walk the text.
    pub fn char_len(&self) -> usize {
        match self {
            Value::Str(s) => s.char_count(),
            other => other.str().chars().count(),
        }
    }

    /// Byte offset of character `index` (or the end of the text). ASCII
    /// strings take the same number of bytes as characters, so the offset
    /// needs no walk.
    pub fn char_at(&self, index: usize) -> (Rc<str>, usize) {
        match self {
            Value::Str(s) if s.byte_len() == s.char_count() => {
                let t = s.flat();
                let i = index.min(t.len());
                (t, i)
            }
            _ => {
                let t = self.str();
                let i = t.char_indices().nth(index).map(|(i, _)| i).unwrap_or(t.len());
                (t, i)
            }
        }
    }

    /// Name of the dynamic type (for `TypeOf`).
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Str(_) => "String",
            Value::Obj(_) => "Object",
            Value::Null => "Null",
            Value::Vector(_) => "Array",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => f.write_str(&format_float(*v)),
            Value::Str(s) => f.write_str(&s.flat()),
            Value::Obj(_) => f.write_str("[Object]"),
            Value::Null => f.write_str("Null"),
            Value::Vector(v) => write!(f, "[Array({})]", v.borrow().len()),
        }
    }
}

thread_local! {
    static EMPTY: NbStr = NbStr(Rc::new(StrRep {
        bytes: 0,
        chars: 0,
        kind: StrKind::Flat(Rc::from("")),
    }));
}

/// Shared empty string (avoids allocating for default values).
pub fn empty_str() -> NbStr {
    EMPTY.with(|e| e.clone())
}

const NONE: u32 = u32::MAX;

struct Slot {
    gen: u32,
    ty: u32,
    alive: bool,
    prev: u32,
    next: u32,
    handle: i32,
    fields: Vec<Value>,
}

#[derive(Clone, Copy)]
struct TypeList {
    head: u32,
    tail: u32,
}

/// Object storage implementing Blitz `Type` semantics: every object belongs to
/// its type's ordered list; `Delete` unlinks it and invalidates references.
pub struct Heap {
    slots: Vec<Slot>,
    free: Vec<u32>,
    lists: Vec<TypeList>,
    handles: HashMap<i32, ObjRef>,
    next_handle: i32,
    live: usize,
}

pub const NO_OBJECT: &str = "Object does not exist";

impl Heap {
    pub fn new(n_types: usize) -> Heap {
        Heap {
            slots: Vec::new(),
            free: Vec::new(),
            lists: vec![TypeList { head: NONE, tail: NONE }; n_types],
            handles: HashMap::new(),
            next_handle: 1,
            live: 0,
        }
    }

    /// Number of live objects.
    pub fn live(&self) -> usize {
        self.live
    }

    #[inline]
    pub fn is_alive(&self, r: ObjRef) -> bool {
        self.slots.get(r.slot as usize).is_some_and(|s| s.alive && s.gen == r.gen)
    }

    /// Normalizes a value to a live reference (`None` for Null or deleted objects).
    #[inline]
    pub fn live_ref(&self, v: &Value) -> Option<ObjRef> {
        match v {
            Value::Obj(r) if self.is_alive(*r) => Some(*r),
            _ => None,
        }
    }

    pub fn alloc(&mut self, ty: u32, info: &TypeInfo) -> ObjRef {
        let fields = info
            .fields
            .iter()
            .map(|f| match f {
                FieldInit::Value(c) => Value::from_const(c),
                FieldInit::Null => Value::Null,
                FieldInit::Variant => Value::Int(0),
                FieldInit::Vector { len, elem } => {
                    Value::Vector(Rc::new(RefCell::new(vec![Value::default_for(*elem); *len as usize])))
                }
            })
            .collect();
        let slot = match self.free.pop() {
            Some(i) => {
                let s = &mut self.slots[i as usize];
                s.ty = ty;
                s.alive = true;
                s.fields = fields;
                s.handle = 0;
                i
            }
            None => {
                self.slots.push(Slot { gen: 0, ty, alive: true, prev: NONE, next: NONE, handle: 0, fields });
                (self.slots.len() - 1) as u32
            }
        };
        let gen = self.slots[slot as usize].gen;
        self.link_after(ty, slot, self.lists[ty as usize].tail);
        self.live += 1;
        ObjRef { slot, gen }
    }

    fn unlink(&mut self, slot: u32) {
        let (ty, prev, next) = {
            let s = &self.slots[slot as usize];
            (s.ty, s.prev, s.next)
        };
        let list = &mut self.lists[ty as usize];
        if prev == NONE {
            list.head = next;
        } else {
            self.slots[prev as usize].next = next;
        }
        if next == NONE {
            self.lists[ty as usize].tail = prev;
        } else {
            self.slots[next as usize].prev = prev;
        }
    }

    /// Links `slot` after `after` (`NONE` = at head).
    fn link_after(&mut self, ty: u32, slot: u32, after: u32) {
        let next = if after == NONE { self.lists[ty as usize].head } else { self.slots[after as usize].next };
        {
            let s = &mut self.slots[slot as usize];
            s.prev = after;
            s.next = next;
        }
        if after == NONE {
            self.lists[ty as usize].head = slot;
        } else {
            self.slots[after as usize].next = slot;
        }
        if next == NONE {
            self.lists[ty as usize].tail = slot;
        } else {
            self.slots[next as usize].prev = slot;
        }
    }

    pub fn delete(&mut self, r: ObjRef) {
        if !self.is_alive(r) {
            return;
        }
        self.unlink(r.slot);
        let s = &mut self.slots[r.slot as usize];
        s.alive = false;
        s.gen = s.gen.wrapping_add(1);
        s.fields = Vec::new();
        if s.handle != 0 {
            self.handles.remove(&s.handle);
        }
        self.free.push(r.slot);
        self.live -= 1;
    }

    pub fn delete_each(&mut self, ty: u32) {
        while let Some(r) = self.first(ty) {
            self.delete(r);
        }
    }

    fn mkref(&self, slot: u32) -> Option<ObjRef> {
        (slot != NONE).then(|| ObjRef { slot, gen: self.slots[slot as usize].gen })
    }

    pub fn first(&self, ty: u32) -> Option<ObjRef> {
        self.mkref(self.lists[ty as usize].head)
    }
    pub fn last(&self, ty: u32) -> Option<ObjRef> {
        self.mkref(self.lists[ty as usize].tail)
    }
    pub fn after(&self, r: ObjRef) -> Option<ObjRef> {
        self.mkref(self.slots[r.slot as usize].next)
    }
    pub fn before(&self, r: ObjRef) -> Option<ObjRef> {
        self.mkref(self.slots[r.slot as usize].prev)
    }

    /// Moves `obj` directly before/after `target` in their type list.
    pub fn insert(&mut self, obj: ObjRef, target: ObjRef, before: bool) -> Result<(), String> {
        let ty = self.slots[obj.slot as usize].ty;
        if self.slots[target.slot as usize].ty != ty {
            return Err("Insert: objects are of different types".into());
        }
        if obj == target {
            return Ok(());
        }
        self.unlink(obj.slot);
        let after = if before { self.slots[target.slot as usize].prev } else { target.slot };
        self.link_after(ty, obj.slot, after);
        Ok(())
    }

    #[inline]
    pub fn field(&self, r: ObjRef, i: u32) -> &Value {
        &self.slots[r.slot as usize].fields[i as usize]
    }
    #[inline]
    pub fn field_mut(&mut self, r: ObjRef, i: u32) -> &mut Value {
        &mut self.slots[r.slot as usize].fields[i as usize]
    }

    pub fn type_of(&self, r: ObjRef) -> u32 {
        self.slots[r.slot as usize].ty
    }

    pub fn handle(&mut self, r: ObjRef) -> i32 {
        let s = &mut self.slots[r.slot as usize];
        if s.handle == 0 {
            s.handle = self.next_handle;
            self.next_handle = self.next_handle.wrapping_add(1).max(1);
            self.handles.insert(s.handle, r);
        }
        s.handle
    }

    pub fn from_handle(&self, h: i32, ty: u32) -> Option<ObjRef> {
        let r = *self.handles.get(&h)?;
        (self.is_alive(r) && self.type_of(r) == ty).then_some(r)
    }

    /// Iterates live objects of a type in list order.
    pub fn iter_type(&self, ty: u32) -> impl Iterator<Item = ObjRef> + '_ {
        let mut cur = self.first(ty);
        std::iter::from_fn(move || {
            let r = cur?;
            cur = self.after(r);
            Some(r)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> TypeInfo {
        TypeInfo { name: "t".into(), field_names: vec!["x".into()], fields: vec![FieldInit::Value(ConstVal::Int(7))] }
    }

    #[test]
    fn list_order_delete_and_stale_refs() {
        let mut h = Heap::new(1);
        let a = h.alloc(0, &info());
        let b = h.alloc(0, &info());
        let c = h.alloc(0, &info());
        assert_eq!(h.iter_type(0).collect::<Vec<_>>(), vec![a, b, c]);
        h.delete(b);
        assert!(!h.is_alive(b));
        assert_eq!(h.iter_type(0).collect::<Vec<_>>(), vec![a, c]);
        // slot reuse must not resurrect the stale reference
        let d = h.alloc(0, &info());
        assert_eq!(d.slot, b.slot);
        assert!(!h.is_alive(b) && h.is_alive(d));
        h.insert(d, a, true).unwrap();
        assert_eq!(h.iter_type(0).collect::<Vec<_>>(), vec![d, a, c]);
        assert_eq!(h.field(a, 0).int(), 7);
        let hd = h.handle(c);
        assert_eq!(h.from_handle(hd, 0), Some(c));
        h.delete_each(0);
        assert_eq!(h.live(), 0);
        assert_eq!(h.from_handle(hd, 0), None);
    }

    #[test]
    fn a_long_chain_of_appends_is_freed_without_overflowing_the_stack() {
        // a small stack, so freeing the chain one level per call would overflow
        let done = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let piece = NbStr::from("x".repeat(CHUNK + 1));
                let mut s = NbStr::from("");
                for _ in 0..200_000 {
                    s = NbStr::concat(s, piece.clone());
                }
                assert_eq!(s.char_count(), 200_000 * (CHUNK + 1));
                drop(s);
            })
            .unwrap()
            .join();
        assert!(done.is_ok());
    }

    #[test]
    fn reading_a_string_lets_the_next_append_start_from_the_text() {
        let mut s = NbStr::from("");
        for _ in 0..300 {
            s = NbStr::concat(s, NbStr::from("ab"));
        }
        let _ = s.flat();
        let t = NbStr::concat(s, NbStr::from("!"));
        let StrKind::Branch { left, .. } = &t.0.kind else { panic!("expected a chain") };
        assert!(matches!(left.0.kind, StrKind::Flat(_)));
        assert_eq!(t.char_count(), 601);
        assert!(t.flat().ends_with("ab!"));
    }

    #[test]
    fn string_append_shares_the_left_side() {
        let mut s = NbStr::from("");
        for i in 0..500 {
            let ch = char::from(b'a' + (i % 26) as u8).to_string();
            s = NbStr::concat(s, NbStr::from(ch));
        }
        assert_eq!(s.char_count(), 500);
        assert_eq!(&s.flat()[..3], "abc");
        assert_eq!(&s.flat()[s.flat().len() - 3..], "def");
        let alias = s.clone();
        let grown = NbStr::concat(s, NbStr::from("!"));
        assert_eq!(alias.char_count(), 500);
        assert!(alias.flat().ends_with('f'));
        assert!(grown.flat().ends_with('!'));
        let mut accents = NbStr::from("");
        for _ in 0..40 {
            accents = NbStr::concat(accents, NbStr::from("é"));
        }
        assert_eq!(accents.char_count(), 40);
        assert_eq!(accents.flat().chars().count(), 40);
    }
}
