# The NeptuneBASIC Manual

NeptuneBASIC is a programming language for making games. You write a
program in plain text, run it, and a window opens with your game in it.
The language is simple enough for a first program and has everything a
finished 2D or 3D game needs: graphics, sound, input, physics, animation,
networking, user interfaces and more.

This manual teaches it from the beginning. You don't need to have
programmed before.

## Part 1: Getting started

| Chapter | What you will learn |
|---|---|
| [1. Getting started](manual/01-getting-started.md) | Installing NeptuneBASIC, running a program, your editor, and your first program |

## Part 2: The language

| Chapter | What you will learn |
|---|---|
| [2. Values and variables](manual/02-values-and-variables.md) | Numbers, text, variables, types, constants, and where variables can be seen |
| [3. Expressions and operators](manual/03-expressions.md) | Arithmetic, comparisons, logic, and the maths functions |
| [4. Making decisions](manual/04-decisions.md) | `If`, `Select` and `IIf` |
| [5. Loops](manual/05-loops.md) | `For`, `While`, `Repeat`, `For Each`, `Exit` and `Continue` |
| [6. Functions](manual/06-functions.md) | Writing your own commands: parameters, defaults, return values, recursion |
| [7. Text](manual/07-text.md) | Joining, cutting, searching and formatting text |
| [8. Arrays and data](manual/08-arrays-and-data.md) | Lists and grids of values, and `Data` lines |
| [9. Types and objects](manual/09-types.md) | Your own kinds of thing: fields, methods, lists of objects, enums |
| [10. Errors and debugging](manual/10-errors.md) | Reading error messages, `Try`/`Catch`, `Strict` and finding bugs |
| [11. Organising programs](manual/11-organising.md) | Splitting programs into files, labels, and program-wide settings |

## Part 3: Making games

| Chapter | What you will learn |
|---|---|
| [12. The game loop](manual/12-game-loop.md) | Opening a window, drawing frames, timing, and the shape of every game |
| [13. 2D graphics](manual/13-2d-graphics.md) | Shapes, colours, text, fonts, images, sprites sheets and pixels |
| [14. 2D games](manual/14-2d-games.md) | Tile maps, a scrolling camera, animation, 2D physics and simple buttons |
| [15. Input](manual/15-input.md) | Keyboard, mouse, joysticks and gamepads |
| [16. Sound and movies](manual/16-sound.md) | Sound effects, music, 3D sound and playing movies |
| [17. 3D basics](manual/17-3d-basics.md) | Entities, moving and turning them, cameras and lights |
| [18. 3D models and looks](manual/18-3d-models.md) | Models, animation, textures, meshes you build, sprites, terrain and sky |
| [19. Collisions and physics](manual/19-collisions-physics.md) | Collisions, picking, rigid-body physics, joints and ragdolls |
| [20. AI and pathfinding](manual/20-ai.md) | Navigation meshes, agents, behaviour trees and grid paths |
| [21. Building worlds](manual/21-worlds.md) | Scenes, prefabs, saving and loading, background loading, and rendering effects |
| [22. Files and data](manual/22-files.md) | Reading and writing files, banks of bytes, and time |
| [23. Networking](manual/23-networking.md) | TCP and UDP, and multiplayer games |
| [24. User interfaces](manual/24-gui.md) | Windows, buttons, lists, menus and dialogs |
| [25. Sharing your game](manual/25-sharing.md) | Making a stand-alone game, graphics quality options, and plugins |

## Build a whole game

Two projects that take you from an empty file to a finished game, a step at
a time, running it after each step.

| Project | What you will make |
|---|---|
| [Project 1: Star Catcher](manual/p1-star-catcher.md) | A 2D game of catching falling stars, with lives, a title screen and a saved best score |
| [Project 2: Crystal Hunt](manual/p2-crystal-hunt.md) | A 3D game of driving around collecting crystals against the clock, with collisions and a following camera |

## Part 4: Reference

| | |
|---|---|
| [2D levels and sprites from other tools](TOOLS2D.md) | Maps from Tiled with layers and objects, and animated sprite sheets from Aseprite: a guide of its own |
| [Character controllers](CHARACTERS.md) | Characters that walk, jump, climb stairs and ride platforms, in 3D and 2D: a guide of its own |
| [Making levels in Blender](BLENDER.md) | Build levels in Blender and load their shapes, lights, cameras, solid objects and custom properties: a guide of its own |
| [The shader manual](SHADERS.md) | Materials, shaders, screen effects and shadows: a manual of its own |
| [Command reference](COMMANDS.md) | Every command, with what it does and an example |
| [Language summary](LANGUAGE.md) | The whole language on one page, for when you know it and need a reminder |
| [Plugins](PLUGINS.md) | Adding your own commands in Rust, C or C++ |

## How to use this manual

- **New to programming?** Read parts 1 and 2 in order, and type in the
  examples rather than copying them: you learn more from the mistakes.
  Then go to [chapter 12](manual/12-game-loop.md). Each chapter ends with
  exercises and their answers.
- **Want to make something straight away?** Learn the language up to
  chapter 6, then build [Project 1](manual/p1-star-catcher.md).
- **Programmed before, in another language?** Skim part 2 (the tables
  and examples are enough), then go to part 3.
- **Looking for a command?** The [command reference](COMMANDS.md) has
  every one, with an example.

Each example in this manual is a complete program unless it says
otherwise. When an example is followed by a box marked **Output**, that
is exactly what the program prints. Every example is checked
automatically by `python tools/check_doc_examples.py`, and the printed
results are compared with what the program really prints.

## Example programs

The `examples` folder has a program for each area, and
`examples/games` has two complete games:

| Program | Shows |
|---|---|
| `examples/games/starcourier.nb` | a complete 2D space shooter |
| `examples/games/orbrun.nb` | a complete 3D game with an animated fox |
| `examples/cube.nb` | the smallest 3D program |
| `examples/draw2d.nb`, `examples/buffers.nb` | 2D drawing |
| `examples/platformer2d.nb`, `examples/tilemap.nb` | 2D games |
| `examples/models.nb`, `examples/animation.nb`, `examples/animstates.nb` | 3D models and animation |
| `examples/collisions.nb`, `examples/picking.nb`, `examples/physics.nb`, `examples/joints.nb` | collisions and physics |
| `examples/terrain.nb`, `examples/island.nb`, `examples/beach.nb`, `examples/worlds.nb` | landscapes |
| `examples/ai.nb` | AI and pathfinding |
| `examples/gui.nb`, `examples/hud.nb` | user interfaces |
| `examples/sound.nb`, `examples/movie.nb` | sound and movies |
| `examples/shaders.nb`, `examples/materials.nb`, `examples/rendering.nb` | materials, shaders and effects |

Run any of them from the NeptuneBASIC folder with `neptune run`:

```
neptune run examples/games/orbrun.nb
```
