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
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/programs/");
    let src = std::fs::read_to_string(format!("{dir}blitz_audit.nb")).unwrap();
    let expected = std::fs::read_to_string(format!("{dir}blitz_audit.expected")).unwrap().replace("\r\n", "\n");
    // Rand/Rnd lines only check ranges, so the output is deterministic
    assert_eq!(ok(&src), expected);
}
