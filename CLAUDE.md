# NeptuneBASIC

A Blitz3D-compatible BASIC for 3D games, written in Rust, running on Fyrox 1.0.
`.nb` source → `neptune-lang` (compiler) → `neptune-vm` (bytecode VM) → `neptune-runtime` (Fyrox commands) → `neptune-cli` (`neptune` binary).

## Where things are (don't explore to find these)

| Need | Look in |
|---|---|
| Lexer / parser / type checking | `crates/neptune-lang/src/{lexer,parser,sema}.rs` |
| IR, bytecode, codegen, superinstructions | `crates/neptune-lang/src/{ir,bytecode,codegen,fuse}.rs` |
| Command signature syntax | `crates/neptune-lang/src/commands.rs` |
| VM, values, heap | `crates/neptune-vm/src/{vm,value}.rs` |
| Console/math/string/file commands | `crates/neptune-vm/src/stdlib.rs` (`COMMANDS`) |
| 3D/2D/input commands | `crates/neptune-runtime/src/commands.rs` (`COMMANDS`, `ALIASES`, `constants()`) |
| Entity table, `Host3D`, `Kind` | `crates/neptune-runtime/src/world.rs` |
| Blitz↔Fyrox coordinate mirror | `crates/neptune-runtime/src/transform.rs` |
| Feature modules | runtime `collision`, `pick`, `mesh`, `model`, `anim`, `texture`, `material`, `classic` (fixed-function shader), `sprite`, `terrain`, `physics`, `sound`, `quality`, `buffer`, `draw2d`, `image2d`, `input`, `joy` (gamepads), `keyanim`, `mirror`, `screenshot`, `primitive` |
| Blender levels and editor scenes (`LoadLevelFile`, custom properties) | runtime `level.rs` (glTF is read from the file's JSON, `.rgs` from the loaded scene's nodes); add-on in `tools/blender`; guides `docs/BLENDER.md`, `docs/EDITOR.md` |
| 2D tool import | runtime `tiled.rs` (Tiled maps and the shared `MapDef`), `mapformats.rs` (LDtk, Ogmo, Sprite Fusion, Pyxel Edit, Aseprite tilemaps, Tiled worlds), `tilegrid.rs` (grid shapes), `inflate.rs`, `spritesheet.rs` (Aseprite, TexturePacker); guide `docs/TOOLS2D.md` |
| Character controllers (`Char*`) | runtime `charcontrol.rs` (the rules, tested without the engine), `character.rs` (3D and 2D physics); guide `docs/CHARACTERS.md` |
| Retained GUI (`Gui*`) | runtime `gui.rs` (core, DPI via `px`), `gui_lists.rs`, `gui_dialogs.rs`, `gui_editors.rs` |
| Networking (TCP/UDP) | `crates/neptune-vm/src/net.rs` |
| Multiplayer | rules `crates/neptune-vm/src/netgame.rs` (no sockets, tested alone); iroh transport `netlink.rs` (feature `online`); entity sync runtime `netsync.rs`; host-checked characters `netchar.rs` (rules, tested alone); examples `examples/games/coinrush.nb`, `examples/netcharacters.nb` |
| Plugins (registry, C ABI) and shared-library loading | `crates/neptune-vm/src/{plugin,dynlib}.rs`; SDK in `sdk/` (C header, `neptune-plugin` crate, examples) |
| CI (Windows, Linux, macOS) | `.github/workflows/ci.yml` |
| Packaging (`neptune build`) | `crates/neptune-cli/src/package.rs` |
| Language server (`neptune lsp`) | `crates/neptune-cli/src/lsp.rs` (+ `json.rs`) |
| Debugger | VM side `crates/neptune-vm/src/debug.rs`; protocol (`neptune dap`) `crates/neptune-cli/src/dap.rs` |
| Primitive shapes | `crates/neptune-runtime/src/primitive.rs` |
| Render scale (`RenderScale`): the first scene draws into a smaller texture that the 2D layer stretches over the window | runtime `renderscale.rs`, `frame.shader` |
| Stored entities (`StoreEntity`): out of the scene graph, so `ent()` fails and `entities()` skips them | runtime `store.rs` |
| VS Code extension | `editors/vscode` (regenerate grammar with `python tools/gen_grammar.py`) |
| IDE (egui, talks to `neptune lsp`/`dap`, never links Fyrox) | `crates/neptune-ide/src`: `editor.rs` (widget), `buffer.rs` (text, carets, undo), `lang.rs` (tokens, blocks), `app.rs` (state, actions), `view.rs`/`panels.rs`/`dialogs.rs` (drawing), `actions.rs` (menu and key table); guide `docs/IDE.md` |
| Level editor (Fyrox's `fyroxed_base` as a library; its own program, so `neptune` and the IDE never link it) | `crates/neptune-editor/src`: `main.rs` (start-up, window title, `--play`), `theme.rs` (the IDE's colours as Fyrox style brushes; keep in step with the IDE's `theme.rs`), `play.rs` (Play's build profiles, which call `neptune check` and `neptune run`), `preview.nb` (the "Look Around" program); guide `docs/EDITOR.md` |
| Language tests | `crates/neptune-vm/tests/language.rs` |
| What's done / to do | `docs/STATUS.md` |
| Command reference (generated) | `docs/COMMANDS.md` from `docs/commands.txt` (`tools/gen_reference.py`) |
| What changed, commit by commit | `CHANGELOG.md` |
| Design | `docs/ARCHITECTURE.md`, `docs/LANGUAGE.md` |

## Saving context

- `sema.rs`, `parser.rs` and runtime `commands.rs` are 1,200–2,000 lines. Grep for the symbol and read only that range; never read them whole.
- For Fyrox APIs, grep the source in `~/.cargo/registry/src/*/fyrox-*-1.0.*` instead of guessing or fetching docs.
- Blitz3D reference source, if checked out, is in `reference/` (git-ignored). `bbruntime/` there has the original command implementations.
- `cargo test -p neptune-vm` runs the language tests without building Fyrox. Use it for compiler/VM work.

## Hard rules

- Everything must work on Windows, Linux and macOS. No platform-only features: when OS code is needed, write it for all three (see `dynlib.rs`, `screenshot.rs`, `report` in the CLI).

- `neptune-lang` and `neptune-vm` must never depend on Fyrox or `neptune-runtime`.
- Match Blitz3D command names, argument order, defaults and return values exactly. Commands Blitz3D never had are extensions and are marked `(ext)` in comments and docs.
- Don't reformat or rewrite unrelated code. Errors are `Result<Value, String>` with short messages like `"Entity does not exist"`.
- Ask before adding a crate dependency.
- When a feature is finished, update `docs/STATUS.md` (and `docs/LANGUAGE.md` if users see new syntax).
- Every command is documented: add a `Name: description` line to `docs/commands.txt` (in the right `# Category`, starting with `(ext)` for extensions), then run `python tools/gen_reference.py` to regenerate `docs/COMMANDS.md`. The test `every_command_is_documented` (neptune-cli) fails otherwise. Hover help in editors uses the same text.
- Add a plain-language entry for each commit's change to the top of `CHANGELOG.md`.

## Runtime commands

Commands are entries in `COMMANDS` in `crates/neptune-runtime/src/commands.rs`, written in Blitz3D `rtSym` signature syntax:

```rust
("%CreateSphere%segments=8%parent=0", |h, a, _| create(h, "sphere", a[0].int(), true, a[1].int())),
```

- A leading tag is the return type; no tag = no return value. Parameter tags: `%` int, `#` float, `$` string, `?` any. `=value` sets a default.
- The closure is `Cmd`: `(&mut Host3D, &[Value], &mut NativeCtx) -> Result<Value, String>`. Return `VOID`, `int(..)` or `float(..)`. Read arguments with `a[i].int()/.float()/.str()`; use the `v3` and `color` helpers.
- Keep table entries to a line or two. Logic goes in a function, and a whole feature goes in its own module.
- Table order sets native ids; put new commands next to related ones.
- Modern names go in `ALIASES`, not as duplicate commands. Named constants go in `constants()`; key codes go in `input.rs`.
- Use `h.ent(e)?` for handle lookup (it gives the error for a bad handle). `h.scene()?` fails before `Graphics3D`. Lights are a pivot with a child light node (`light_of`).

## Coordinates (easy to get wrong)

- Programs see **Blitz space**: left-handed, +X right, degrees, positive pitch looks down, positive yaw turns left.
- Fyrox renders +X to the left: `R_f = S R_b S`, `S = diag(-1,1,1)`. Anything touching positions, rotations, directions, normals or picks must go through `transform.rs`. Never pass raw values to Fyrox.
- The mirror flips triangle winding (`mesh.rs` swaps the last two corners on upload). Call `h.flush_meshes()` before anything that reads GPU geometry.
- 2D uses logical (DPI-independent) pixels. Program colours are sRGB 0–255; clear colours go through `srgb_to_linear`.
- Entities use Fyrox PBR by default and switch to the `classic.rs` shader for blend modes, `EntityFX`, shininess, multiple texture layers, texture transforms or fog. Use that switch; don't make a new material path.

## Compiler

- A language feature usually touches lexer → parser → ast → sema → ir → codegen in that order. Follow the closest existing feature.
- Sema inserts Blitz conversions explicitly; bytecode is typed (`AddI`/`AddF`); only `Variant` dispatches at runtime.
- `Vm::run` is resumable (yields at `RenderWorld`/`Flip`/`Graphics3D`). Never block.
- Keep Blitz 32-bit semantics: float → int rounds half to even.

## Testing and checks

- Language/stdlib changes: add `assert_eq!(ok("...source..."), "expected output")` cases to `crates/neptune-vm/tests/language.rs`.
- Pure runtime logic: `#[cfg(test)] mod tests` in the same file.
- Engine-only behaviour: an example in `examples/*.nb`, checked with `neptune check`.

Before calling a task done:

```
cargo build
cargo test
cargo clippy --all-targets
cargo run -p neptune-cli -- check examples/<file>.nb
```
