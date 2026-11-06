# NeptuneBASIC for Visual Studio Code

* Syntax highlighting for `.nb` files: keywords, every runtime command,
  strings with `$"{...}"` interpolation, `;` and `'` comments, `$FF` and
  `%101` numbers.
* Errors and warnings from `neptune check` shown in the editor when a file
  is opened or saved.
* **F5** (or the ▶ button) runs the current program in a terminal.
  *NeptuneBASIC: Run Program (Low Quality)* runs it with `--quality low`;
  *NeptuneBASIC: Build Stand-alone Game* runs `neptune build`.

Programs run from the workspace folder, so media paths behave the same as
`neptune run` in a terminal at the repository root.

## Install

1. Build NeptuneBASIC: `cargo build --release`.
2. Copy this folder into your VS Code extensions folder
   (`%USERPROFILE%\.vscode\extensions\neptunebasic` on Windows,
   `~/.vscode/extensions/neptunebasic` elsewhere) and restart VS Code, or
   package it with `npx @vscode/vsce package` and install the `.vsix`.
3. Set **neptunebasic.executable** to the full path of `neptune`
   (for example `C:\NeptuneBASIC\target\release\neptune.exe`) unless it is
   on your `PATH`.

After adding runtime commands, regenerate the grammar from the repository
root with `python tools/gen_grammar.py`.
