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

NeptuneBASIC is built from its source code with Rust's `cargo` tool:

1. Install Rust from [rustup.rs](https://rustup.rs).
2. Get the NeptuneBASIC source (for example with `git clone`).
3. In the NeptuneBASIC folder, build it:

```
cargo build --release
```

The `neptune` program is now in `target/release` (`neptune.exe` on
Windows). Add that folder to your `PATH`, or type the full path, to use it
from anywhere.

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
| `neptune commands` | lists every command |
| `neptune lsp` | the language server, used by editors |

## An editor

Programs are plain text, so any text editor works. Visual Studio Code is
best: the NeptuneBASIC extension in `editors/vscode` colours your code,
shows mistakes as you type, completes command names, shows what each
command does when you point at it, and runs your program with **F5**.
Its `README.md` explains how to install it. Other editors that support
the Language Server Protocol can use `neptune lsp` the same way.

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
