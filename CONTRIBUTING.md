# Working on NeptuneBASIC

This is for people changing NeptuneBASIC itself. To make games with it, see
the [README](README.md) and [the manual](docs/MANUAL.md).

## How it fits together

A `.nb` program goes through four crates:

```
neptune-lang (compiler) -> neptune-vm (bytecode VM) -> neptune-runtime (Fyrox) -> neptune-cli (the neptune program)
```

| Folder | What is in it |
|---|---|
| `crates/neptune-lang` | lexer, parser, type checking, IR, bytecode and code generation |
| `crates/neptune-vm` | the VM, values, the console/maths/string/file commands, networking, plugins |
| `crates/neptune-runtime` | 2D, 3D, sound, input, physics and GUI commands on top of Fyrox |
| `crates/neptune-cli` | the `neptune` program: run, check, build, kit, lsp, dap, bench |
| `crates/neptune-ide` | Neptune IDE (egui); talks to `neptune lsp` and `neptune dap`, never links Fyrox |
| `crates/neptune-editor` | Neptune Level Editor, built on our copy of Fyrox's editor in `vendor/fyroxed_base` |
| `editors/vscode` | the VS Code extension |
| `sdk` | the plugin SDK: C header, Rust crate and examples |
| `tools` | scripts for maintainers (see below) and the Blender add-on |
| `examples` | example programs and their media; shipped in the download |
| `benchmarks` | programs `neptune bench` runs |
| `tests` | programs the CI packages and runs |
| `docs` | the user guides; `docs/dev` has the design notes, status and benchmark results |

Read [docs/dev/ARCHITECTURE.md](docs/dev/ARCHITECTURE.md) for the design and
[docs/dev/STATUS.md](docs/dev/STATUS.md) for what is done and what is left.

## Rules

* **All three systems.** Everything works on Windows, Linux and macOS. When
  OS code is needed, write it for all three.
* **Layers.** `neptune-lang` and `neptune-vm` never depend on Fyrox or
  `neptune-runtime`.
* **Blitz3D compatibility.** Commands that Blitz3D had keep its names,
  argument order, defaults and return values. New commands are extensions,
  marked `(ext)` in comments and docs.
* **Coordinates.** Programs see Blitz space (left-handed, +X right, degrees).
  Fyrox draws +X to the left, so anything with positions, rotations,
  directions or normals goes through `crates/neptune-runtime/src/transform.rs`.
* **Errors** are `Result<Value, String>` with short messages such as
  `"Entity does not exist"`.
* **Dependencies.** Discuss a new crate before adding it.
* **Small diffs.** Don't reformat code you aren't changing.

## Adding a command

Runtime commands are entries in `COMMANDS` in
`crates/neptune-runtime/src/commands.rs` (console, maths and string commands
are in `crates/neptune-vm/src/stdlib.rs`), written in Blitz3D's signature
syntax:

```rust
("%CreateSphere%segments=8%parent=0", |h, a, _| create(h, "sphere", a[0].int(), true, a[1].int())),
```

A leading tag is the return type. Parameter tags are `%` int, `#` float,
`$` string and `?` any, and `=value` sets a default. Keep the table entry to
a line or two and put the logic in a function, or a whole feature in its own
module. Modern names go in `ALIASES`, constants in `constants()`.

Every command is documented. Add a `Name: description` line to
`docs/commands.txt` under the right `# Category` (starting with `(ext)` for
extensions), then regenerate the reference:

```bash
python tools/gen_reference.py
```

The test `every_command_is_documented` fails until you do. If you added
keywords or commands, regenerate the VS Code grammar too:

```bash
python tools/gen_grammar.py
```

## Tests and checks

Before sending a change:

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo run -p neptune-cli -- check examples/<file>.nb
```

`cargo test -p neptune-vm` runs the language tests without building Fyrox,
which is quicker for compiler and VM work.

* Language and standard-library changes: add cases to
  `crates/neptune-vm/tests/language.rs`.
* Runtime logic that doesn't need the engine: a `#[cfg(test)]` module in the
  same file.
* Engine behaviour: an example in `examples/`, checked with `neptune check`.
* `python tools/check_doc_examples.py` checks the code examples in the docs.

When a feature is finished, update `docs/dev/STATUS.md` (and
`docs/LANGUAGE.md` if there is new syntax), and add a plain-language entry at
the top of `CHANGELOG.md`.

## Maintainer scripts

| Script | Does |
|---|---|
| `tools/gen_reference.py` | writes `docs/COMMANDS.md` from `docs/commands.txt` and `docs/examples/*.txt` |
| `tools/gen_grammar.py` | writes the VS Code syntax grammar |
| `tools/check_doc_examples.py` | compiles (and with `--gpu`, runs) the examples in the docs |
| `tools/list_commands.py` | lists every command with its signature |
| `tools/gen_big_program.py` | writes a very large program to stress the compiler |
| `tools/make_sample_2d.py` | writes the sample 2D art in `examples/media/tiled` |
| `tools/blender/` | the Blender add-on, and a script that makes the sample level |

## Making the download

The download is one folder per system with `neptune`, `neptune-ide`,
`START.txt`, `cube.nb`, `examples`, the user guides from `docs` and the
Blender add-on. Nothing in it needs Rust or Python.

```bash
cargo build --release -p neptune-cli -p neptune-ide
target/release/neptune kit
```

That writes `dist/NeptuneBASIC-windows` (or `-linux`, `-macos`). Zip it.

The Kit workflow (`.github/workflows/kit.yml`) builds the folder on all three
systems. Run it by hand and download the archives from the run's page, or
push a version tag to put them on a GitHub release:

```bash
git tag v0.2.0
git push origin v0.2.0
```
