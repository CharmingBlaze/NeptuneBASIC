# NeptuneBASIC documentation

New to NeptuneBASIC? Start with [the manual](MANUAL.md). It teaches the
language and how to make games from the beginning, and you don't need to
have programmed before.

## Learn

| | |
|---|---|
| [The manual](MANUAL.md) | 25 chapters and two whole-game projects, from your first program to a finished game |
| [Language summary](LANGUAGE.md) | the whole language on one page, for when you need a reminder |
| [Command reference](COMMANDS.md) | every command, with what it does and an example |

## Guides

| | |
|---|---|
| [Neptune IDE](IDE.md) | the editor: keys, running, debugging, building a game |
| [Shaders and materials](SHADERS.md) | materials, shaders, screen effects and shadows (a manual of its own) |
| [Sound](SOUND.md) | playing sounds and music, 3D sound, mixing and effects |
| [Character controllers](CHARACTERS.md) | characters that walk, jump, climb stairs and ride platforms |
| [Levels in Blender](BLENDER.md) | build levels in Blender and load them with `LoadLevelFile` |
| [2D tools](TOOLS2D.md) | maps from Tiled, LDtk and others; sprite sheets from Aseprite |
| [Neptune Level Editor](EDITOR.md) | NeptuneBASIC's own level editor (early version) |
| [Plugins](PLUGINS.md) | add your own commands in Rust, C or C++ |

## For people working on NeptuneBASIC

The [`dev`](dev) folder has the [architecture](dev/ARCHITECTURE.md), the
[implementation status](dev/STATUS.md) and the
[benchmark results](dev/BENCHMARKS.md). `commands.txt` and the `examples`
folder here are the sources `tools/gen_reference.py` builds the command
reference from. See [CONTRIBUTING.md](../CONTRIBUTING.md).
