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
   `Variant` operations. Includes a fused `IncLocalI` for loop counters.
8. **Runtime integration** – native commands are declared by hosts with
   Blitz3D's own `rtSym` signature syntax (`"%CreateCube%parent=0"`) and
   passed to the compiler in an `Env`; the VM dispatches them by id through
   the `Host` trait.

## VM design

* **Resumable**: `Vm::run` returns `Yielded` when a command requests it
  (`RenderWorld`, `Flip`, `Graphics3D`). The engine calls `run` once per
  frame from its own event loop, so a classic `While ... RenderWorld ... Wend`
  program needs no threads and never blocks the window.
* **Values**: `Int(i32)`, `Float(f32)`, `Str(Rc<str>)`, `Obj`, `Null`,
  `Vector`. 32-bit numbers match Blitz3D semantics (hex literals, shifts,
  float → int rounding to nearest-even).
* **Objects**: Blitz `Type` objects live in a slot heap with per-type
  ordered lists (`First`, `Last`, `After`, `Before`, `Insert`, `Each`).
  References carry a generation, so references to deleted objects behave
  as `Null` instead of dangling – no use-after-free, no reference cycles.
* **Errors**: runtime errors carry the source location from a per-function
  pc → span table and can be caught with `Try`/`Catch`.

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
