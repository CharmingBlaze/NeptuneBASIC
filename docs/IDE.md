# Neptune IDE

The editor made for NeptuneBASIC. It works the way the Blitz3D IDE did: type,
press F5, and your program runs. Around that it has what a modern editor adds.

Build and start it with:

```
cargo build --release
target/release/neptune-ide
```

You can also name files or a folder to open: `neptune-ide game.nb` or
`neptune-ide mygame/`. The IDE uses the `neptune` program for compiling,
running and debugging. It looks next to itself and on the `PATH`; if `neptune`
is somewhere else, set its place in Settings.

## The keys that matter

| Key | What it does |
|---|---|
| F5 | Run. When stopped in the debugger, continue |
| F6 | Run the same program again |
| Ctrl+F5 | Run without the debugger |
| Shift+F5 | Stop |
| F7 | Check for errors |
| F1 | Show the command's parameters in the status bar; press again for its reference page |
| F9 | Set or clear a breakpoint (or click in the margin) |
| F10, F11, Shift+F11 | Step over, into, out |
| F4 | Pause a running program |
| F12 or Ctrl+click | Go to where a name is defined |
| Ctrl+P | Open any file in the project |
| Ctrl+Shift+P | Search every command the IDE has |
| Ctrl+T | Jump to a function, type or label |
| Ctrl+G | Go to a line |
| Ctrl+F, Ctrl+H, Ctrl+Shift+F | Find, replace, find in all files |
| Ctrl+D | Select the next use of this word as well, then type over them all |
| Alt+click | Add another caret |
| Ctrl+/ | Comment or uncomment lines |
| Alt+Up, Alt+Down | Move lines |
| Ctrl+Shift+I | Tidy the file: indent every block and fix capitals |

Help > Keyboard Shortcuts lists all of them.

## While you type

* Keywords and commands get their proper capitals, as in Blitz: type
  `positionentity` and it becomes `PositionEntity`.
* A new line is indented by block, and `EndIf`, `Wend`, `Next` and the rest
  move back out to line up with what they close.
* Suggestions appear after two letters, with each command's parameters and
  description. Letters in order are enough: `posent` finds `PositionEntity`.
  Short names like `for`, `func`, `main3d` and `main2d` offer ready-made blocks.
* Errors appear as you type, underlined and at the end of the line. F8 walks
  through them; the Problems tab lists them all.
* Hover over a command to read about it. When the program is stopped, hover
  over a variable to see its value.

## Working with programs

* An unsaved program can simply be run, as in Blitz.
* When a program fails, the IDE goes to the line with the error. Locations
  printed in the Output tab can be clicked.
* A program that reads with `Input` gets a box under the Output tab. `Input`
  does not work under the debugger, so use Ctrl+F5 for those programs.
* Program > Create Executable makes a stand-alone game (`neptune build`).
  Program > Program Command Line sets what the program receives.
* Closing the IDE never loses work: open files, unsaved text and breakpoints
  come back next time.
* Old `.bb` files that are not UTF-8 are opened and saved in their own
  encoding.

## Debugging

Click in the margin to set a breakpoint and press F5. The Debug tab shows the
call stack, the variables (open objects and arrays with the arrows) and your
watch expressions. Type an expression in the Watch column to add one;
right-click it to remove it. Debug Enabled, in the Program menu and on the
toolbar, decides whether F5 uses the debugger, as it did in Blitz.

## Settings

Ctrl+, opens them: four colour themes (one is Blitz's own blue), font size,
interface scale, tab width, and every automatic behaviour can be turned off.
They are kept in `ide.json`, in `%APPDATA%\NeptuneBASIC` on Windows,
`~/Library/Application Support/NeptuneBASIC` on macOS and
`~/.config/NeptuneBASIC` on Linux.

## Not there yet

Word wrap, regular expressions in Find, replacing across files, and editing a
variable's value while stopped.
