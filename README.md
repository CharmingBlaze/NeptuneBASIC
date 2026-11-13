# NeptuneBASIC

A BASIC for making 2D and 3D games. Write a few lines, press F5, and a
window opens with your game in it. The language is simple enough for a
first program, and its thousand-plus commands cover graphics, sound,
input, physics, animation, networking and user interfaces. The
[Fyrox](https://fyrox.rs) engine is underneath.

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

## Get started

1. Download the folder for your computer (Windows, Linux or macOS) from the
   [releases page](https://github.com/CharmingBlaze/NeptuneBASIC/releases)
   and unzip it. Nothing else needs installing.
2. Start **Neptune IDE** (`neptune-ide`).
3. Open `cube.nb`, or pick a program from the **Examples** menu, and press
   **F5**.

`START.txt` in the folder has the first-time steps for macOS and Linux.
To learn the language from the beginning, read [the manual](docs/MANUAL.md).
It is in the folder's `docs` folder too, and the IDE shows any command's
help when you press **F1** on it.

When your game is finished, **Program > Create Executable** makes a folder
you can zip and give to players. They don't need NeptuneBASIC to play it.

## What you get

* **Easy to start.** A short program opens a window, and a game loop is a
  `While` loop. Every command has a plain-language description and an
  example.
* **A modern BASIC.** Type inference, `Variant` values, enums, methods on
  types, `Try`/`Catch`, string interpolation, `For Each`, `+=`, `Strict`
  mode, and error messages that point at the line.
* **Everything a game needs.** 2D and 3D graphics, animated glTF and FBX
  models, terrain, sprites, PBR materials, shadows, shaders in GLSL or WGSL,
  post-processing, rigid-body physics in 2D and 3D, character controllers,
  pathfinding, sound and 3D sound, gamepads, tile maps, a GUI toolkit, files,
  TCP/UDP and multiplayer games that sync their entities for you.
* **Your tools.** Levels from [Blender](docs/BLENDER.md), maps and sprite
  sheets from [Tiled, LDtk and Aseprite](docs/TOOLS2D.md).
* **Neptune IDE.** Run with F5, a debugger, completion, errors as you type
  and built-in help ([guide](docs/IDE.md)). A
  [VS Code extension](editors/vscode/README.md) is available too.
* **Windows, Linux and macOS.** The same programs run on all three.
* **Old and new PCs.** Any OpenGL 3.3 graphics card (about 2010 onwards).
  `--quality low` keeps older laptops smooth, and `ultra` turns everything on.
* **Blitz3D programs run.** Classic command names work, and modern names
  such as `SetEntityPosition` work alongside them.
* **Plugins.** Add your own commands in Rust, C or C++ ([guide](docs/PLUGINS.md)).

## Sample games

The `examples/games` folder holds finished games to play, read and change:

| Game | What it is |
|---|---|
| `starcourier.nb` | a 2D space shooter |
| `orbrun.nb` | collect the orbs in 3D, with an animated fox |
| `foxquest.nb` | a third-person adventure in a level made in Blender |
| `arena.nb` | a first-person shooter in about 300 lines |
| `skyhop.nb` | a 3D platformer with double jumps |
| `kartrace.nb` | three laps against three rivals |
| `dirtbike.nb`, `jetski.nb`, `quadrace.nb`, `skyrace.nb` | racing demos in the style of a 1990s console |
| `coinrush.nb` | a multiplayer game in about 150 lines |

## Documentation

| | |
|---|---|
| [The manual](docs/MANUAL.md) | learn the language and how to make games, from the beginning |
| [Command reference](docs/COMMANDS.md) | every command, with an example |
| [Language summary](docs/LANGUAGE.md) | the whole language on one page |
| [Neptune IDE](docs/IDE.md) | the editor: keys, debugging, building games |
| [Shaders and materials](docs/SHADERS.md) | materials, shaders, screen effects and shadows |
| [Sound](docs/SOUND.md) | playing, 3D sound, mixing and effects |
| [Character controllers](docs/CHARACTERS.md) | characters that walk, jump and climb, in 3D and 2D |
| [Levels in Blender](docs/BLENDER.md) | build levels in Blender and load them |
| [2D tools](docs/TOOLS2D.md) | maps from Tiled and LDtk, sprites from Aseprite |
| [Neptune Level Editor](docs/EDITOR.md) | NeptuneBASIC's own level editor (early version) |
| [Plugins](docs/PLUGINS.md) | add your own commands |
| [What's new](CHANGELOG.md) | every change, newest first |

## The `neptune` command

The IDE does all of this for you, but `neptune` also works from a terminal:

| Command | Does |
|---|---|
| `neptune run game.nb` | runs a program (`--quality low` to `ultra` sets the graphics preset) |
| `neptune check game.nb` | checks a program for mistakes without running it |
| `neptune build game.nb` | makes a stand-alone game in `dist/<name>/` |
| `neptune commands` | lists every command |
| `neptune lsp` | the language server, for editors |

## Building from source

You only need this to work on NeptuneBASIC itself. Install Rust from
[rustup.rs](https://rustup.rs), then:

```bash
cargo build --release
target/release/neptune run examples/cube.nb
```

[CONTRIBUTING.md](CONTRIBUTING.md) explains how the source is laid out,
how to run the tests and how the download folder is made.

## License

NeptuneBASIC is made by CharmingBlaze and released under the zlib license
(see [LICENSE](LICENSE)). Its language and command design follow Blitz3D by
Blitz Research Ltd, also under the zlib license; see [NOTICE](NOTICE).
