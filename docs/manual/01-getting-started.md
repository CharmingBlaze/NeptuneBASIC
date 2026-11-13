# 1. Getting started

[Contents](../MANUAL.md) · Next: [2. Values and variables](02-values-and-variables.md)

## What NeptuneBASIC is

NeptuneBASIC is a language and an engine in one. You write a program,
for example:

```basic
Print "Hello!"
```

save it in a file ending in `.nb`, and run it with the `neptune` program.
For a game, the program opens a window and draws in it; NeptuneBASIC
takes care of the graphics card, the sound card, the keyboard and the
rest, so your program can be about the game.

Under the surface it draws with the Fyrox engine (written in Rust), which
gives realistic lighting, shadows, physics and animated 3D models. Your
programs run the same on Windows, Linux and macOS.

## Installing

Download the NeptuneBASIC folder for your computer (Windows, Linux or
macOS) from the [releases page](https://github.com/CharmingBlaze/NeptuneBASIC/releases)
and unzip it anywhere. Nothing else needs installing. The folder holds:

| | |
|---|---|
| `neptune-ide` | Neptune IDE, where you write and run programs |
| `neptune` | the compiler, which the IDE uses; you can also run it yourself |
| `START.txt` | the first steps, and what to do on macOS and Linux the first time |
| `cube.nb` and `examples` | programs to run, read and change |
| `docs` | this manual and the command reference |
| `blender` | the add-on for making levels in Blender |

Keep `neptune` and `neptune-ide` together in that folder. To use `neptune`
from a terminal anywhere, add the folder to your `PATH`.

**Building it yourself.** If you would rather build NeptuneBASIC from its
source code, install Rust from [rustup.rs](https://rustup.rs), get the
source (for example with `git clone`), and in its folder run
`cargo build --release`. The programs are then in `target/release`.

## Running a program

Make a file called `hello.nb` containing:

```basic
Print "Hello, world!"
```

and run it:

```
neptune run hello.nb
```

Output:

```output
Hello, world!
```

The `neptune` program has a few other commands:

| Command | Does |
|---|---|
| `neptune run game.nb` | compiles and runs the program |
| `neptune check game.nb` | only checks it for mistakes |
| `neptune build game.nb` | makes a stand-alone game you can give to people ([chapter 25](25-sharing.md)) |
| `neptune kit` | packs the compiler and the IDE into one folder you can give to someone who wants to make games |
| `neptune commands` | lists every command |
| `neptune lsp` | the language server, used by editors |

## An editor

The easiest place to write programs is **Neptune IDE**, which comes with
NeptuneBASIC. Start `neptune-ide` from the folder you unzipped, type a program,
and press **F5** to run it. It colours your code, shows mistakes as you
type, suggests command names, and has this manual and every command's
explanation and example built in: put the caret on a command and press
**F1**. The [IDE guide](../IDE.md) has the rest.

Programs are plain text, so any text editor works too. For Visual Studio
Code, the extension in `editors/vscode` does much the same as the IDE, and
its `README.md` explains how to install it. Other editors that support the
Language Server Protocol can use `neptune lsp`.

## Your first program

Let's write a small program that asks your name and greets you:

```basic
; greet.nb: my first program
name = Input("What is your name? ")
Print "Hello, " + name + "!"
Print "Your name has " + Len(name) + " letters."
```

Run it, type your name, and press Enter:

```
What is your name? Ada
Hello, Ada!
Your name has 3 letters.
```

Line by line:

- `; greet.nb: my first program` is a **comment**. Everything after a `;`
  is ignored by the computer: it is a note for people reading the program.
- `Input("What is your name? ")` shows the question, waits for you to
  type a line, and gives back what you typed. `name = ...` stores it in a
  **variable** called `name`.
- `Print` shows a line of text. `+` joins pieces of text together.
- `Len(name)` gives the number of letters in `name`.

`Print`, `Input` and `Len` are **commands**. NeptuneBASIC has over a
thousand of them, for everything from maths to 3D physics; the
[command reference](../COMMANDS.md) lists them all.

## Your first game

Now something with a window. This program shows a red cube you can
spin with the arrow keys:

```basic
; spin.nb: a cube you can turn
Graphics3D 1280, 720                 ; open a 1280 x 720 window for 3D

camera = CreateCamera()              ; something to look through
PositionEntity camera, 0, 2, -5      ; up a little and back
CameraLookAt camera, 0, 0, 0         ; looking at the middle

light = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity light, 45, 30, 0          ; light from above, at an angle

cube = CreateCube()
EntityColor cube, 255, 60, 60        ; red

While Not KeyHit(KEY_ESCAPE)         ; until Escape is pressed...
    If KeyDown(KEY_LEFT) Then TurnEntity cube, 0, 2, 0
    If KeyDown(KEY_RIGHT) Then TurnEntity cube, 0, -2, 0
    RenderWorld                      ; draw the 3D scene
    Flip                             ; show it
Wend
```

Every game has this shape: **set things up**, then **loop**: read the
input, update the game, draw a frame, show it. [Chapter 12](12-game-loop.md)
explains it fully, but part 2 comes first: the language itself.

## How programs are written

A few things are true everywhere in NeptuneBASIC:

- **One statement per line.** To put several on one line, separate them
  with `:`. A long line can continue on the next if it ends in ` _`.
- **Capital letters don't matter.** `Print`, `print` and `PRINT` are the
  same, and so are the variables `Score` and `score`.
- **Comments** start with `;` (or `'`) and run to the end of the line.

```basic
x = 1 : y = 2                ; two statements on one line
total = x + y + _
        10                   ; one statement on two lines
PRINT total                  ; the same as Print
```

Output:

```output
13
```

## When something goes wrong

If you make a mistake, `neptune` tells you where, and what it expected:

```
error: function 'prnt' not found
  --> greet.nb:1:1
  |
1 | Prnt "Hello"
  | ^^^^
```

`greet.nb:1:1` means file `greet.nb`, line 1, column 1. Fix the line and
run it again. [Chapter 10](10-errors.md) has more about errors.

[Contents](../MANUAL.md) · Next: [2. Values and variables](02-values-and-variables.md)
