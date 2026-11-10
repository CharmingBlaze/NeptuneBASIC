//! The VM's debugger support: breakpoints, stepping and looking at variables.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use neptune_vm::debug::{BreakpointSpec, StepKind, StopReason};
use neptune_vm::{base_env, NoHost, Output, Status, Vm};

fn debug_vm(src: &str) -> (Vm, Arc<AtomicBool>) {
    let c = neptune_lang::compile("main.nb", src, &base_env(), &mut neptune_lang::NoLoader);
    let prog = c.program.unwrap_or_else(|| panic!("{}", c.diagnostics.items.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("\n")));
    let mut vm = Vm::new(prog);
    vm.core.output = Output::Buffer(String::new());
    let attention = Arc::new(AtomicBool::new(false));
    vm.debug_enable(attention.clone());
    (vm, attention)
}

/// Runs until the program stops; `None` once it has finished.
fn run(vm: &mut Vm) -> Option<StopReason> {
    loop {
        match vm.run(&mut NoHost).expect("no error expected") {
            Status::Finished => return None,
            Status::Yielded => {}
            Status::Paused => {
                if let Some(r) = vm.debug_stop() {
                    return Some(r.clone());
                }
            }
        }
    }
}

fn line(vm: &Vm) -> u32 {
    vm.debug_frames()[0].line
}

fn global(vm: &Vm, name: &str) -> String {
    vm.debug_eval(0, name).map(|v| vm.debug_describe(&v)).unwrap_or_else(|e| e)
}

const CALLS: &str = "Global total = 0
Function Add(n)
    total = total + n
    Return total
End Function

a = Add(1)
b = Add(2)
Print b
";

const LOOP: &str = "Type Thing
    Field x
End Type
Global t.Thing = New Thing
t\\x = 7
Global name$ = \"bob\"
Dim grid(5)
For i = 1 To 10
    grid(i Mod 5) = i
Next
Print \"done\"
";

fn when(line: u32, condition: &str) -> BreakpointSpec {
    BreakpointSpec { line, condition: Some(condition.into()), hit_condition: None }
}

fn hits(line: u32, rule: &str) -> BreakpointSpec {
    BreakpointSpec { line, condition: None, hit_condition: Some(rule.into()) }
}

/// The values of `i` at which a breakpoint on the loop body stops.
fn stops(spec: BreakpointSpec) -> Vec<String> {
    let (mut vm, _) = debug_vm(LOOP);
    assert_eq!(vm.debug_set_breakpoint_specs(0, &[spec]), vec![Ok(9)]);
    let mut seen = Vec::new();
    while run(&mut vm).is_some() {
        seen.push(global(&vm, "i"));
        vm.debug_continue();
    }
    assert_eq!(vm.core.take_output(), "done\n");
    seen
}

#[test]
fn a_condition_decides_whether_a_breakpoint_stops() {
    assert_eq!(stops(when(9, "i = 4")), ["4"]);
    assert_eq!(stops(when(9, "i > 8")), ["9", "10"]);
    assert_eq!(stops(when(9, "i Mod 5 = 0 And t\\x = 7")), ["5", "10"]);
    assert_eq!(stops(when(9, "(i + 1) * 2 = 8 Or i >= 10")), ["3", "10"]);
    assert_eq!(stops(when(9, "Not i <> 2")), ["2"]);
    assert_eq!(stops(when(9, "name = \"bob\" And i < 2")), ["1"]);
    assert_eq!(stops(when(9, "i * 0.5 = 1.5")), ["3"]);
    assert_eq!(stops(when(9, "$a = i")), ["10"]);
    assert_eq!(stops(when(9, "t <> Null And i = 1")), ["1"]);
    // the array is only read when the index is good, and indices can be sums
    assert_eq!(stops(when(9, "i > 1 And i < 7 AndAlso grid(i - 1) = i - 1 AndAlso i = 5")), ["5"]);
    assert!(stops(when(9, "False")).is_empty());
}

#[test]
fn hit_counts_pick_which_hits_stop() {
    assert_eq!(stops(hits(9, "3")), ["3"]);
    assert_eq!(stops(hits(9, ">=9")), ["9", "10"]);
    assert_eq!(stops(hits(9, ">9")), ["10"]);
    assert_eq!(stops(hits(9, "<2")), ["1"]);
    assert_eq!(stops(hits(9, "%4")), ["4", "8"]);
}

#[test]
fn hits_only_count_when_the_condition_holds() {
    let spec = BreakpointSpec { line: 9, condition: Some("i > 5".into()), hit_condition: Some("2".into()) };
    assert_eq!(stops(spec), ["7"]);
}

#[test]
fn a_bad_hit_count_is_refused() {
    let (mut vm, _) = debug_vm(LOOP);
    let placed = vm.debug_set_breakpoint_specs(0, &[hits(9, "often"), hits(9, "%0")]);
    assert!(placed.iter().all(Result::is_err));
    assert_eq!(run(&mut vm), None);
}

#[test]
fn a_condition_that_fails_stops_and_says_why() {
    let (mut vm, _) = debug_vm(LOOP);
    vm.debug_set_breakpoint_specs(0, &[when(9, "nothing > 1")]);
    assert_eq!(run(&mut vm), Some(StopReason::Condition("'nothing' is not a variable here".into())));
    assert_eq!(line(&vm), 9);
    // going on does not stop on the same statement again
    vm.debug_set_breakpoints(0, &[]);
    vm.debug_continue();
    assert_eq!(run(&mut vm), None);
}

#[test]
fn andalso_does_not_read_its_unused_side() {
    let (vm, _) = debug_vm("Global a = 0\n");
    assert_eq!(vm.debug_condition(0, "False AndAlso nothing\\x = 1"), Ok(false));
    assert_eq!(vm.debug_condition(0, "True OrElse 1 / 0"), Ok(true));
    assert!(vm.debug_condition(0, "1 / 0").is_err());
    assert!(vm.debug_condition(0, "1 2").is_err());
}

#[test]
fn a_step_still_stops_on_a_line_whose_condition_is_false() {
    let (mut vm, _) = debug_vm("For i = 1 To 3\n    a = i\n    b = i\nNext\n");
    vm.debug_set_breakpoint_specs(0, &[when(2, "i = 2"), when(3, "False")]);
    assert_eq!(run(&mut vm), Some(StopReason::Breakpoint));
    assert_eq!(line(&vm), 2);
    vm.debug_step(StepKind::Over);
    assert_eq!(run(&mut vm), Some(StopReason::Step));
    assert_eq!(line(&vm), 3);
}

#[test]
fn breakpoints_stop_and_move_to_the_next_statement() {
    let (mut vm, _) = debug_vm(CALLS);
    // line 5 is End Function and 6 is blank: the first statement after is line 7
    assert_eq!(vm.debug_set_breakpoints(0, &[3, 6, 100]), vec![Some(3), Some(7), None]);
    assert_eq!(run(&mut vm), Some(StopReason::Breakpoint));
    assert_eq!(line(&vm), 7);
    vm.debug_continue();
    assert_eq!(run(&mut vm), Some(StopReason::Breakpoint));
    assert_eq!(line(&vm), 3);
    assert_eq!(global(&vm, "n"), "1");
    assert_eq!(global(&vm, "total"), "0");

    let frames = vm.debug_frames();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].function.to_lowercase(), "add");
    assert_eq!(frames[1].line, 7);
    assert_eq!(vm.debug_eval(1, "n").unwrap_err(), "'n' is not a variable here");

    vm.debug_set_breakpoints(0, &[]);
    vm.debug_continue();
    assert_eq!(run(&mut vm), None);
    assert_eq!(vm.core.take_output(), "3\n");
}

#[test]
fn stepping_in_over_and_out() {
    let (mut vm, _) = debug_vm(CALLS);
    vm.debug_set_breakpoints(0, &[7]);
    run(&mut vm);
    vm.debug_step(StepKind::In);
    assert_eq!(run(&mut vm), Some(StopReason::Step));
    assert_eq!(line(&vm), 3);
    assert_eq!(vm.debug_frames().len(), 2);
    vm.debug_step(StepKind::Over);
    run(&mut vm);
    assert_eq!(line(&vm), 4);
    vm.debug_step(StepKind::Out);
    run(&mut vm);
    assert_eq!(line(&vm), 8);
    assert_eq!(vm.debug_frames().len(), 1);
    vm.debug_step(StepKind::Over);
    run(&mut vm);
    assert_eq!(line(&vm), 9, "stepping over a call stays in this function");
    vm.debug_step(StepKind::Over);
    assert_eq!(run(&mut vm), None);
}

#[test]
fn a_breakpoint_inside_a_stepped_over_call_still_stops() {
    let (mut vm, _) = debug_vm(CALLS);
    vm.debug_set_breakpoints(0, &[3, 7]);
    run(&mut vm);
    vm.debug_step(StepKind::Over);
    assert_eq!(run(&mut vm), Some(StopReason::Breakpoint));
    assert_eq!(line(&vm), 3);
}

#[test]
fn a_loop_stops_at_its_breakpoint_every_turn() {
    let (mut vm, _) = debug_vm("t = 0\nFor i = 1 To 3\n    t = t + i\nNext\nPrint t\n");
    vm.debug_set_breakpoints(0, &[3]);
    for want in ["1", "2", "3"] {
        assert_eq!(run(&mut vm), Some(StopReason::Breakpoint));
        assert_eq!(global(&vm, "i"), want);
        vm.debug_continue();
    }
    assert_eq!(run(&mut vm), None);
    assert_eq!(vm.core.take_output(), "6\n");
}

#[test]
fn several_statements_on_a_line_stop_once() {
    let (mut vm, _) = debug_vm("a = 1 : b = 2 : c = 3\nd = 4\n");
    vm.debug_set_breakpoints(0, &[1]);
    assert_eq!(run(&mut vm), Some(StopReason::Breakpoint));
    vm.debug_continue();
    assert_eq!(run(&mut vm), None);
}

#[test]
fn pausing_from_outside_interrupts_an_endless_loop() {
    let (mut vm, attention) = debug_vm("n = 0\nWhile 1\n    n = n + 1\nWend\n");
    vm.debug_pause(StopReason::Entry);
    assert_eq!(run(&mut vm), Some(StopReason::Entry));
    assert_eq!(line(&vm), 1);
    vm.debug_continue();
    // the flag comes from another thread; the VM hands over at a statement
    attention.store(true, Ordering::Relaxed);
    assert_eq!(vm.run(&mut NoHost).unwrap(), Status::Paused);
    assert_eq!(vm.debug_stop(), None);
    vm.debug_pause(StopReason::Pause);
    assert_eq!(run(&mut vm), Some(StopReason::Pause));
}

#[test]
fn stop_statement_pauses_and_goes_on() {
    let (mut vm, _) = debug_vm("a = 1\nStop\nPrint \"after\"\n");
    assert_eq!(run(&mut vm), Some(StopReason::Stop));
    assert_eq!(line(&vm), 2);
    vm.debug_continue();
    assert_eq!(run(&mut vm), None);
    assert_eq!(vm.core.take_output(), "after\n");
}

#[test]
fn an_uncaught_error_stops_where_it_happened_then_ends() {
    let (mut vm, _) = debug_vm("Function Boom(x)\n    Return 1 / x\nEnd Function\nPrint Boom(0)\n");
    assert_eq!(run(&mut vm), Some(StopReason::Error("Integer divide by zero".into())));
    assert_eq!(line(&vm), 2);
    assert_eq!(global(&vm, "x"), "0");
    vm.debug_continue();
    let e = vm.run(&mut NoHost).unwrap_err();
    assert_eq!(e.message, "Integer divide by zero");
    assert!(vm.is_finished());
}

#[test]
fn a_caught_error_does_not_stop() {
    let (mut vm, _) = debug_vm("Try\n    Throw \"x\"\nCatch e$\n    Print e\nEndTry\n");
    assert_eq!(run(&mut vm), None);
    assert_eq!(vm.core.take_output(), "x\n");
}

#[test]
fn shows_objects_arrays_and_paths() {
    let src = "Type Pos\n    Field x#\nEnd Type
Type Player\n    Field name$\n    Field pos.Pos\nEnd Type
Dim grid(1, 2)
grid(1, 2) = 42
Global p.Player = New Player
p\\name = \"Ann\"
p\\pos = New Pos
p\\pos\\x = 1.5
Local v[2]
v[1] = 7
Local i = 1
Stop
";
    let (mut vm, _) = debug_vm(src);
    assert_eq!(run(&mut vm), Some(StopReason::Stop));
    assert_eq!(global(&vm, "p").to_lowercase(), "player");
    assert_eq!(global(&vm, "P\\NAME$"), "\"Ann\"");
    assert_eq!(global(&vm, "p\\pos\\x"), vm.debug_describe(&neptune_vm::Value::Float(1.5)));
    assert_eq!(global(&vm, "grid(1, 2)"), "42");
    assert_eq!(global(&vm, "grid(i, 2)"), "42");
    assert_eq!(global(&vm, "grid(5, 2)"), "index out of bounds");
    assert_eq!(global(&vm, "v[1]"), "7");
    assert_eq!(global(&vm, "1 + 1"), "only variables, fields and elements can be shown");

    let p = vm.debug_eval(0, "p").unwrap();
    let names: Vec<String> = vm.debug_children(&p).into_iter().map(|c| c.0.to_lowercase()).collect();
    assert_eq!(names, vec!["name", "pos"]);

    let arrays = vm.debug_arrays();
    assert_eq!(arrays.len(), 1);
    assert_eq!(arrays[0].dims, vec![2, 3]);
    let items = vm.debug_array_items(arrays[0].index);
    assert_eq!(items.len(), 6);
    assert_eq!(items[5].0, "(1, 2)");
    assert_eq!(items[5].1.int(), 42);

    // compiler temporaries stay hidden
    assert!(vm.debug_locals(0).iter().all(|(n, _)| !n.starts_with('\0')));
}

#[test]
fn programs_not_being_debugged_are_unchanged() {
    assert_eq!(neptune_vm::run_source_captured("Print 1\nStop\nPrint 2\n").unwrap(), "1\n");
}
