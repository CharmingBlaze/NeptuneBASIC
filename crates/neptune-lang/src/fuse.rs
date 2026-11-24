//! Superinstructions: the last pass of code generation.
//!
//! Replaces short runs of instructions that programs execute all the time
//! (`LoadLocal, PushInt, CmpI, JumpIfZero`, the test of an `If` or `While`)
//! with one instruction that does the same work without going through the
//! value stack. The interpreter spends most of its time fetching
//! instructions and pushing and popping, so fewer, larger instructions run
//! faster.
//!
//! A run is only fused when nothing can enter it in the middle: no jump
//! lands inside it and no statement starts inside it (the debugger stops at
//! statements). Jump targets, the line table and the statement table are
//! renumbered to match.

use crate::bytecode::{Cmp, Op};
use crate::diag::Span;

/// Longest run looked at: four instructions, plus one of lookahead.
const WINDOW: usize = 5;

/// Fuses the instructions of one function in place.
pub fn fuse(code: &mut Vec<Op>, lines: &mut Vec<(u32, Span)>, stmts: &mut Vec<u32>) {
    let n = code.len();
    // instructions that must stay the first of their run
    let mut entry = vec![false; n + 1];
    for op in code.iter() {
        if let Some(t) = op.target() {
            entry[t as usize] = true;
        }
    }
    for &s in stmts.iter() {
        entry[s as usize] = true;
    }

    let mut out = Vec::with_capacity(n);
    // old instruction index -> new index of the run it is now part of
    let mut map = vec![0u32; n + 1];
    let mut i = 0;
    while i < n {
        let mut avail = 1;
        while avail < WINDOW && i + avail < n && !entry[i + avail] {
            avail += 1;
        }
        let (op, len) = fuse_at(&code[i..i + avail]).unwrap_or((code[i], 1));
        for m in &mut map[i..i + len] {
            *m = out.len() as u32;
        }
        out.push(op);
        i += len;
    }
    map[n] = out.len() as u32;

    for op in &mut out {
        if let Some(t) = op.target_mut() {
            *t = map[*t as usize];
        }
    }
    // a fused run reports errors at its last instruction, the one that can
    // fail (the array access after its index, the field after its object)
    let mut new_lines: Vec<(u32, Span)> = Vec::with_capacity(lines.len());
    for &(pc, span) in lines.iter() {
        let pc = map[pc as usize];
        match new_lines.last_mut() {
            Some(last) if last.0 == pc => last.1 = span,
            _ => new_lines.push((pc, span)),
        }
    }
    *lines = new_lines;
    for s in stmts.iter_mut() {
        *s = map[*s as usize];
    }
    stmts.dedup();
    *code = out;
}

/// A conditional jump as (jumps when the condition is true, target).
fn cond(op: Op) -> Option<(bool, u32)> {
    match op {
        Op::JumpIfNotZero(t) => Some((true, t)),
        Op::JumpIfZero(t) => Some((false, t)),
        _ => None,
    }
}

fn sense(c: Cmp, when_true: bool) -> Cmp {
    if when_true {
        c
    } else {
        c.negate()
    }
}

/// The instruction replacing the start of `w`, and how many it replaces.
fn fuse_at(w: &[Op]) -> Option<(Op, usize)> {
    use Op::*;
    let small = |s: u32| u16::try_from(s).ok();

    if let [LoadLocal(a), LoadLocal(b), CmpI(c), j, ..] = *w {
        if let (Some(a), Some(b), Some((when, target))) = (small(a), small(b), cond(j)) {
            return Some((JumpCmpILL { c: sense(c, when), a, b, target }, 4));
        }
    }
    if let [LoadLocal(a), PushInt(k), CmpI(c), j, ..] = *w {
        if let (Some(slot), Some((when, target))) = (small(a), cond(j)) {
            return Some((JumpCmpILK { c: sense(c, when), slot, k, target }, 4));
        }
    }
    if let [LoadLocal(b), CmpI(c), j, ..] = *w {
        if let (Some(slot), Some((when, target))) = (small(b), cond(j)) {
            return Some((JumpCmpIL { c: sense(c, when), slot, target }, 3));
        }
    }
    if let [PushInt(k), CmpI(c), j, ..] = *w {
        if let Some((when, target)) = cond(j) {
            return Some((JumpCmpIK { c: sense(c, when), k, target }, 3));
        }
    }
    let [x, y, ..] = *w else { return None };
    let op = match (x, y) {
        (CmpI(c), j) => {
            let (when, t) = cond(j)?;
            JumpCmpI(sense(c, when), t)
        }
        // not negated: with NaN, `a < b` and `a >= b` are both false
        (CmpF(c), JumpIfNotZero(t)) => JumpCmpF(c, t),
        (CmpF(c), JumpIfZero(t)) => JumpNotCmpF(c, t),
        (LoadLocal(s), AddI) => AddIL(s),
        (LoadLocal(s), SubI) => SubIL(s),
        (LoadLocal(s), MulI) => MulIL(s),
        (LoadLocal(s), AddF) => AddFL(s),
        (LoadLocal(s), SubF) => SubFL(s),
        (LoadLocal(s), MulF) => MulFL(s),
        (LoadLocal(s), DivF) => DivFL(s),
        (PushInt(k), AddI) => AddIK(k),
        (PushInt(k), SubI) => AddIK(k.wrapping_neg()),
        (PushInt(k), MulI) => MulIK(k),
        (PushInt(k), DivI) if k != 0 => DivIK(k),
        (PushInt(k), ModI) if k != 0 => ModIK(k),
        (PushFloat(k), AddF) => AddFK(k),
        (PushFloat(k), SubF) => AddFK(-k),
        (PushFloat(k), MulF) => MulFK(k),
        (PushFloat(k), DivF) => DivFK(k),
        (LoadLocal(s), LoadField(f)) => LoadLocalField(s, f),
        (LoadLocal(slot), LoadArray { array, dims: 1 }) => LoadArrayL { array, slot },
        (IncLocalI(s, d), Jump(target)) => IncLocalJump { slot: small(s)?, d: i16::try_from(d).ok()?, target },
        // two loads in one step, unless the second fuses with what follows it
        (LoadLocal(a), LoadLocal(b)) if fuse_at(&w[1..]).is_none() => LoadLocal2(a, b),
        _ => return None,
    };
    Some((op, 2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Op::*;

    fn span(n: u32) -> Span {
        Span { file: 0, start: n, end: n + 1 }
    }

    fn run(code: &[Op], stmts: &[u32]) -> (Vec<Op>, Vec<u32>) {
        let (mut code, mut stmts, mut lines) = (code.to_vec(), stmts.to_vec(), Vec::new());
        fuse(&mut code, &mut lines, &mut stmts);
        (code, stmts)
    }

    #[test]
    fn for_loop_becomes_two_instructions_around_its_body() {
        // For i = 1 To n : sum = sum + i : Next   (i = 0, n = 1, sum = 2)
        let code = [
            LoadLocal(0),
            LoadLocal(1),
            CmpI(Cmp::Gt),
            JumpIfNotZero(10),
            LoadLocal(2),
            LoadLocal(0),
            AddI,
            StoreLocal(2),
            IncLocalI(0, 1),
            Jump(0),
            End,
        ];
        let (out, stmts) = run(&code, &[0, 4]);
        assert_eq!(
            out,
            [
                JumpCmpILL { c: Cmp::Gt, a: 0, b: 1, target: 5 },
                LoadLocal(2),
                AddIL(0),
                StoreLocal(2),
                IncLocalJump { slot: 0, d: 1, target: 0 },
                End,
            ]
        );
        assert_eq!(stmts, [0, 1]);
    }

    #[test]
    fn nothing_is_fused_across_a_jump_target_or_a_statement() {
        // a jump lands on the CmpI
        let code = [LoadLocal(0), PushInt(3), CmpI(Cmp::Lt), JumpIfZero(2), End];
        assert_eq!(run(&code, &[0]).0, [LoadLocal(0), PushInt(3), JumpCmpI(Cmp::Ge, 2), End]);
        // a statement starts at the AddI
        let code = [LoadLocal(0), PushInt(3), AddI, End];
        assert_eq!(run(&code, &[0, 2]).0, code);
        assert_eq!(run(&code, &[0]).0, [LoadLocal(0), AddIK(3), End]);
    }

    #[test]
    fn jumps_when_false_negate_integers_but_not_floats() {
        let code = [LoadLocal(0), PushInt(3), CmpI(Cmp::Lt), JumpIfZero(4), End];
        assert_eq!(run(&code, &[0]).0, [JumpCmpILK { c: Cmp::Ge, slot: 0, k: 3, target: 1 }, End]);
        let code = [CmpF(Cmp::Lt), JumpIfZero(2), End];
        assert_eq!(run(&code, &[0]).0, [JumpNotCmpF(Cmp::Lt, 1), End]);
    }

    #[test]
    fn constants_keep_their_meaning() {
        assert_eq!(run(&[PushInt(5), SubI, End], &[0]).0, [AddIK(-5), End]);
        assert_eq!(run(&[PushInt(i32::MIN), SubI, End], &[0]).0, [AddIK(i32::MIN), End]);
        // division by a constant 0 must still fail at run time
        assert_eq!(run(&[PushInt(0), DivI, End], &[0]).0, [PushInt(0), DivI, End]);
        assert_eq!(run(&[PushInt(0), ModI, End], &[0]).0, [PushInt(0), ModI, End]);
        assert_eq!(run(&[PushFloat(2.0), SubF, End], &[0]).0, [AddFK(-2.0), End]);
    }

    #[test]
    fn two_loads_pair_up_only_when_the_second_has_nothing_better() {
        assert_eq!(run(&[LoadLocal(0), LoadLocal(1), MulI, End], &[0]).0, [LoadLocal(0), MulIL(1), End]);
        assert_eq!(run(&[LoadLocal(0), LoadLocal(1), Call(0), End], &[0]).0, [LoadLocal2(0, 1), Call(0), End]);
        // the second load starts a four-instruction test
        let code = [LoadLocal(0), LoadLocal(1), PushInt(2), CmpI(Cmp::Eq), JumpIfZero(5), End];
        assert_eq!(run(&code, &[0]).0, [LoadLocal(0), JumpCmpILK { c: Cmp::Ne, slot: 1, k: 2, target: 2 }, End]);
    }

    #[test]
    fn big_slots_and_steps_are_left_alone() {
        let code = [LoadLocal(70000), LoadLocal(1), CmpI(Cmp::Lt), JumpIfZero(4), End];
        assert_eq!(run(&code, &[0]).0, [LoadLocal(70000), JumpCmpIL { c: Cmp::Ge, slot: 1, target: 2 }, End]);
        let code = [IncLocalI(0, 100000), Jump(0), End];
        assert_eq!(run(&code, &[0]).0, code);
    }

    #[test]
    fn a_fused_run_reports_the_line_of_its_last_instruction() {
        let mut code = vec![LoadLocal(0), LoadArray { array: 0, dims: 1 }, Pop, End];
        let mut lines = vec![(0, span(10)), (1, span(20)), (2, span(30))];
        let mut stmts = vec![0];
        fuse(&mut code, &mut lines, &mut stmts);
        assert_eq!(code, [LoadArrayL { array: 0, slot: 0 }, Pop, End]);
        assert_eq!(lines, [(0, span(20)), (1, span(30))]);
    }

    #[test]
    fn try_handlers_and_gosubs_are_renumbered() {
        let code = [TryBegin { handler: 5 }, LoadLocal(0), PushInt(1), AddI, TryEnd, Gosub(7), End, GosubReturn];
        let (out, _) = run(&code, &[0, 1]);
        assert_eq!(out, [TryBegin { handler: 4 }, LoadLocal(0), AddIK(1), TryEnd, Gosub(6), End, GosubReturn]);
    }
}
