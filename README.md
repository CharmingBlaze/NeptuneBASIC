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

## Using it

```bash
cargo build --release
```

```bash
target/release/neptune run examples/cube.nb
```

Other commands: `neptune check <file>`, `neptune disasm <file>`,
`neptune commands`.

## Documentation

* [Language reference](docs/LANGUAGE.md)
* [Architecture](docs/ARCHITECTURE.md)
* [Implementation status](docs/STATUS.md)

## License

NeptuneBASIC is released under the zlib license (see `LICENSE`).
Language and command design reference Blitz3D by Blitz Research Ltd,
also released under the zlib license; see `NOTICE`.
