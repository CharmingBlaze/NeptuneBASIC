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

[Contents](../MANUAL.md) · Previous: [10. Errors and debugging](10-errors.md) · Next: [12. The game loop](12-game-loop.md)
