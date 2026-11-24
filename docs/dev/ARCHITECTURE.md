# NeptuneBASIC architecture

NeptuneBASIC is a modern BASIC for 3D games. The language keeps Blitz3D's
command-driven style and syntax; the engine underneath is
[Fyrox](https://fyrox.rs) 1.0, and every Fyrox technology is meant to be
reachable from BASIC commands.

```
          source (.nb)
               │
   ┌───────────▼────────────┐   neptune-lang  (no engine dependency)
   │ lexer → parser → AST   │
   │ sema (resolve, infer,  │
   │       type check) → IR │
   │ codegen → bytecode     │
   └───────────┬────────────┘
               │ Program (typed bytecode + debug spans)
   ┌───────────▼────────────┐   neptune-vm   (no engine dependency)
   │ resumable stack VM     │
   │ object heap (Types)    │
   │ standard library       │
   └───────────┬────────────┘
               │ Host trait  (native command ids)
   ┌───────────▼────────────┐   neptune-runtime
   │ 3D command table       │
   │ Blitz-space transforms │
   │ input                  │
   │ Fyrox plugin / window  │──► Fyrox scene graph, renderer, (physics, sound, ...)
   └───────────┬────────────┘
               │
          neptune-cli  (`neptune run/check/disasm/commands`)
```

## Crates

| crate | role |
|---|---|
| `neptune-lang` | Lexer, parser, AST, semantic analysis & type inference, typed IR, bytecode generation, diagnostics with source spans. Independent of any engine. |
| `neptune-vm` | Bytecode interpreter, object heap, standard library (console, math, strings, files, time, random). Independent of any engine. |
| `neptune-runtime` | 3D/input commands implemented on Fyrox; owns the window and frame loop. |
| `neptune-cli` | The `neptune` executable. |

## Compiler pipeline

1. **Lexer** (`lexer.rs`) – Blitz3D tokenizer rules (case-insensitive, `;`
   comments, `$FF`/`%101` literals, two-word keywords like `End If`) plus `'`
   comments, `_` line continuation, `$"..."` interpolated strings, `+=` etc.
2. **Parser** (`parser.rs`) – recursive descent, a port of Blitz3D's
   `parser.cpp` grammar and operator precedence (including optional
   parentheses around command arguments and `Dim`-before-use arrays) with
   NeptuneBASIC extensions. Error recovery is per line.
3. **AST** (`ast.rs`).
4. **Semantic analysis** (`sema.rs`) – name resolution (locals, globals,
   constants, enums, arrays, functions, methods, commands), control-flow
   validation, constant folding.
5. **Type checking & inference** (`sema.rs`) – every expression gets a static
   type; implicit Blitz conversions are inserted explicitly. Untagged
   variables and function results are *inferred* by a whole-program
   fixpoint (Int + Float widens to Float). `Classic` restores Blitz's
   "untagged means Int" rule. `Variant` is the opt-in dynamic type.
6. **Intermediate representation** (`ir.rs`) – a typed tree with resolved
   slots. Backends consume only this form; a native backend (e.g. Cranelift)
   can be added beside the bytecode generator without touching earlier stages.
7. **Code generation** (`codegen.rs`) – typed stack bytecode (`AddI`/`AddF`,
   `CmpS`, ...), so the VM does no type dispatch on hot paths except for
   `Variant` operations. Its last pass (`fuse.rs`) joins short runs of
   instructions into superinstructions: the test of an `If`, `While` or `For`
   becomes one instruction (`JumpCmpILK`: jump if a local compares with a
   constant), and so do arithmetic with a local or a constant, a field of an
   object in a local, and an array indexed by a local. A run is never fused
   across a jump target or the start of a statement, so jumps and the
   debugger's stops are unaffected.
8. **Runtime integration** – native commands are declared by hosts with
   Blitz3D's own `rtSym` signature syntax (`"%CreateCube%parent=0"`) and
   passed to the compiler in an `Env`; the VM dispatches them by id through
   the `Host` trait.

## VM design

* **Resumable**: `Vm::run` returns `Yielded` when a command requests it
  (`RenderWorld`, `Flip`, `Graphics3D`). The engine calls `run` once per
  frame from its own event loop, so a classic `While ... RenderWorld ... Wend`
  program needs no threads and never blocks the window.
* **Values**: `Int(i32)`, `Float(f32)`, `Str`, `Obj`, `Null`, `Vector`.
  Strings are shared. `s = s + x` keeps the left-hand side and copies only
  its last block (the whole string, while the result is still short).
  32-bit numbers match Blitz3D semantics (hex literals, shifts, float → int
  rounding to nearest-even).
* **Objects**: Blitz `Type` objects live in a slot heap with per-type
  ordered lists (`First`, `Last`, `After`, `Before`, `Insert`, `Each`).
  References carry a generation, so references to deleted objects behave
  as `Null` instead of dangling – no use-after-free, no reference cycles.
* **Errors**: runtime errors carry the source location from a per-function
  pc → span table and can be caught with `Try`/`Catch`.
* **Speed** (all safe Rust; measure with `cargo run --release -p neptune-vm
  --example langbench`):
  * The value stack is allocated ahead and its top (`sp`) is a local of the
    interpreter loop, as are `pc` and the frame base, so they live in
    registers.
  * Numbers are read and written in place as 4-byte numbers, not moved as
    whole `Value`s. A whole-value read of a slot that was just written in
    part makes the processor wait for the write (a store-forwarding stall);
    this was the largest single cost.
  * Rarely used instructions run in a separate function (`exec_cold`), which
    keeps the main loop small enough for the compiler to keep its variables
    in registers. An instruction that games run every frame belongs in the
    main loop.
  * `Dim` arrays of integers and floats store plain numbers (`ArrayData`).

## Runtime design

* Each entity handle indexes an entity table; every entity owns a Fyrox
  node. Lights are a pivot plus a child light node.
* Transforms are kept in **Blitz space** (left-handed, +X right, degrees,
  `RotateEntity` = yaw·pitch·roll, positive pitch looks down). Fyrox renders
  +X to the left, so transforms are mirrored on the way into the engine with
  `R_f = S R_b S`, `S = diag(-1,1,1)` – a homomorphism, so parent/child
  composition in Fyrox's graph equals composition in Blitz space.
* Program flow: console phase (no window) → `Graphics3D` yields → the CLI
  opens a Fyrox `Executor` window with a `NeptunePlugin` that owns the VM →
  each engine update runs the VM until the next frame boundary.

## Roadmap (phases)

See [STATUS.md](STATUS.md) for what is implemented and tested today.
