//! End-to-end language tests: compile NeptuneBASIC source and check output.

use neptune_vm::run_source_captured as run;

fn ok(src: &str) -> String {
    match run(src) {
        Ok(o) => o,
        Err(e) => panic!("program failed:\n{e}\n--- source ---\n{src}"),
    }
}

fn compile_err(src: &str) -> String {
    match run(src) {
        Ok(o) => panic!("expected an error, program printed:\n{o}"),
        Err(e) => e,
    }
}

#[test]
fn arithmetic_and_precedence() {
    assert_eq!(ok("Print 1 + 2 * 3\nPrint (1 + 2) * 3\nPrint 7 / 2\nPrint 7 Mod 3\nPrint 2 ^ 10\n"), "7\n9\n3\n1\n1024.0\n");
    assert_eq!(ok("Print -5 Shr 28\nPrint -5 Sar 1\nPrint 1 Shl 4\nPrint ~0\n"), "15\n-3\n16\n-1\n");
    assert_eq!(ok("Print 10 / 4.0\nPrint 1 / 3.0\n"), "2.5\n0.333333\n");
}

#[test]
fn blitz_conversions() {
    // float -> int rounds half to even like Blitz3D
    assert_eq!(ok("a% = 2.5\nb% = 3.5\nc% = -1.6\nPrint a\nPrint b\nPrint c\n"), "2\n4\n-2\n");
    assert_eq!(ok("s$ = 42\nPrint s + \"!\"\nn = Int(\"12abc\")\nPrint n + 1\nPrint Float(\"2.5\") * 2\n"), "42!\n13\n5.0\n");
    assert_eq!(ok("Print \"x\" + 1.5\nPrint Str(7) + Str 8\n"), "x1.5\n78\n");
}

#[test]
fn smart_variable_inference() {
    // untagged variables take the type of what is assigned
    assert_eq!(ok("x = 1.5\nPrint x\nname = \"Neptune\"\nPrint name\n"), "1.5\nNeptune\n");
    // Int + Float assignments widen to Float across the whole function
    assert_eq!(ok("speed = 1\nspeed = speed + 0.5\nPrint speed\n"), "1.5\n");
    // function return types are inferred
    assert_eq!(ok("Function Half(v#)\n Return v / 2\nEnd Function\nPrint Half(3)\n"), "1.5\n");
    // For loop variable inferred from bounds
    assert_eq!(ok("For t = 0 To 1 Step 0.5\n Print t\nNext\n"), "0.0\n0.5\n1.0\n");
}

#[test]
fn classic_mode_keeps_blitz_semantics() {
    assert_eq!(ok("Classic\nx = 1.5\nPrint x\n"), "2\n");
}

#[test]
fn variants_are_dynamic() {
    let src = "Local v As Variant = 10\nPrint TypeOf(v)\nv = \"ten\"\nPrint TypeOf(v) + \" \" + v\nv = 2.5\nPrint v * 2\nv = 1\nPrint v + 1\n";
    assert_eq!(ok(src), "Int\nString ten\n5.0\n2\n");
}

#[test]
fn control_flow() {
    let src = r#"
For i = 1 To 10
    If i = 3 Then Continue
    If i > 5 Then Exit
    Write i
Next
Print ""
n = 0
While n < 3
    n += 1
Wend
Print n
Repeat
    n -= 1
Until n = 0
Print n
For j = 10 To 1 Step -3 : Write j + " " : Next
Print
k = 0
Repeat
  k = k + 1
  If k = 4 Then Exit
Forever
Print k
"#;
    assert_eq!(ok(src), "1245\n3\n0\n10 7 4 1 \n4\n");
}

#[test]
fn if_elseif_else_and_logic() {
    let src = r#"
For x = 1 To 4
    If x = 1
        Print "one"
    ElseIf x = 2
        Print "two"
    Else If x = 3
        Print "three"
    Else
        Print "many"
    EndIf
Next
If 1 And Not 0 Then Print "yes" Else Print "no"
Function Boom()
  Print "evaluated"
  Return 1
End Function
If 0 AndAlso Boom() Then Print "bad"
If 1 OrElse Boom() Then Print "short"
"#;
    assert_eq!(ok(src), "one\ntwo\nthree\nmany\nyes\nshort\n");
}

#[test]
fn select_case() {
    let src = r#"
For i = 1 To 4
  Select i
    Case 1, 2
      Print "low"
    Case 3
      Print "three"
    Default
      Print "other"
  End Select
Next
s$ = "b"
Select s
  Case "a" : Print "A"
  Case "b" : Print "B"
End Select
"#;
    assert_eq!(ok(src), "low\nlow\nthree\nother\nB\n");
}

#[test]
fn functions_defaults_and_recursion() {
    let src = r#"
Function Fib(n)
    If n < 2 Then Return n
    Return Fib(n - 1) + Fib(n - 2)
End Function
Function Greet$(name$, greeting$ = "Hello")
    Return greeting + ", " + name
End Function
Print Fib(20)
Print Greet("Ada")
Print Greet("Bob", "Hi")
"#;
    assert_eq!(ok(src), "6765\nHello, Ada\nHi, Bob\n");
}

#[test]
fn globals_locals_and_consts() {
    let src = r#"
Const MAX = 3, NAME$ = "neo"
Global counter = 0
Function Bump()
    Local counter2 = 5
    counter = counter + 1
    Return counter2
End Function
Bump() : Bump()
Print counter
Print MAX * 2
Print NAME
"#;
    assert_eq!(ok(src), "2\n6\nneo\n");
}

#[test]
fn dim_arrays_and_blitz_vectors() {
    let src = r#"
Dim grid(3, 3)
For y = 0 To 3 : For x = 0 To 3
  grid(x, y) = x * 10 + y
Next : Next
Print grid(2, 3)
Dim names$(2)
names(1) = "mid"
Print names(1)
Local v[4]
v[4] = 99
Print v[4]
Dim grid(1, 1)
Print grid(1, 1)
"#;
    assert_eq!(ok(src), "23\nmid\n99\n0\n");
}

#[test]
fn types_lists_and_methods() {
    let src = r#"
Type Ship
    Field name$
    Field x#, speed# = 2.5
    Field hp = 100
    Method Move(dt#)
        Self\x = Self\x + Self\speed * dt
    End Method
    Method Describe$()
        Return Self\name + " @ " + Self\x
    End Method
End Type
For i = 1 To 3
    s.Ship = New Ship
    s\name = "ship" + i
Next
For s.Ship = Each Ship
    s\Move(2)
    If s\name = "ship2" Then Delete s
Next
For s.Ship = Each Ship
    Print s\Describe()
Next
f.Ship = First Ship
Print f\hp
l.Ship = Last Ship
Insert l Before f
Print (First Ship)\name
Delete Each Ship
Print First Ship = Null
"#;
    assert_eq!(ok(src), "ship1 @ 5.0\nship3 @ 5.0\n100\nship3\n1\n");
}

#[test]
fn deleted_objects_become_null_and_handles() {
    let src = r#"
Type T
  Field v
End Type
a.T = New T
a\v = 5
h = Handle(a)
b.T = Object.T(h)
Print b\v
Delete a
Print b = Null
Print Object.T(h) = Null
"#;
    assert_eq!(ok(src), "5\n1\n1\n");
}

#[test]
fn inferred_object_variables() {
    let src = "Type P\n Field n\nEnd Type\np = New P\np\\n = 3\nPrint p\\n\n";
    assert_eq!(ok(src), "3\n");
}

#[test]
fn string_append_keeps_aliases_and_length() {
    let src = r#"
a$ = "hi"
b$ = a
a = a + "!"
Print a
Print b
s$ = ""
For i = 1 To 500
    s = s + Chr(65 + (i Mod 26))
Next
t$ = s
s = s + "Z"
Print Len(s) + " " + Len(t)
Print Left(t, 3) + Right(t, 3) + Right(s, 1)
u$ = ""
For i = 1 To 40
    u = u + "é"
Next
Print Len(u)
w$ = "ééxé"
Print Mid(w, 2, 2) + " " + Instr(w, "x") + " " + Left(w, 1) + Right(w, 2)
"#;
    assert_eq!(ok(src), "hi!\nhi\n501 500\nBCDEFGZ\n40\néx 3 éxé\n");
}

#[test]
fn string_functions_and_interpolation() {
    let src = r#"
s$ = "Hello, World"
Print Left(s, 5) + "|" + Right(s, 5) + "|" + Mid(s, 8, 3)
Print Len(s) + " " + Instr(s, "World") + " " + Upper(s)
Print Replace(s, "World", "Neptune")
score = 41
Print $"Score: {score + 1}, pi~{Int(Pi)} {{braces}}"
Print Chr(65) + Asc("a") + Hex(255) + " " + Trim("  x  ")
"#;
    assert_eq!(
        ok(src),
        "Hello|World|Wor\n12 8 HELLO, WORLD\nHello, Neptune\nScore: 42, pi~3 {braces}\nA97000000FF x\n"
    );
}

#[test]
fn gosub_goto_and_data() {
    let src = r#"
Gosub sub
Goto skip
Print "never"
.skip
Restore nums
Read a, b, c$
Print a + b
Print c
End
.sub
Print "in sub"
Return
.nums
Data 3, 4, "data!"
"#;
    assert_eq!(ok(src), "in sub\n7\ndata!\n");
}

#[test]
fn try_catch_throw() {
    let src = r#"
Function Risky(n)
    If n = 0 Then Throw "zero!"
    Return 10 / n
End Function
Try
    Print Risky(2)
    Print Risky(0)
    Print "unreachable"
Catch e$
    Print "caught: " + e
End Try
Try
    Dim a(2)
    a(5) = 1
Catch err$
    Print Left(err, 21)
End Try
For i = 1 To 3
  Try
    If i = 2 Then Exit
  Catch m$
  End Try
Next
Try
  Throw "after loop"
Catch m$
  Print m
End Try
"#;
    assert_eq!(ok(src), "5\ncaught: zero!\narray index out of bo\nafter loop\n");
}

#[test]
fn goto_out_of_try_drops_only_the_blocks_it_leaves() {
    let src = r#"
Try
  Try
    Goto out
  Catch e$
    Print "inner"
  End Try
  .out
  Throw "x"
Catch e$
  Print "outer"
End Try
"#;
    assert_eq!(ok(src), "outer\n");
    // leaving everything: nothing is left to catch the Throw
    let e = compile_err("Try\nGoto out\nCatch e$\nPrint \"wrong\"\nEnd Try\n.out\nThrow \"uncaught\"\n");
    assert!(e.contains("uncaught"), "{e}");
    // a jump inside one Try, and into its own Catch, leaves it alone or fully
    assert_eq!(ok("Try\nGoto a\nPrint \"no\"\n.a\nThrow \"x\"\nCatch e$\nPrint e\nEnd Try\n"), "x\n");
    assert_eq!(ok("Try\nGoto a\nCatch e$\n.a\nPrint \"in catch\"\nEnd Try\n"), "in catch\n");
}

#[test]
fn gosub_return_restores_error_handlers() {
    let src = r#"
Try
  Gosub s
  Throw "after"
Catch m$
  Print m
End Try
End
.s
Try
  Return
Catch e$
  Print "wrong"
End Try
"#;
    assert_eq!(ok(src), "after\n");
    // Gosub and its label inside one Try keep that Try active
    let src = r#"
Try
  Gosub s
  Throw "still caught"
  .s
  Return
Catch m$
  Print m
End Try
"#;
    assert_eq!(ok(src), "still caught\n");
}

#[test]
fn catch_forgets_gosubs_it_interrupted() {
    let src = r#"
Try
  Gosub bad
  Print "wrong"
Catch e$
  Print "caught"
End Try
Return
.bad
Throw "x"
"#;
    assert_eq!(ok(src), "caught\n");
}

#[test]
fn cannot_jump_into_a_try() {
    let e = compile_err("Goto a\nTry\n.a\nCatch e$\nEnd Try\n");
    assert!(e.contains("cannot jump into a 'Try' block"), "{e}");
    let e = compile_err("Gosub a\nTry\n.a\nCatch e$\nEnd Try\n");
    assert!(e.contains("cannot jump into a 'Try' block"), "{e}");
    let e = compile_err("Try\nGoto a\nCatch e$\nEnd Try\nTry\n.a\nCatch e$\nEnd Try\n");
    assert!(e.contains("cannot jump into a 'Try' block"), "{e}");
}

#[test]
fn deep_code_compiles_or_reports_an_error() {
    let parens = |n: usize| format!("Print {}1{}\n", "(".repeat(n), ")".repeat(n));
    let ifs = |n: usize| format!("{}Print 1\n{}", "If 1\n".repeat(n), "EndIf\n".repeat(n));
    let unary = |n: usize| format!("Print {}1\n", "- ".repeat(n));
    let chain = |n: usize| format!("Print 1{}\n", " + 1".repeat(n));
    let calls = |n: usize| format!("Function f(a)\nReturn a\nEnd Function\nPrint {}1{}\n", "f(".repeat(n), ")".repeat(n));
    let interp = |n: usize| format!("Print {}1{}\n", "$\"{".repeat(n), "}\"".repeat(n));
    let elseifs = |n: usize| format!("x = {n}\nIf x = 0\nPrint 0\n{}EndIf\n", (1..=n).map(|i| format!("ElseIf x = {i}\nPrint {i}\n")).collect::<String>());
    let fields = |n: usize| format!("Type T\nField t.T\nEnd Type\na.T = New T\nb.T = a{}\n", "\\t".repeat(n));
    let too_deep = "nested too deeply";

    assert_eq!(ok(&parens(150)), "1\n");
    assert_eq!(ok(&ifs(200)), "1\n");
    assert_eq!(ok(&chain(3000)), "3001\n");
    assert_eq!(ok(&calls(150)), "1\n");
    assert_eq!(ok(&elseifs(900)), "900\n");

    for src in [parens(50_000), ifs(50_000), unary(50_000), chain(50_000), calls(50_000), interp(1_000), elseifs(5_000), fields(50_000)] {
        let e = compile_err(&src);
        assert!(e.contains(too_deep), "{e}");
        assert_eq!(e.matches("error:").count(), 1, "{e}");
    }
    // the deepest mix the limit allows must still fit on the compiler's stack
    let mix = format!("{}Print 1{}\n{}", "If 1\n".repeat(150), " + 1".repeat(1_500), "EndIf\n".repeat(150));
    assert_eq!(ok(&mix), "1501\n");
}

#[test]
fn byte_order_mark_is_ignored() {
    assert_eq!(ok("\u{feff}Print 1\n"), "1\n");
    let e = compile_err("\u{feff}Print (\n");
    assert!(e.contains("main.nb:1:"), "{e}");
    assert!(!e.contains('\u{feff}'), "{e}");
}

#[test]
fn runtime_errors_have_locations() {
    let e = run("x = 1\ny = 0\nPrint x / y\n").unwrap_err();
    assert!(e.contains("Integer divide by zero"), "{e}");
    assert!(e.contains("main.nb:3"), "{e}");
    let e = run("Type T\nField a\nEnd Type\nt.T = Null\nPrint t\\a\n").unwrap_err();
    assert!(e.contains("Object does not exist") && e.contains("main.nb:5"), "{e}");
}

#[test]
fn compile_errors() {
    assert!(compile_err("Print Foo(1)\n").contains("function 'foo' not found"));
    assert!(compile_err("Strict\nx = 1\n").contains("not declared"));
    assert!(compile_err("Exit\n").contains("inside a loop"));
    assert!(compile_err("Goto nowhere\n").contains("label 'nowhere' not found"));
    assert!(compile_err("s$ = \"a\" - 1\n").contains("only '+'"));
    assert!(compile_err("x# = 1\nx$ = \"s\"\n").contains("type tag"));
    let e = compile_err("Function F(a)\nEnd Function\nF 1, 2\n");
    assert!(e.contains("expects 1 argument"), "{e}");
    // diagnostics point at the source line
    let e = compile_err("Print 1\nWhile 1\n");
    assert!(e.contains("main.nb:"), "{e}");
}

#[test]
fn strict_mode_with_declarations() {
    let src = "Strict\nLocal total As Float = 0\nFor Local_i = 1 To 1 : Next\n";
    assert!(run(src).is_err());
    let src = "Strict\nLocal total As Float = 0\nLocal i As Int\nFor i = 1 To 4\n total += i / 2.0\nNext\nPrint total\n";
    assert_eq!(ok(src), "5.0\n");
}

#[test]
fn math_and_random() {
    assert_eq!(ok("Print Sin(90)\nPrint Sqr(16)\nPrint ATan2(1, 1)\nPrint Abs(-3) + Sgn(-2.5)\n"), "1.0\n4.0\n45.0\n2.0\n");
    let out = ok("SeedRnd 7\nok = 1\nFor i = 1 To 1000\n r = Rand(1, 6)\n If r < 1 Or r > 6 Then ok = 0\n f# = Rnd(2)\n If f < 0 Or f > 2 Then ok = 0\nNext\nPrint ok\n");
    assert_eq!(out, "1\n");
}

#[test]
fn file_io_roundtrip() {
    let dir = std::env::temp_dir().join(format!("neptune_io_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("t.dat").to_string_lossy().replace('\\', "/");
    let src = format!(
        "f = WriteFile(\"{path}\")\nWriteInt f, 1234\nWriteFloat f, 2.5\nWriteString f, \"neptune\"\nWriteLine f, \"line two\"\nCloseFile f\n\
         f = ReadFile(\"{path}\")\nPrint ReadInt(f)\nPrint ReadFloat(f)\nPrint ReadString(f)\nPrint ReadLine(f)\nPrint Eof(f)\nCloseFile f\nPrint FileType(\"{path}\")\n"
    );
    assert_eq!(ok(&src), "1234\n2.5\nneptune\nline two\n1\n1\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn performance_smoke() {
    // tight integer loop: guards against accidental slow paths
    let t = std::time::Instant::now();
    let out = ok("total = 0\nFor i = 1 To 3000000\n total = total + (i And 7)\nNext\nPrint total\n");
    assert_eq!(out, "10500000\n");
    assert!(t.elapsed().as_secs_f32() < 10.0, "loop too slow: {:?}", t.elapsed());
}

#[test]
fn interpolation_with_nested_strings() {
    let out = ok(r#"
name$ = "x"
Print $"[{IIf(name = "x", "yes", "no")}] {{literal}} {Upper("ab") + "}"}"
"#);
    assert_eq!(out, "[yes] {literal} AB}\n");
}

#[test]
fn enums() {
    let src = r#"
Enum State
    Idle
    Running
    Jumping = 10
    Falling
End Enum
Local s As State = State.Running
Print s
Print State.Falling
Select s
  Case State.Idle : Print "idle"
  Case State.Running : Print "running"
End Select
"#;
    assert_eq!(ok(src), "1\n11\nrunning\n");
    assert!(compile_err("Enum E\n A\nEnd Enum\nPrint E.B\n").contains("no member 'b'"));
}

#[test]
fn iif_is_lazy_and_typed() {
    let src = r#"
Function Loud(v)
  Print "called " + v
  Return v
End Function
x = 5
Print IIf(x > 3, "big", "small")
Print IIf(x > 3, Loud(1), Loud(2))
Print IIf(0, 1, 2.5)
"#;
    assert_eq!(ok(src), "big\ncalled 1\n1\n2.5\n");
}

#[test]
fn for_each_in() {
    let src = r#"
Type Enemy
  Field hp = 10
End Type
For i = 1 To 3 : e = New Enemy : e\hp = i : Next
total = 0
For Each foe In Enemy
  total += foe\hp
Next
Print total
Local scores#[2]
scores[0] = 1.5 : scores[1] = 2 : scores[2] = 3
sum# = 0
For Each s In scores
  sum += s
Next
Print sum
Local flag As Boolean = True
Print flag
"#;
    assert_eq!(ok(src), "6\n6.5\n1\n");
}

#[test]
fn example_modern_tour() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/modern.nb")).unwrap();
    assert_eq!(
        ok(&src),
        "Ember (hp 100, level 1.5) is fire\nTide (hp 100, level 1.5) is water\nGolem (hp 100, level 1.5) is earth\n\
         Battle over: Ember was defeated\nloot is Int 25\nloot is String: a golden key\n2 monsters still standing\nTotal damage: 25.0\n"
    );
}

#[test]
fn banks() {
    let src = r#"
b = CreateBank(8)
PokeInt b, 0, 123456
PokeFloat b, 4, 2.5
Print PeekInt(b, 0)
Print PeekFloat(b, 4)
PokeByte b, 1, 255
Print PeekByte(b, 1)
ResizeBank b, 12
Print BankSize(b)
c = CreateBank(12)
CopyBank b, 0, c, 4, 8
Print PeekFloat(c, 8)
Try
    PeekInt b, 10
Catch e$
    Print "caught"
End Try
FreeBank b
"#;
    assert_eq!(ok(src), "123456\n2.5\n255\n12\n2.5\ncaught\n");
}

#[test]
fn file_seek_dirs_and_bytes() {
    let dir = std::env::temp_dir().join(format!("neptune-lang-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
    let src = format!(
        r#"
f = WriteFile("{d}/a.bin")
b = CreateBank(4)
PokeInt b, 0, 77
WriteBytes b, f, 0, 4
WriteInt f, 99
CloseFile f
f = ReadFile("{d}/a.bin")
Print ReadAvail(f)
SeekFile f, 4
Print FilePos(f)
Print ReadInt(f)
SeekFile f, 0
Print ReadBytes(b, f, 0, 4)
Print PeekInt(b, 0)
CloseFile f
WriteFile("{d}/b.txt")
dir = ReadDir("{d}")
n = 0
Repeat
    name$ = NextFile(dir)
    If name <> "" Then n = n + 1 : Print name
Until name = ""
CloseDir dir
"#
    );
    assert_eq!(ok(&src), "8\n4\n99\n4\n77\n.\n..\na.bin\nb.txt\n");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn timers_and_system() {
    let src = r#"
t = CreateTimer(100)
start = MilliSecs()
ticks = 0
For i = 1 To 5
    ticks = ticks + WaitTimer(t)
Next
FreeTimer t
elapsed = MilliSecs() - start
Print ticks >= 5
Print elapsed >= 35
SeedRnd 1234
Print RndSeed()
SetEnv "NEPTUNE_TEST_VAR", "hello"
Print GetEnv("NEPTUNE_TEST_VAR")
Print SystemProperty("tempdir") <> ""
"#;
    assert_eq!(ok(src), "1\n1\n1234\nhello\n1\n");
}

#[test]
fn tcp_and_udp_loopback() {
    // a fixed high port keeps the BASIC simple; retry a few in case one is taken
    for port in [47811, 47823, 47837, 47851] {
        let src = format!(
            r#"
TCPTimeouts 2000, 2000
server = CreateTCPServer({port})
If server = 0 Then RuntimeError "port in use"
client = OpenTCPStream("127.0.0.1", {port})
conn = AcceptTCPStream(server)
WriteLine client, "ping"
WriteInt client, 42
Print ReadLine(conn)
Print ReadInt(conn)
Print DottedIP(TCPStreamIP(conn))
CloseTCPStream client
CloseTCPStream conn
CloseTCPServer server

UDPTimeouts 2000
a = CreateUDPStream()
b = CreateUDPStream({udp})
WriteString a, "hello udp"
SendUDPMsg a, $7F000001, {udp}
from = RecvUDPMsg(b)
Print DottedIP(from)
Print ReadString(b)
Print CountHostIPs("127.0.0.1") >= 1
"#,
            udp = port + 1
        );
        match run(&src) {
            Ok(out) => {
                assert_eq!(out, "ping\n42\n127.0.0.1\n127.0.0.1\nhello udp\n1\n");
                return;
            }
            Err(e) if e.contains("port in use") => continue,
            Err(e) => panic!("{e}"),
        }
    }
    panic!("no free port for the network test");
}

#[test]
fn blitz3d_language_audit() {
    // A sweep of Blitz3D syntax and built-in behaviour (typed arrays, type
    // lists, Data, Gosub/Goto, rounding, shifts, string functions, ...).
    // Rand/Rnd lines only check ranges, so the output is deterministic
    program_matches("blitz_audit");
}

/// Runs `tests/programs/<name>.nb` and compares with `<name>.expected`.
fn program_matches(name: &str) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/programs/");
    let src = std::fs::read_to_string(format!("{dir}{name}.nb")).unwrap();
    let expected = std::fs::read_to_string(format!("{dir}{name}.expected")).unwrap().replace("\r\n", "\n");
    assert_eq!(ok(&src), expected);
}

// The expected output of the next two was printed by the interpreter as it
// was before superinstructions (`neptune-lang/src/fuse.rs`), typed `Dim`
// arrays and the in-place value stack: they must not change what programs do.

#[test]
fn fused_instructions_behave_like_the_plain_ones() {
    // every comparison against variables and constants, loops up and down,
    // arithmetic with constants (wrapping, negative divisors), NaN
    program_matches("fused_ops");
}

#[test]
fn stack_and_arrays_behave_as_before() {
    // typed arrays with conversions and bounds errors, fields, vectors,
    // recursion deep enough to grow the stack, errors caught mid-expression
    program_matches("stack_and_arrays");
}

fn warnings(src: &str) -> String {
    let env = neptune_vm::stdlib::base_env();
    let c = neptune_lang::compile("main.nb", src, &env, &mut neptune_lang::NoLoader);
    c.diagnostics.render(&c.program.expect("program compiles").sources)
}

#[test]
fn never_assigned_warning() {
    assert!(warnings("Print speed\n").contains("'speed' is used but never assigned"));
    // assigned from a variable whose type could not be inferred
    let src = "While n < 3\n    If shot = 0 Then shot = n\n    n = n + 1\nWend\nPrint shot\n";
    assert!(!warnings(src).contains("never assigned"), "{}", warnings(src));
}

#[test]
fn settings_save_and_load() {
    let file = std::env::temp_dir().join(format!("neptune-settings-{}.txt", std::process::id()));
    let path = file.to_string_lossy().replace('\\', "/");
    let src = format!(
        "SetSetting \"Audio.Music\", 0.5\nSetSetting \"name\", \"Ann\"\nSetSetting \"best\", 1200\nSaveSettings \"{path}\"\nClearSettings\nPrint SettingExists(\"name\")\nPrint LoadSettings(\"{path}\")\nPrint GetSetting(\"NAME\") + \" \" + GetSettingInt(\"best\") + \" \" + GetSettingFloat(\"audio.music\")\nPrint GetSetting(\"missing\", \"none\") + \" \" + GetSettingInt(\"missing\", 7)\nDeleteSetting \"name\"\nPrint SettingExists(\"name\")\nPrint LoadSettings(\"no such file.txt\")\n"
    );
    let out = ok(&src);
    std::fs::remove_file(&file).ok();
    assert_eq!(out, "0\n1\nAnn 1200 0.5\nnone 7\n0\n0\n");
}

#[test]
fn runtime_errors_list_the_calls_that_led_there() {
    let e = compile_err(
        "Function Divide(a, b)
    Return a / b
End Function

Function Half(n)
    Return Divide(n, 0)
End Function

Gosub work
End

.work
Print Half(10)
Return
",
    );
    let lines: Vec<&str> = e.lines().collect();
    assert_eq!(lines.len(), 5, "{e}");
    assert_eq!(lines[0], "runtime error: Integer divide by zero");
    assert!(lines[1].starts_with("  at ") && lines[1].ends_with(":2:5 (in divide)"), "{e}");
    assert!(lines[2].starts_with("  called from ") && lines[2].ends_with(":6:5 (in half)"), "{e}");
    assert!(lines[3].starts_with("  called from ") && lines[3].ends_with(":13:1 (in <main>)"), "{e}");
    assert!(lines[4].starts_with("  called from ") && lines[4].ends_with(":9:1 (in <main>)"), "{e}");
}

#[test]
fn runtime_error_in_the_main_program_has_no_callers() {
    let e = compile_err("x = 0\nPrint 1 / x\n");
    assert_eq!(e.lines().count(), 2, "{e}");
    assert!(!e.contains("called from"), "{e}");
}

#[test]
fn very_deep_call_stacks_leave_out_the_middle() {
    let e = compile_err(
        "Function Down(n)
    If n = 0 Then RuntimeError \"bottom\"
    Return Down(n - 1)
End Function
Print Down(100)
",
    );
    let lines: Vec<&str> = e.lines().collect();
    // message, where it happened, 16 callers, the gap, 4 callers
    assert_eq!(lines.len(), 23, "{e}");
    assert_eq!(lines[0], "runtime error: bottom");
    assert_eq!(lines[18], "  ... 81 more calls ...", "{e}");
    assert!(lines[22].ends_with(":5:1 (in <main>)"), "{e}");
}
