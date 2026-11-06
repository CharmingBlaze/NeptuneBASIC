# Changelog

What changed, commit by commit, in plain language. The newest work is at
the top. Every command mentioned here is explained in
[docs/COMMANDS.md](docs/COMMANDS.md).

## Asset management

* **Caching, more background loading and live reloading** – Loading a
  texture, image or sound file again shares what was decoded the first
  time (each load still gets its own handle and settings, as in Blitz3D),
  and a file changed on disk is read again. `LoadImageAsync` and
  `LoadSoundAsync` complete background loading. `ReloadAssets` and
  `WatchAssets` bring in art changed while the game runs, keeping every
  handle's settings; `ImageName`, `SoundName` and `ModelName` tell where an
  asset came from.

## GUI widgets

* **Fyrox's retained UI for BASIC programs** – 72 `Gui*` commands: windows,
  layout panels, grids and scroll areas, buttons, text, text boxes, check
  boxes, sliders, progress bars, images, drop-downs, lists, trees, tabs,
  menus, popups, message boxes, file browsers and dialogs, docking, number
  boxes, Vector3 editors, colour pickers and fields and curve editors, with
  click and change events, styling and themes. Sizes are logical pixels, so
  the GUI keeps its size on high-DPI screens. `examples/gui.nb` is a small
  settings app over a 3D scene. Also: the debug build no longer exceeds the
  Windows linker's debug-database limit (dependencies are built without
  debug info).

## Animation states, blend spaces, layers and tracks

* **Animation layers, blend spaces, state machines, root motion and
  property tracks** – Several animations can play on one model at once,
  mixed by weight or limited to part of the skeleton (`AnimLayer`: run with
  the legs, aim with the upper body). Blend spaces mix sequences by
  parameters such as speed, keeping them in step (`AnimBlend`). State
  machines switch between named states by themselves when parameter
  conditions hold or a one-shot animation ends, with crossfades
  (`AddAnimState`, `AddAnimTransition`, `SetAnimParam`). Root motion lets
  an animation's own movement move the entity. Property tracks keyframe
  colour, alpha, emission, light colour and range, camera zoom, fog,
  texture offsets and transforms on any entity (`CreateTrack`, `TrackKey`,
  `PlayTrack`). `examples/animstates.nb` drives a fox with all of it.

## Command reference and documentation

* **Every command documented** – `docs/COMMANDS.md` explains all commands
  (what they do, their parameters and what they return), grouped by area.
  The same text appears when you hover over a command in VS Code. A test
  fails if a command has no description, so new commands are always
  documented.

## Physics: motors, limits and ragdolls

* **8ce01d2 Joint motors, limits and ragdolls** – Hinges and sliders can now
  be driven: `JointMotor` turns or slides at a speed (a windmill, a wheel),
  `JointTarget` pulls to an angle or position like a spring (a door, a
  lift), `JointLimits` keeps them in a range, and `JointAngle` /
  `JointPosition` read them back. All of it works in 3D and in 2D
  (`JointMotor2D`, ...), and 3D gained a slider joint (`CreateSliderJoint`).
  `CreateRagdoll` gives an animated model physics limbs along its skeleton;
  `RagdollEnable` makes it go limp and fall, and turning it off hands the
  bones back to the animation. Two older bugs were fixed on the way:
  `EntityX/Y/Z` and `EntityPitch/Yaw/Roll` returned the starting values for
  physics bodies, and a hinge about the X axis got stuck.

## 2D games

* **eec19c4 2D camera, 2D physics, 2D animation and UI widgets** –
  `Camera2D` scrolls, zooms and rotates everything drawn in 2D; the HUD can
  switch back to screen pixels. 2D physics (Fyrox's Rapier 2D) works in
  pixels: bodies with box, circle, capsule, polygon and line shapes, a
  tile map turned into collision with one command, forces, contacts, ray
  casts and hinge, fixed and slider joints. `CreateAnim2D` plays frame
  sequences, `DrawImageEx` draws rotated and scaled images, and immediate
  UI widgets (buttons, check boxes, sliders, text boxes, panels) need one
  line each. Also fixed: `Cls` left the background black in 2D-only
  programs. `examples/platformer2d.nb` uses all of it.

## Modern rendering

* **caeb2c3 Sky boxes, PBR, decals, particles, LOD, camera effects, custom
  shaders** – Physically based materials (`EntityMetallic`,
  `EntityRoughness`, glowing `EntityEmissive`, metallic/roughness/AO/
  lightmap maps and parallax height maps), `SkyBox`, decals, soft particle
  emitters, distance-based level of detail, fixed or automatic exposure,
  colour-grading lookup tables, cameras that draw into textures (security
  monitors), custom Fyrox shaders with extra per-vertex data, and DDS and
  compressed textures. `examples/rendering.nb` shows them together.

## Loading, saving and tile maps

* **2fdf6b4 Background loading, saved scenes and 2D tile maps** –
  `LoadMeshAsync` and `LoadTextureAsync` return at once so a loading screen
  can show progress. `SaveScene` / `SaveEntity` write a Fyrox scene file
  that `LoadScene` loads back (models are saved by file name, so animated
  characters come back intact). Tile maps draw only what is on screen, use
  16- or 47-frame autotiling, and save or load their own format or CSV
  from the Tiled editor. `LoadMesh` no longer crashes on files outside the
  game folder.

## Completing Blitz3D

* **e535e81 Last input commands** – `MouseWait`, `JoyWait`,
  `EnableDirectInput` and `DirectInputEnabled`, the last Blitz3D names.
* **17a508f Movies** – `OpenMovie`, `DrawMovie` and friends play animated
  GIFs, Motion-JPEG AVI files and folders of numbered images on every
  platform (Blitz3D used Windows-only DirectShow). Also fixed a false
  "variable is never assigned" warning.
* **5c5dcf5 Multiplayer games** – Blitz3D's net game commands
  (`HostNetGame`, `JoinNetGame`, `SendNetMsg`, `RecvNetMsg`, ...) over TCP,
  with games found automatically on the local network (Blitz3D used
  DirectPlay).
* **52f6ec0 Overlay cameras** – `CameraClsMode` cameras that do not clear
  are drawn over the cameras before them, like in Blitz3D.
* **0bcbf3e Plugins and three platforms** – Native plugins (`.dll`, `.so`,
  `.dylib`) add commands to the language, with a C header, a Rust crate and
  examples. `CallDLL` and error dialogs work on Windows, Linux and macOS,
  and CI builds and tests on all three.
* **eab9501 Screenshots everywhere** – `SaveBuffer` of the screen reads the
  game's own frame, on every platform.
* **4ecd77e Language audit** – A test program checks the language behaves
  like Blitz3D line by line.
* **2ec1755 Gamma and the last 3D commands** – `SetGamma`/`UpdateGamma`,
  `LoadAnimSeq`, `LoaderMatrix` and the remaining Blitz3D commands; mirrors
  show their reflection in the same frame.
* **4419a9e, 285e968 Sample games** – Star Courier (a 2D shooter) and Orb
  Run (a 3D fox runner), both with an attract mode, and docs for full
  Blitz3D coverage.
* **dfa4851 Cube maps and mirrors** – Reflection and refraction with cube
  map textures, and `CreateMirror`.
* **da470f5 Image transforms and libraries** – `TFormImage`,
  `TFormFilter`, `CallDLL`, `PlayCDTrack` (accepted; there are no CDs).
* **664d130 Maths, entity and keyframe commands** – `TFormPoint` and
  friends, `AlignToVector`, `CaptureWorld` tweening, `EntityAutoFade`,
  animated textures, and keyframe animation of any entity.
* **eb34e88 Gamepads, fullscreen and on-screen Print** – Controllers
  through gilrs, borderless fullscreen, pointer control, `WaitKey` that
  does not freeze the window, `Print` on the screen in graphics mode.
* **3b1a823 Standard library** – Timers, banks, folders, file seeking,
  TCP and UDP networking and system commands.

## Tools

* **351d8b3 Language server** – `neptune lsp`: errors as you type,
  completion, hover, signature help, outline and go to definition; used by
  the VS Code extension and usable from other editors.
* **563d420, c2cdb27 Packaging** – `neptune build` makes a stand-alone game
  folder (no console window on Windows).
* **d954c55 VS Code extension** – Highlighting, run (F5) and build.
* **fffa98b, 22173e2 Development helpers** – A project map and an automatic
  build check for AI-assisted work.

## Building the engine

* **24317d5 Physics on copies** – Parented and copied entities get their
  own physics bodies; sprite and terrain copies are independent.
* **4cafbd9 Text into images; primitive textures** – `Text` draws into
  image and texture buffers; built-in shapes have Blitz3D's texture layout.
* **7268b8b Animation blending** – `Animate` blends between sequences.
* **0b6b89e, 530667b, d4795cb Documentation** – Status, language and
  setup docs; glTF/FBX chosen over the old Blitz model formats.
* **0ac3485 Buffers and pixels** – Drawing into images and textures,
  `ReadPixel`/`WritePixel`, `LockBuffer` and more image commands.
* **c2cdb27 Packaging** – see Tools.
* **7ec868f Sprites and terrain** – Camera-facing sprites and heightmap
  terrain.
* **8630f03 Graphics quality** – Presets from old integrated GPUs to
  high-end cards, and post effects (bloom, SSAO, FXAA, shadows, light
  scattering).
* **acbe4b8 Physics** – Rigid bodies on Fyrox's Rapier engine: shapes,
  forces, contacts, ray casts and joints.
* **a7f2e8f Sound** – Sounds, channels, streamed music and 3D sound.
* **b2ae7d7 Model animation** – Playing glTF animations, `FindChild` and
  access to a model's parts.
* **79f17e0 Sample models** – The animated Fox and CesiumMan from the
  Khronos samples.
* **b373eb5 Custom meshes** – Building meshes vertex by vertex, and brushes.
* **701dab3 Classic materials** – Blitz3D's blend modes, effects, fog and
  texture layers, orthographic cameras, and modern command names.
* **9f2dfe5 2D drawing and picking** – Shapes, images, pixel-perfect
  collisions, and picking entities with the mouse or rays.
* **4676a26 Collisions** – Blitz3D's sliding collision system.
* **91d6d2c Models** – Loading glTF and FBX models, `CopyEntity`.
* **105cb28 Textures** – Loading and applying textures, masking, normal
  maps.
* **b5d4203 2D text** – `Text`, `Color`, `Cls` and fonts in DPI-independent
  pixels.
* **6ac78bd The start** – The compiler (lexer, parser, type checking with
  inferred types, bytecode), the virtual machine and the first 3D commands
  on Fyrox.
