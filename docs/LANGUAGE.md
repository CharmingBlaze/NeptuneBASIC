# NeptuneBASIC language reference

NeptuneBASIC accepts classic Blitz3D BASIC and extends it. Keywords and
identifiers are case-insensitive.

## Programs and comments

```basic
; Blitz comment
' BASIC comment
x = 1 : y = 2            ; ':' separates statements
total = 1 + 2 + _
        3                ; '_' continues a line
Include "util.nb"        ; textual include (each file once)
Import "math.nb"         ; same as Include
```

## Types and variables

| type | tag | `As` name | notes |
|---|---|---|---|
| Int | `%` | `Int`, `Integer`, `Bool`, `Boolean` | 32-bit |
| Float | `#` | `Float`, `Single` | 32-bit |
| String | `$` | `String`, `Str` | |
| object | `.Type` | `TypeName` | reference to a `Type` instance |
| Variant | – | `Variant`, `Any` | dynamic, holds any value |

```basic
score% = 10          ; tag
Local speed As Float = 2.5
Global name$ = "Neo"
Const MAX = 100, TITLE$ = "Game"
```

### Smart variables (type inference)

Untagged variables take the type of what they are assigned, across the
whole function, so you rarely need tags:

```basic
x = 1.5              ; Float (Blitz3D would truncate to Int 2)
name = "Bob"         ; String
speed = 1
speed = speed + 0.5  ; Int and Float assignments widen to Float
p = New Player       ; object of type Player
Function Half(v#)    ; return type inferred as Float
    Return v / 2
End Function
```

* `Classic` (a statement at the top of the file) restores Blitz3D rules:
  untagged variables are Int.
* `Strict` requires every variable to be declared with `Local`/`Global`.
* Reading a variable that is never assigned anywhere produces a warning.

### Variants

```basic
Local v As Variant = 10
v = "ten"             ; type can change at run time
Print TypeOf(v)       ; "String"
```

Conversions between Int, Float and String are implicit (Blitz rules:
Float→Int rounds to nearest even, String→number parses the leading number).

## Operators (lowest to highest precedence)

`AndAlso` `OrElse` (short-circuit) · `Not` · `And` `Or` `Xor` (bitwise) ·
`=` `<>` `<` `>` `<=` `>=` · `+` `-` · `Shl` `Shr` `Sar` · `*` `/` `Mod` ·
`^` · unary `-` `+` `~` `Abs` `Sgn`, casts `Int()` `Float()` `Str()`.

Compound assignment: `+=`, `-=`, `*=`, `/=`.

`IIf(condition, a, b)` evaluates only the selected branch.

## Strings

```basic
s$ = "Score: " + score               ; numbers convert automatically
Print $"Player {name} has {hp * 2} HP {{literal braces}}"
```

## Control flow

```basic
If x > 1 Then Print "big" Else Print "small"
If a
    ...
ElseIf b          ; also "Else If"
    ...
Else
    ...
EndIf             ; also "End If"

While cond : ... : Wend
Repeat : ... : Until cond      ; or Forever
For i = 1 To 10 Step 2 : ... : Next
For Each e In Enemy : ... : Next        ; all objects of a Type
For Each v In values : ... : Next       ; elements of a Blitz array
For e.Enemy = Each Enemy : ... : Next   ; classic form
Exit / Continue

Select value
    Case 1, 2 : ...
    Case 3    : ...
    Default   : ...
End Select

Goto label / Gosub label / Return       ; labels: .label
```

## Functions

```basic
Function Greet$(name$, greeting$ = "Hello")
    Return greeting + ", " + name
End Function
Function Area(w, h) As Float
    Return w * h
End Function
```

## Enums

```basic
Enum State
    Idle             ; 0
    Running          ; 1
    Jumping = 10
    Falling          ; 11
End Enum
Local s As State = State.Running
```

## Types (objects), methods

```basic
Type Ship
    Field name$
    Field x#, speed# = 2.5      ; field defaults
    Field cargo[9]              ; fixed array field
    Method Move(dt#)
        Self\x = Self\x + Self\speed * dt
    End Method
End Type

s.Ship = New Ship
s\Move(0.016)
For Each s In Ship : Print s\name : Next
Delete s                      ; references to s become Null
Delete Each Ship
first.Ship = First Ship : nxt.Ship = After first
Insert nxt Before first
h = Handle(s) : s = Object.Ship(h)
```

## Arrays

```basic
Dim grid(10, 10)        ; global, indices 0..10, re-Dim clears
grid(3, 4) = 1
Local v[4]              ; Blitz array, indices 0..4
v[0] = 5
```

## Data

```basic
Restore levels
Read w, h, name$
.levels
Data 10, 20, "first"
```

## Errors

```basic
Try
    risky()
Catch e$
    Print "failed: " + e
End Try
Throw "custom error"
```

Runtime errors (division by zero, Null object access, out-of-range
indices, ...) report `file:line:col` and can be caught with `Try`.
