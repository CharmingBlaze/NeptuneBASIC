# NeptuneBASIC language summary

The whole language on one page, for when you know it and need a
reminder. To learn it, read [the manual](MANUAL.md); each section here
links to the chapter that explains it. Keywords and names are not
case-sensitive.

## Programs and comments ([chapter 1](manual/01-getting-started.md#how-programs-are-written))

```basic
; a comment                  ' also a comment
x = 1 : y = 2                ; ':' separates statements
total = 1 + 2 + _
        3                    ; ' _' continues a line
```

<!-- nocheck -->
```basic
Include "util.nb"            ; another file, included once (Import is the same)
```

## Types and variables ([chapter 2](manual/02-values-and-variables.md))

| Type | Mark | `As` names | Holds |
|---|---|---|---|
| Int | `%` | `Int`, `Integer`, `Bool`, `Boolean` | 32-bit whole numbers |
| Float | `#` | `Float`, `Single` | 32-bit floating point |
| String | `$` | `String`, `Str` | text |
| object | `.Type` | the type's name | a reference to a `Type` object, or `Null` |
| Variant | | `Variant`, `Any` | any value; `TypeOf(v)` says which |

```basic
score% = 10
Local speed As Float = 2.5
Global title$ = "Game"
Const MAX = 100, NAME$ = "Neptune"
x = 1.5               ; no mark: the type comes from what is stored (Float)
```

- An unmarked variable takes the type of what it is given; given both
  Ints and Floats, it is a Float. Unmarked **parameters** are Ints.
- Float to Int rounds to the nearest, halves to even. Text to number reads
  the leading number. Numbers joined to text become text.
- `Global` variables are seen everywhere; others belong to the function
  (or main program) they are used in. `Local` declares one explicitly.
- `Strict` at the top requires every variable to be declared. `Classic`
  makes unmarked variables Ints.
- Literals: `42`, `$FF` (hex), `%101` (binary), `2.5`, `1e-3`, `"text"`,
  `True` (1), `False` (0), `Null`, `Pi`.

## Operators ([chapter 3](manual/03-expressions.md))

From first to last: brackets and calls · unary `-` `~` · `^` · `*` `/`
`Mod` · `Shl` `Shr` `Sar` · `+` `-` · `=` `<>` `<` `>` `<=` `>=` · `And`
`Or` `Xor` (bitwise) · `Not` · `AndAlso` `OrElse` (stop early).

- `Int / Int` is a whole number; `^` is always a Float.
- Comparisons give 1 or 0; text compares letter by letter, case-sensitive.
- `x += 1`, `-=`, `*=`, `/=`.
- `IIf(condition, a, b)` works out only the chosen value.
- Built in: `Abs`, `Sgn`, `Int()`, `Float()`, `Str()`.

## Text ([chapter 7](manual/07-text.md))

```basic
name = "Ada" : hp = 7
s$ = "Score: " + 10
Print $"{name} has {hp * 2} HP {{in braces}}"
Print Mid("hello", 2, 3) + Upper(Left("neptune", 1)) + Chr(34)
```

## Decisions ([chapter 4](manual/04-decisions.md))

```basic
x = 5 : a = 1 : b = 0
If x > 1 Then Print "big" Else Print "small"

If a
    Print "a"
ElseIf b             ; or "Else If"
    Print "b"
Else
    Print "neither"
EndIf                ; or "End If"

Select x
    Case 1, 2 : Print "one or two"
    Case 5 : Print "five"
    Default : Print "something else"
End Select

Select True          ; ranges: the first true case wins
    Case x >= 10 : Print "ten or more"
    Case x >= 5 : Print "five or more"
End Select
```

## Loops ([chapter 5](manual/05-loops.md))

<!-- nocheck -->
```basic
For i = 1 To 10 Step 2 : ... : Next
While condition : ... : Wend
Repeat : ... : Until condition       ; or Forever
For Each e In Enemy : ... : Next     ; every object of a type
For Each v In values : ... : Next    ; every element of a fixed array
For e.Enemy = Each Enemy : ... : Next
Exit                                 ; leave the innermost loop
Continue                             ; go round again
```

## Functions ([chapter 6](manual/06-functions.md))

```basic
Function Greeting$(name$, word$ = "Hello")
    Return word + ", " + name
End Function

Function Area(w#, h#) As Float
    Return w * h
End Function

Print Greeting("Ada")
Print Area(2, 3.5)
```

Parameters are copies (objects are references). A function can't have
the name of a built-in command. `Return` with no value leaves early.

## Arrays and data ([chapter 8](manual/08-arrays-and-data.md))

```basic
Dim grid(10, 10)        ; places 0..10 each way; seen everywhere; Dim again clears
grid(3, 4) = 1
Dim names$(5)           ; the mark sets the element type
Local v#[4]             ; a fixed array, places 0..4
v[0] = 5.5
Restore levels
Read w, h, title$
Print title + " " + w + "x" + h
End
.levels
Data 10, 20, "first"
```

## Types, objects and enums ([chapter 9](manual/09-types.md))

```basic
Type Ship
    Field name$
    Field x#, speed# = 2.5          ; field defaults
    Field cargo[9]                  ; a fixed array field
    Method Move(dt#)
        Self\x = Self\x + Self\speed * dt
    End Method
End Type

s.Ship = New Ship
s\name = "Hawk"
s\Move(0.016)
For Each s In Ship : Print s\name : Next
top.Ship = First Ship : nxt.Ship = After top       ; also Last, Before
Insert top After nxt
h = Handle(s) : s = Object.Ship(h)                 ; object <-> number
Delete s                                           ; references become Null
Delete Each Ship

Enum State
    Idle             ; 0
    Running          ; 1
    Jumping = 10
End Enum
Local now As State = State.Running
```

## Errors ([chapter 10](manual/10-errors.md))

```basic
Try
    If 1 Then Throw "custom error"
Catch message$
    Print "failed: " + message
End Try
```

Runtime errors (dividing by zero, `Null` objects, array places that don't
exist, ...) report the file, line and column, and can be caught with
`Try`. `RuntimeError message$` stops the program with a message.

## Program structure ([chapter 11](manual/11-organising.md))

<!-- nocheck -->
```basic
Goto label / Gosub label / Return    ; labels are written .label
End                                  ; stop the program (Stop does the same, but pauses under the debugger)
```

`CommandLine()` gives the words after the program's name.

## Commands

Everything else (graphics, sound, input, physics, files, networking,
...) is a command: there are over a thousand, each in the [command
reference](COMMANDS.md) with an example, and part 3 of [the
manual](MANUAL.md) explains them area by area. Many have a modern name as
well as a classic one (`SetEntityPosition` and `PositionEntity`); both
work.
