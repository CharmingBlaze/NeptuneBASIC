# 2. Values and variables

[Contents](../MANUAL.md) · Previous: [1. Getting started](01-getting-started.md) · Next: [3. Expressions and operators](03-expressions.md)

Programs work with **values**: numbers like `42` and `2.5`, and text like
`"Hello"`. A **variable** is a name that holds a value, so you can keep it
and change it.

- [Kinds of value](#kinds-of-value)
- [Variables](#variables)
- [How a variable gets its type](#how-a-variable-gets-its-type)
- [Type marks](#type-marks)
- [Converting between types](#converting-between-types)
- [Constants](#constants)
- [Local and global variables](#local-and-global-variables)
- [True and false](#true-and-false)
- [Variants](#variants)
- [Strict mode](#strict-mode)

## Kinds of value

| Type | Holds | Examples |
|---|---|---|
| **Int** | whole numbers, from about -2 billion to +2 billion | `0`, `42`, `-7`, `$FF` (hexadecimal), `%101` (binary) |
| **Float** | numbers with a fractional part, about 7 digits accurate | `2.5`, `-0.01`, `3.0`, `1e6` |
| **String** | text, any length | `"Hello"`, `""` (empty), `"Score: 10"` |
| **objects** | a thing of a type you define ([chapter 9](09-types.md)) | `New Player` |
| **Variant** | any of the above, changing as the program runs | |

Ints are exact and fast: use them for counting, scores and positions on a
grid. Floats can hold fractions: use them for speeds, angles and
anything measured. A number written with a point (`3.0`) is a float; one
without (`3`) is an int.

```basic
Print 42
Print 2.5
Print 3.0
Print "Hello"
```

Output:

```output
42
2.5
3.0
Hello
```

## Variables

A variable is created the first time you give it a value with `=`:

```basic
lives = 3
name = "Ada"
speed = 1.5
Print name
Print lives
lives = lives - 1        ; take one away, and store the result in lives
Print lives
```

Output:

```output
Ada
3
2
```

`lives = lives - 1` is not an equation: it means "work out `lives - 1`,
then store it in `lives`". The right-hand side is worked out first, with
the old value.

Variable names:

- start with a letter, and contain letters, digits and `_`:
  `score`, `player2`, `max_speed`;
- are the same in any capitals: `Score` and `SCORE` are one variable;
- can't be a word the language uses, like `If`, `For`, `Type`, `End`,
  `Data` or `Field`.

## How a variable gets its type

Each variable has one type, which NeptuneBASIC works out from what you
store in it:

```basic
count = 10          ; an Int
height = 1.85       ; a Float
city = "Lisbon"     ; a String
speed = 1           ; an Int at first...
speed = speed + 0.5 ; ...but it is given a Float here, so it is a Float throughout
Print speed
```

Output:

```output
1.5
```

If a variable is given both ints and floats, it becomes a float
everywhere in that part of the program, so nothing is lost. A variable
can't hold a number in one place and text in another: that is an error,
because it is almost always a mistake. (When you really want a variable
that can hold anything, use a [Variant](#variants).)

A variable you read before ever giving it a value is 0 (or `""` for
text), and NeptuneBASIC warns you about it, since it is usually a typing
mistake.

## Type marks

You can also say a variable's type yourself, with a mark at the end of
its name or with `As`:

| Type | Mark | With `As` |
|---|---|---|
| Int | `%` | `As Int` (or `Integer`, `Bool`, `Boolean`) |
| Float | `#` | `As Float` (or `Single`) |
| String | `$` | `As String` (or `Str`) |
| Variant | | `As Variant` (or `Any`) |

```basic
score% = 10
ratio# = 0.75
title$ = "Neptune"
Local speed As Float = 2.5
Print title + " " + score
```

Output:

```output
Neptune 10
```

The mark is part of how you write the variable the first time; after
that `score` and `score%` are the same variable. Marks are useful when
the type is not obvious, and they are needed for [function
parameters](06-functions.md) that are not whole numbers.

## Converting between types

NeptuneBASIC converts between types when it needs to:

```basic
whole% = 2.7          ; a float stored in an int is rounded
Print whole
Print 7 / 2           ; int / int gives an int: the fraction is dropped
Print 7 / 2.0         ; if either is a float, the answer is a float
Print "Score: " + 10  ; numbers joined to text become text
```

Output:

```output
3
3
3.5
Score: 10
```

Rounding to an int goes to the nearest whole number; a number exactly
half way goes to the nearest even one (`2.5` becomes 2, `3.5` becomes 4).

To convert on purpose, use `Int`, `Float` and `Str`:

| Write | Gives |
|---|---|
| `Int(2.7)` | `3`: rounded to the nearest int |
| `Floor(2.7)`, `Ceil(2.1)` | `2.0`, `3.0`: always down, always up |
| `Int("42")` | `42`: the number at the start of the text (0 if there is none) |
| `Float(7)` | `7.0` |
| `Float("3.5")` | `3.5` |
| `Str(12)` | `"12"` |

```basic
typed = "12"
Print typed + 1           ; text + number: joined as text
Print Int(typed) + 1      ; number + number: added
```

Output:

```output
121
13
```

## Constants

A **constant** is a name for a value that never changes. Constants make
programs easier to read and to change: write the number once, at the top.

```basic
Const SCREEN_WIDTH = 1280
Const GRAVITY# = 9.81
Const GAME_NAME$ = "Orb Run"
Const LEFT = 0, RIGHT = 1       ; several on one line
Print GAME_NAME + " at " + SCREEN_WIDTH
```

Output:

```output
Orb Run at 1280
```

Giving a constant a new value is an error. NeptuneBASIC has many
constants of its own, like `KEY_ESCAPE`, `LIGHT_POINT` and `Pi`.

## Local and global variables

A variable used in the main program belongs to the main program. A
variable used inside a [function](06-functions.md) belongs to that
function, and disappears when it ends. Two functions can each have a
variable called `i` without disturbing each other.

To share a variable everywhere, declare it with `Global`, at the top of
the program:

```basic
Global lives = 3            ; seen everywhere, including in functions
score = 100                 ; only seen in the main program

Function LoseLife()
    lives = lives - 1       ; the global lives
End Function

LoseLife()
Print lives
```

Output:

```output
2
```

`Local` declares a variable that belongs to the function (or main
program) it is in, even if a global has the same name:

```basic
Global count = 10

Function Test()
    Local count = 1          ; a different count, only inside Test
    Print count
End Function

Test()
Print count
```

Output:

```output
1
10
```

Use globals for the few things the whole game needs (the player, the
score, the level), and pass everything else to functions as parameters.

## True and false

Comparisons give **1** for true and **0** for false, and `True` and
`False` are names for 1 and 0. Anywhere a condition is expected, 0 means
false and any other number means true.

```basic
Print 5 > 3
Print 5 < 3
Print True
alive = True
If alive Then Print "still alive"
```

Output:

```output
1
0
1
still alive
```

## Variants

A **Variant** can hold any value, and its type can change while the
program runs. `TypeOf` tells you what it holds.

```basic
Local v As Variant = 10
Print TypeOf(v)
v = "ten"
Print TypeOf(v)
v = 2.5
Print TypeOf(v) + ": " + v
```

Output:

```output
Int
String
Float: 2.5
```

Variants are handy for settings read from files or values that can be
of several kinds. They are a little slower than plain variables, and
NeptuneBASIC can't catch type mistakes in them before the program runs,
so use ordinary variables when you can.

## Strict mode

A typing mistake in a variable name makes a new variable, which can
cause bugs that are hard to find (`scroe = scroe + 1` instead of
`score`). `Strict`, at the top of a program, makes NeptuneBASIC insist
that every variable is declared first with `Local`, `Global` or `Const`:

```basic
Strict
Local score = 0
score = score + 10
Print score
```

Output:

```output
10
```

Without the `Local`, `score = score + 10` would be an error: "variable
'score' is not declared (Strict mode)". Strict mode is a good habit in
bigger programs.

[Contents](../MANUAL.md) · Previous: [1. Getting started](01-getting-started.md) · Next: [3. Expressions and operators](03-expressions.md)
