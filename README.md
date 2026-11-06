# NeptuneBASIC

A modern BASIC for making 3D games — the immediacy of Blitz3D, a modern
language, and the [Fyrox](https://fyrox.rs) engine underneath. Written in Rust.

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

* **Classic syntax still works** – `Type`/`Field`, `Function`, `Dim`, `Gosub`,
  `Data`, `$`/`#`/`%` tags, `While/Wend`, Blitz3D command names.
* **Modern language** – type inference ("smart variables"), `Variant`
  dynamic values, enums, methods on types, `Try/Catch/Throw`, string
  interpolation, `IIf`, `For Each ... In`, `+=`, `AndAlso`/`OrElse`,
  `Strict` mode, clear diagnostics with source locations.
* **Fast** – statically typed bytecode (no type dispatch on hot paths),
  compiled with a real multi-stage compiler designed for a future native backend.
* **Fyrox underneath** – entities are Fyrox scene nodes, so PBR rendering,
  shadows and the rest of Fyrox are available to BASIC commands.
* **All of Blitz3D** – every Blitz3D command: 2D and 3D graphics, custom
  meshes, animation, terrain, sprites, mirrors, cube maps, collisions,
  picking, sound, keyboard/mouse/joysticks, files, banks, timers and TCP/UDP
  networking.
* **And more** – animated glTF/FBX models, rigid-body physics (Rapier),
  gamepads, fullscreen, graphics quality presets, a language server.
* **Runs on old and new PCs** – any OpenGL 3.3 GPU (roughly 2010 onwards).
  `--quality low` turns off the expensive effects for older laptops;
  `ultra` turns them all on.
* **Modern names too** – `SetEntityPosition`, `LoadModel`,
  `UpdatePhysics`, ... work alongside the classic Blitz3D names.

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
* `neptune build <file>` – make a stand-alone game in `dist/<name>/`
  (executable plus the media files the program uses). Build NeptuneBASIC
  with `--release` first so the game runs at full speed.
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

* [Language reference](docs/LANGUAGE.md)
* [Architecture](docs/ARCHITECTURE.md)
* [Implementation status](docs/STATUS.md)

## License

NeptuneBASIC is released under the zlib license (see `LICENSE`).
Language and command design reference Blitz3D by Blitz Research Ltd,
also released under the zlib license; see `NOTICE`.
