# NeptuneBASIC for Visual Studio Code

* Syntax highlighting for `.nb` files: keywords, every runtime command,
  strings with `$"{...}"` interpolation, `;` and `'` comments, `$FF` and
  `%101` numbers.
* Powered by the NeptuneBASIC language server (`neptune lsp`):
  * errors and warnings as you type,
  * completion of keywords, commands (with their signatures), constants and
    your own functions, types, globals and labels,
  * hover help and signature help while typing arguments,
  * outline (document symbols) and go to definition.
* **F5** (or the debug button) debugs the current program: breakpoints,
  stepping into, over and out of functions, pause, the call stack, locals,
  globals, `Dim` arrays and objects you can open, watches and values on hover.
  It also stops on a runtime error, at the line where it happened, and at a
  `Stop` statement. The output of `Print` (before `Graphics`) and `DebugLog`
  appears in the Debug Console.
* **Ctrl+F5** (or the ▶ button) runs the current program in a terminal.
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

The extension has no npm dependencies. If the language server cannot be
started it falls back to running `neptune check` when files are opened or
saved.

## Other editors

Any editor with LSP support can use `neptune lsp` (stdin/stdout). For
example, Neovim:

```lua
vim.filetype.add({ extension = { nb = "neptunebasic" } })
vim.lsp.start({ name = "neptune", cmd = { "neptune", "lsp" }, filetypes = { "neptunebasic" } })
```

Helix (`languages.toml`):

```toml
[language-server.neptune]
command = "neptune"
args = ["lsp"]

[[language]]
name = "neptunebasic"
scope = "source.neptune"
file-types = ["nb"]
comment-token = ";"
language-servers = ["neptune"]
```

After adding runtime commands, regenerate the grammar from the repository
root with `python tools/gen_grammar.py`.
