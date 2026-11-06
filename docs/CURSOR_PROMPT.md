# Prompt for Cursor AI: finish NeptuneBASIC

Paste everything below the line into Cursor (Agent mode). Work through one
task at a time; start a new chat per task if context gets long.

---

You are working on **NeptuneBASIC**, a modern BASIC for 3D games written in
Rust. It keeps Blitz3D's command-driven syntax and runs on the **Fyrox 1.0**
engine. Your job is to finish the remaining items in `docs/STATUS.md`, one at
a time, with the same quality bar as the existing code.

## Read first (do not skip)

1. `docs/STATUS.md`: what is done, partial, and not started. This is the task list.
2. `docs/ARCHITECTURE.md`: crate layout, compiler pipeline, VM, runtime design.
3. `docs/LANGUAGE.md`: the language as users see it.
4. `crates/neptune-runtime/src/commands.rs`: how runtime commands are declared.
5. `reference/blitz3d/` (original Blitz3D C++ source; `bbruntime/` holds the
   command implementations) and `reference/blitz3d-ng/`. Use them to match
   Blitz3D's exact command names, parameter order, defaults and behaviour.

## How the project is built

* Crates: `neptune-lang` (lexer → parser → sema → IR → bytecode, no engine
  dependency), `neptune-vm` (resumable stack VM + stdlib, no engine
  dependency), `neptune-runtime` (Fyrox-backed 3D/2D/input commands),
  `neptune-cli` (the `neptune` binary: `run`, `check`, `disasm`, `commands`).
* Runtime commands are table entries in `commands.rs` using Blitz3D `rtSym`
  signature syntax, e.g.
  `("%CreateSphere%segments=8%parent=0", |h, a, _| ...)`.
  Leading tag = return type; `%` int, `#` float, `$` string, `?` any;
  `=value` gives a default. Put non-trivial logic in its own module
  (like `collision.rs`, `texture.rs`, `model.rs`, `image2d.rs`) and keep the
  table entry short.
* Transforms live in **Blitz space** (left-handed, +X right, degrees, positive
  pitch looks down). Fyrox renders +X to the left, so values are mirrored with
  `R_f = S R_b S`, `S = diag(-1,1,1)`. Any new command that touches positions,
  directions, normals or picks must go through the helpers in `transform.rs`.
* 2D coordinates are **logical pixels** (DPI-independent), same units as
  `GraphicsWidth`, `MouseX` and `Text`.
* Extensions that Blitz3D never had are marked `(ext)` in docs.

## Rules

* Match Blitz3D names, argument order, defaults and return values exactly
  unless STATUS.md says a command is an extension. When Blitz3D behaviour is
  odd, copy it and leave a short comment saying so.
* Keep `neptune-lang` and `neptune-vm` free of any Fyrox dependency.
* Do not rewrite or reformat unrelated code. Follow existing style: small
  helpers, `Result<Value, String>` errors with clear messages
  (e.g. "Entity does not exist"), doc comment at the top of each module.
* Every pure-logic piece (maths, parsing, pixel ops, collision geometry,
  mesh building) gets `#[test]` unit tests in the same file. Engine-only
  behaviour gets an example program in `examples/` that you describe how to
  check by hand.
* After each task run, and fix everything they report:
  ```
  cargo build
  cargo test
  cargo clippy --all-targets
  cargo run -p neptune-cli -- check examples/<your example>.nb
  ```
* When a task is finished, update `docs/STATUS.md` (status column, command
  list, examples table) and, if users see new syntax, `docs/LANGUAGE.md`.
* One task = one focused commit with a clear message. Do not start the next
  task until the current one builds, passes tests and is documented.
* If something in Fyrox 1.0 is missing or the API is unclear, read the Fyrox
  source in `~/.cargo/registry` rather than guessing, and say what you found.
  Stop and ask me before adding a new crate dependency.

## Tasks, in order

### 0. Finish 2D drawing (work in progress, uncommitted)
`draw2d.rs`, `image2d.rs` and the 2D entries in `commands.rs` are partly
written (`Rect`, `Oval`, `LoadImage`, `DrawImage`, `DrawImageRect`,
`DrawBlockRect`, ...). Read the current diff (`git diff`, plus untracked
`image2d.rs`) before changing anything. Complete and test:
`Line`, `Plot`, `Rect`, `Oval`, `Color`/`ClsColor` interaction,
`LoadImage`, `LoadAnimImage`, `CreateImage`, `FreeImage`, `CopyImage`,
`DrawImage`, `DrawBlock`, `DrawImageRect`, `DrawBlockRect`, `TileImage`,
`TileBlock`, `MaskImage`, `HandleImage`, `MidHandle`, `AutoMidHandle`,
`ImageXHandle/YHandle`, `ImageWidth/Height`, `ScaleImage`, `ResizeImage`,
`RotateImage` (optional), `ImagesOverlap`, `ImagesCollide`, `ImageRectOverlap`,
`ImageRectCollide`, `RectsOverlap`. Add `examples/draw2d.nb`.

### 1. Cameras: `CameraProjMode` (0 off, 1 perspective, 2 orthographic), `CameraFogMode/Color/Range`, `CameraProject` + `ProjectedX/Y/Z`.

### 2. Materials: `EntityShininess`, `EntityBlend` (1 alpha, 2 multiply, 3 add), `EntityFX` (1 fullbright, 2 vertex colour, 4 flat, 8 no fog, 16 two-sided, 32 force alpha), `EntityOrder`.

### 3. Textures: `PositionTexture`, `RotateTexture`, texture layers / multitexturing (`EntityTexture` index), `TextureBlend`, `CreateTexture`, `TextureWidth/Height`, `FreeTexture`, `TextureCoords`. Pixel access (`LockBuffer`, `ReadPixel(Fast)`, `WritePixel(Fast)`, `TextureBuffer`, `SetBuffer`) can share code with 2D images.

### 4. Picking: `EntityPickMode`, `LinePick`, `EntityPick`, `CameraPick`, `PickedX/Y/Z`, `PickedNX/NY/NZ`, `PickedEntity`, `PickedTime`, `PickedSurface`, `PickedTriangle`, `EntityVisible`. Reuse collision geometry from `collision.rs` where possible.

### 5. Custom meshes: `CreateMesh`, `CreateSurface`, `AddVertex`, `AddTriangle`, `VertexCoords`, `VertexNormal`, `VertexColor`, `VertexTexCoords`, `VertexX/Y/Z`, `CountSurfaces`, `GetSurface`, `CountVertices`, `CountTriangles`, `TriangleVertex`, `UpdateNormals`, `FlipMesh`, `ScaleMesh`, `PositionMesh`, `RotateMesh`, `FitMesh`, `AddMesh`, `PaintSurface`/`CreateBrush` family. Remember the Blitz → Fyrox X mirror flips triangle winding.

### 6. Models: animation playback (`Animate`, `SetAnimTime`, `AnimTime`, `AnimLength`, `AnimSeq`, `Animating`, `ExtractAnimSeq`), `FindChild`, `CountChildren`, `GetChild`. Optionally a `.b3d` loader (spec is in the reference sources).

### 7. Sound (Fyrox sound engine): `LoadSound`, `PlaySound`, `LoopSound`, `StopChannel`, `PauseChannel`, `ResumeChannel`, `ChannelPlaying`, `SoundVolume/Pan/Pitch`, `ChannelVolume/Pan/Pitch`, `FreeSound`, `PlayMusic`, and 3D sound: `CreateListener`, `Load3DSound`, `EmitSound`.

### 8. Physics (Rapier via Fyrox), all `(ext)`: rigid bodies, colliders, gravity, forces/impulses, joints, ray casts. Design the command names first, write them into `docs/LANGUAGE.md`, and show me the list before implementing.

### 9. Terrain, water, fog, post-processing: `CreateTerrain`, `LoadTerrain`, `TerrainHeight`, `ModifyTerrain`, `TerrainY`, `TerrainDetail`; `AmbientLight`; fog via camera; post-processing (bloom, tone mapping, SSAO) as `(ext)`. Also sprites: `CreateSprite`, `LoadSprite`, `SpriteViewMode`, `HandleSprite`, `RotateSprite`.

### 10. Tooling
* Packaging: `neptune build game.nb -o dist/` producing a stand-alone
  executable with the compiled bytecode and the media it references.
* Editor support: a TextMate grammar and a minimal VS Code extension
  (syntax highlighting, `neptune check` diagnostics on save). A language
  server can come later.

## How to report back after each task

Give me: the commands added (with signatures), files changed, test results
(paste the `cargo test` summary), how to try the new example, and anything
that still differs from Blitz3D or that you were unsure about.

Start with task 0. First summarise what the current uncommitted 2D code
already does and what is missing, then implement.
