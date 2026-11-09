# 11. Organising programs

[Contents](../MANUAL.md) · Previous: [10. Errors and debugging](10-errors.md) · Next: [12. The game loop](12-game-loop.md)

As programs grow, they need to be split up and kept tidy. This chapter
covers splitting a program into files, labels and jumps, ending a
program, and the settings that change how the whole program is read.

- [Several files: Include](#several-files-include)
- [A layout that works](#a-layout-that-works)
- [Ending a program](#ending-a-program)
- [Labels, Goto and Gosub](#labels-goto-and-gosub)
- [Arguments and the environment](#arguments-and-the-environment)
- [Program-wide settings](#program-wide-settings)
- [Modern and classic command names](#modern-and-classic-command-names)
- [Try it yourself](#try-it-yourself)

## Several files: Include

`Include "file.nb"` puts another file's program in at that point, as if
it were typed there. Each file is included once, however many files ask
for it, so files can include what they need freely.

<!-- nocheck -->
```basic
; main.nb
Include "player.nb"
Include "enemies.nb"
Include "hud.nb"

Graphics3D 1280, 720
SetupPlayer()
While Not KeyHit(KEY_ESCAPE)
    UpdatePlayer()
    UpdateEnemies()
    RenderWorld
    DrawHud()
    Flip
Wend
```

File names are relative to the file that includes them. `Import` is
another name for `Include`.

## A layout that works

A tidy program file usually goes:

1. a comment saying what the file is for;
2. `Include` lines;
3. `Const`ants and `Global` variables;
4. `Type`s and `Enum`s;
5. the main program;
6. `Function`s.

For a game of any size, keep one file per part of the game (player,
enemies, levels, menus, sound), each with its types, globals and
functions, and a short `main.nb` that includes them and runs the game
loop.

## Ending a program

A program ends when it reaches its last line, or at `End` (or `Stop`).
Use `End` to stop a program early, or to keep it from running into
`Data` lines or labelled sections at the end of the file.

```basic
Print "Goodbye!"
End
Print "this never runs"
```

Output:

```output
Goodbye!
```

## Labels, Goto and Gosub

A **label** marks a line: a name starting with `.`, on a line of its own.
`Restore` uses labels to find `Data` ([chapter 8](08-arrays-and-data.md#data-lines)).

`Goto label` jumps to a label. `Gosub label` jumps to it and comes back
at the next `Return`:

```basic
Gosub ShowTitle
Print "Playing..."
End

.ShowTitle
Print "== SPACE ADVENTURE =="
Return
```

Output:

```output
== SPACE ADVENTURE ==
Playing...
```

Functions do the same job better: they have parameters, their own
variables and a return value, so programs written with them are easier
to follow. `Goto` and `Gosub` are there for old programs and the
occasional quick jump.

## Arguments and the environment

A program can read the words typed after its name when it was started,
for example `neptune run game.nb level2 easy`:

| Command | Gives |
|---|---|
| `CommandLine()` | the words after the program's name, separated by spaces: `"level2 easy"` |
| `GetEnv(name$)`, `SetEnv name$, value$` | an environment variable |
| `SystemProperty("os")` | the operating system's name; also `"appdir"`, `"tempdir"`, `"homedir"`, `"cpucount"`, ... |
| `CurrentDate()`, `CurrentTime()` | today's date and the time, as text |

```basic
args = CommandLine()
If Instr(args, "easy") Then Print "Easy mode"
Print "Running on " + SystemProperty("os")
```

## Program-wide settings

Two words, at the very top of the main file, change how the whole
program is read:

| Setting | Effect |
|---|---|
| `Strict` | every variable must be declared with `Local`, `Global` or `Const` before use: misspelled names become errors |
| `Classic` | variables without a type mark are always Ints (`x = 2.5` stores 2), the rule older BASIC programs were written for |

```basic
Classic
x = 2.5        ; an Int in Classic mode
y# = 2.5       ; marked Float
Print x
Print y
```

Output:

```output
2
2.5
```

Without `Classic`, a variable's type comes from what is stored in it
([chapter 2](02-values-and-variables.md#how-a-variable-gets-its-type)).
Use `Classic` only for programs written for that rule.

## Modern and classic command names

Many commands have two names: a classic one and a modern one that says
more plainly what it does. Both work the same:

| Classic name | Modern name |
|---|---|
| `AppTitle` | `SetWindowTitle` |
| `LoadMesh` | `LoadModel` |
| `PositionEntity` | `SetEntityPosition` |
| `UpdateWorld` | `UpdatePhysics` |

The [command reference](../COMMANDS.md#modern-names) lists them all. Use
whichever you prefer, but pick one style for a program.

## Try it yourself

1. **Title screen.** Using `Gosub`, write a program that shows a title banner
   (`== DRAGON KEEP ==`), then prints `Your adventure begins...`, then shows a
   second banner (`Thanks for playing!`) and ends. Each banner is its own
   labelled section that comes back with `Return`.
   *Hint: put `End` before the labels so the program does not run into them.*
2. **Tidy gem hunt.** Write a small program laid out as the chapter suggests:
   a comment at the top, a `Const`, a `Type`, then the main program, then the
   functions. It makes `GEM_COUNT` (3) gems worth 10, 20 and 30, and a function
   `TotalValue()` that returns what they are all worth.
   *Hint: the main program can call a function that is written further down the file.*
3. **Difficulty from the command line.** Read the words typed after the
   program's name. If they contain `hard`, print `Hard mode: 3 lives`, if they
   contain `easy`, print `Easy mode: 9 lives`, and otherwise print `Normal
   mode: 5 lives`. Run it with `neptune run yourfile.nb hard` to try it.
   *Hint: `CommandLine()` gives the words as text, and `Instr` finds a word in it.*

### Answers

1.

```basic
Gosub Banner
Print "Your adventure begins..."
Gosub Goodbye
End

.Banner
Print "== DRAGON KEEP =="
Return

.Goodbye
Print "Thanks for playing!"
Return
```

```output
== DRAGON KEEP ==
Your adventure begins...
Thanks for playing!
```

`Gosub` jumps to the label and `Return` comes back to the line after the
`Gosub`. The `End` stops the program before it runs into the labelled
sections a second time.

2.

```basic
; Gem hunt: makes some gems and adds up their value.

Const GEM_COUNT = 3

Type Gem
    Field value
End Type

For i = 1 To GEM_COUNT
    g.Gem = New Gem
    g\value = i * 10
Next
Print "The gems are worth " + TotalValue()

Function TotalValue()
    total = 0
    For Each g In Gem
        total = total + g\value
    Next
    Return total
End Function
```

```output
The gems are worth 60
```

The file follows the usual order: comment, constant, type, main program,
function. A function can be written after the line that calls it, so the main
program can stay at the top where it is easy to find. The gems are worth
10, 20 and 30.

3.

```basic
args = CommandLine()
If Instr(args, "hard")
    Print "Hard mode: 3 lives"
ElseIf Instr(args, "easy")
    Print "Easy mode: 9 lives"
Else
    Print "Normal mode: 5 lives"
EndIf
```

```output
Normal mode: 5 lives
```

`Instr` gives the position of the word, or 0 when it is missing, and a
non-zero number counts as true. With nothing typed after the program's name
the text is empty, so the program prints the normal mode, as shown here.

[Contents](../MANUAL.md) · Previous: [10. Errors and debugging](10-errors.md) · Next: [12. The game loop](12-game-loop.md)
