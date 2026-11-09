# NeptuneBASIC

A BASIC for making 2D and 3D games: write a few lines, run them, and a
window opens with your game in it. A simple language that is easy to
learn, over a thousand commands for graphics, sound, input, physics and
more, and the [Fyrox](https://fyrox.rs) engine underneath. Written in Rust.

New to it? Start with [the manual](docs/MANUAL.md), which teaches the
language from the beginning.

```basic
Graphics3D 1280, 720

camera = CreateCamera()
PositionEntity camera, 0, 5, -10
CameraLookAt camera, 0, 0, 0

light = CreateLight(LIGHT_DIRECTIONAL)

cube = CreateCube()
PositionEntity cube, 0, 1, 0
EntityColor cube, 255, 0, 0

While Not KeyDown(KEY_ESCAPE)
    If KeyDown(KEY_W) Then MoveEntity cube, 0, 0, 0.1
    UpdateWorld()
    RenderWorld()
Wend
End
```

## Why NeptuneBASIC

* **Easy to start** – a short program opens a window, a game loop is a
  `While` loop, and every command has a plain-language description and an
  example.
* **Modern language** – type inference ("smart variables"), `Variant`
  dynamic values, enums, methods on types, `Try/Catch/Throw`, string
  interpolation, `IIf`, `For Each ... In`, `+=`, `AndAlso`/`OrElse`,
  `Strict` mode, clear diagnostics with source locations.
* **Fast** – statically typed bytecode (no type dispatch on hot paths),
  compiled with a real multi-stage compiler designed for a future native backend.
* **Fyrox underneath** – entities are Fyrox scene nodes, so PBR rendering,
  shadows and the rest of Fyrox are available to BASIC commands.
* **Everything a game needs** – 2D and 3D graphics, custom
  meshes, animation, terrain, sprites, mirrors, cube maps, collisions,
  picking, sound, movies, keyboard/mouse/joysticks, files, banks, timers,
  TCP/UDP networking and multiplayer games (TCP with LAN discovery).
* **And more** – animated glTF/FBX models, rigid-body physics (Rapier),
  gamepads, fullscreen, graphics quality presets, sky boxes, PBR materials,
  decals, soft particles, level of detail, render-to-texture, shaders in
  GLSL or WGSL with hot reloading, ready-made PBR, toon, water and
  procedural materials, post-processing chains, joint motors and ragdolls, a 2D camera, 2D physics with joints, 2D animation, UI widgets,
  background loading,
  saved scenes (Fyrox `.rgs`), 2D tile maps with autotiling, a language
  server.
* **Windows, Linux and macOS** – the same programs and the same commands on
  all three, tested in CI on each.
* **Plugins** – add your own typed commands in Rust, C or C++ as shared
  libraries; see [docs/PLUGINS.md](docs/PLUGINS.md).
* **Runs on old and new PCs** – any OpenGL 3.3 GPU (roughly 2010 onwards).
  `--quality low` turns off the expensive effects for older laptops;
  `ultra` turns them all on.
* **Runs classic BASIC games** – programs written for Blitz3D run with
  their original command names; modern names such as
  `SetEntityPosition` and `LoadModel` work alongside them.

## Using it

```bash
cargo build --release
```

```bash
target/release/neptune run examples/cube.nb
```

Other commands:

* `neptune run --quality low <file>` – run with the low graphics preset
  (also `medium`, `high`, `ultra`).
* `neptune run --uncapped <file>` – no vsync and no 60 updates-a-second
  limit: the program runs as fast as the machine allows (used by the
  benchmarks, see [docs/BENCHMARKS.md](docs/BENCHMARKS.md)).
* `neptune build <file>` – make a stand-alone game in `dist/<name>/`
  (executable plus the media files the program uses). Build NeptuneBASIC
  with `--release` first so the game runs at full speed.
* `neptune plugins` – list loaded plugins and their commands.
* `neptune lsp` – language server for editors (errors as you type,
  completion, hover, signature help, outline, go to definition).
* `neptune check <file>`, `neptune disasm <file>`, `neptune commands`.

Editor support for VS Code (highlighting, the language server, F5 to run) is in
[`editors/vscode`](editors/vscode/README.md).

## Sample games

Two complete games show what a finished NeptuneBASIC program looks like:

* `examples/games/starcourier.nb` – a 2D space shooter.
* `examples/games/orbrun.nb` – a 3D collect-the-orbs game with an animated fox.

```bash
neptune run examples/games/orbrun.nb
```

## Documentation

* [The manual](docs/MANUAL.md): learn the language and how to make games, from the beginning
* [Command reference](docs/COMMANDS.md): every command, with an example
* [Language summary](docs/LANGUAGE.md): the whole language on one page
* [Shaders, materials and post-processing](docs/SHADERS.md)
* [Sound: playing, 3D sound, mixing and effects](docs/SOUND.md)
* [Making levels in Blender](docs/BLENDER.md)
* [2D levels and sprites from Tiled and Aseprite](docs/TOOLS2D.md)
* [Changelog](CHANGELOG.md): what changed, commit by commit
* [Architecture](docs/ARCHITECTURE.md)
* [Implementation status](docs/STATUS.md)
* [Plugins](docs/PLUGINS.md)

## License

NeptuneBASIC is released under the zlib license (see `LICENSE`).
Language and command design reference Blitz3D by Blitz Research Ltd,
also released under the zlib license; see `NOTICE`.
