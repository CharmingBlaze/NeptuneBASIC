# The NeptuneBASIC Shader Manual

How to make things look the way you want: ready-made materials (shiny
metal, cartoon shading, water, swaying grass, patterns made from noise),
screen effects (bloom, old-TV, rain, depth of field), shadows, and shaders
you write yourself.

You don't need to know anything about graphics programming to start. The
first chapters only use BASIC commands; GLSL, the language shaders are
written in, comes later and is explained from the beginning.

Everything in this manual is an extension (ext): Blitz3D had no shaders.

## Contents

| Chapter | What you will learn |
|---|---|
| [1. Introduction](shaders/01-introduction.md) | What shaders, materials, textures and post-processing are, in plain words, and which tool to pick for a job |
| [2. First steps](shaders/02-first-steps.md) | A guided tour: a scene, a shiny material, a cartoon look, a screen effect, shadows and your first shader, one small program at a time |
| [3. Ready-made materials](shaders/03-materials.md) | PBR, toon and stylized looks, water, foliage and wind, procedural patterns, and the 33 built-in shaders, with every setting explained |
| [4. Writing shaders](shaders/04-writing-shaders.md) | A tutorial: colour, animation, parameters, textures, lighting, transparency, moving vertices and keywords |
| [5. The shader language](shaders/05-shader-language.md) | Reference for NeptuneBASIC's GLSL: file layout, directives, surfaces, built-in values, attributes and the function library |
| [6. Post-processing](shaders/06-post-processing.md) | Effect chains, the 30 built-in effects with every parameter, and writing your own effects |
| [7. Shadows](shaders/07-shadows.md) | How real-time shadows work, every shadow setting, and fixing the usual problems |
| [8. WGSL and Fyrox shader files](shaders/08-wgsl-and-fyrox.md) | The other two shader formats |
| [9. Debugging and performance](shaders/09-debugging.md) | Reading errors, hot reloading, inspecting shaders, keeping things fast, coordinates, limits and how the system works |
| [10. Command reference](shaders/10-reference.md) | Every command, with its syntax, what each argument means, what it returns, and an example |

## How to read this manual

- **New to all this?** Read chapters 1 and 2 in order, and type in the
  examples. Then dip into chapter 3 for the looks you want.
- **Want to write shaders?** Do chapter 4 from start to finish, then keep
  chapter 5 open while you work.
- **Looking up a command?** Go straight to chapter 10. Every command
  there links back to the chapter that explains it.

## Conventions

- Code in boxes marked `basic` is NeptuneBASIC. Code marked `glsl` is
  shader code, usually in a file of its own ending in `.glsl`.
- Programs in this manual are complete unless they say otherwise: copy
  one into a file, say `test.nb`, and run it with `neptune run test.nb`.
  Snippets that continue an earlier program say so.
- Colours given to commands are 0-255 for red, green and blue, as
  everywhere else in NeptuneBASIC. Inside shaders, colours are 0-1.
- Every BASIC example in this manual is checked by
  `python tools/check_doc_examples.py`, and every complete shader is
  compiled on the GPU by `python tools/check_doc_examples.py --gpu`.

## Example programs

| File | Shows |
|---|---|
| `examples/shaders.nb` | every kind of material and shader side by side, with screen effects on keys 1-7 |
| `examples/shadertests.nb` | 20 checks of the shader system on the GPU (`neptune run --uncapped examples/shadertests.nb`) |
| `examples/media/shaders/rimtoon.glsl` | a complete surface shader: toon lighting, noise and a glowing rim |
| `examples/media/shaders/ripple.wgsl` | a WGSL shader |
| `examples/media/shaders/scanlines_post.glsl` | a custom screen effect |
| `examples/island.nb`, `examples/beach.nb` | the terrain and water shaders in a real scene |

The one-line descriptions of all commands, used for hover help in
editors, are in the [command reference](COMMANDS.md).
