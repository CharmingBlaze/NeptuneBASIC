# Benchmarks

Measured with `neptune bench` (programs in [`benchmarks/`](../benchmarks)), run with `neptune run --uncapped`: no vsync and no 60 updates-a-second limit, so frame rates show what the machine can do. Re-run it on your own machine to compare: build with `cargo build --release`, then run `target/release/neptune bench` from the repository root.

## Machine

| | |
|---|---|
| OS | Microsoft Windows 11 Home 10.0.26200 |
| CPU | AMD Ryzen 7 250 w/ Radeon 780M Graphics |
| Cores (threads) | 16 |
| Memory | 15 GB |
| Graphics | NVIDIA GeForce RTX 5050 Laptop GPU, AMD Radeon 780M Graphics |
| NeptuneBASIC | 0.1.0 (release build) |
| Runs | 3 (each result is the median) |
| Run time | 509 s |

## Language

The bytecode VM on its own, no window: wall-clock time of each program (lower is better).

| Benchmark | Result |
|---|---:|
| fib(30), recursive calls | 49.0 ms |
| 20M integer loop iterations | 283 ms |
| 3M float sin/cos/sqrt | 234 ms |
| 100k string appends + 20k searches | 274 ms |
| quicksort 200k integers | 115 ms |
| 200k objects x 10 updates | 262 ms |
| prime sieve to 5M | 347 ms |

## Entities

Copies of a cube: still, all turned every frame by the program, and merged with `EntityBatch` (frames per second, higher is better).

| Benchmark | Result |
|---|---:|
| create 1000 copies | 2.000 ms |
| 1000 still cubes | 228 fps |
| 1000 cubes all turning | 229 fps |
| create 5000 copies | 27.0 ms |
| 5000 still cubes | 78.2 fps |
| 5000 cubes all turning | 63.6 fps |
| create 10000 copies | 121 ms |
| 10000 still cubes | 46.2 fps |
| 10000 cubes all turning | 32.4 fps |
| create 20000 copies | 543 ms |
| 20000 still cubes | 26.7 fps |
| 20000 cubes all turning | 18.8 fps |
| 20000 still cubes, batched | 92.7 fps |

## Models

Loading glTF files and textures (milliseconds) and animated glTF characters, each playing its animation (frames per second).

| Benchmark | Result |
|---|---:|
| load Fox.glb (first time) | 10.0 ms |
| load Fox.glb again (each, cached) | 0.300 ms |
| LoadTexture (each, cached) | 0.070 ms |
| 10 animated Fox models | 302 fps |
| 50 animated Fox models | 214 fps |
| 100 animated Fox models | 138 fps |
| 200 animated Fox models | 75.6 fps |

## Physics

Rigid bodies falling and stacking (Rapier physics), and spheres sliding over a level with Blitz3D-style collisions (frames per second).

| Benchmark | Result |
|---|---:|
| 250 falling, stacking boxes | 150 fps |
| 500 falling, stacking boxes | 84.8 fps |
| 1000 falling, stacking boxes | 43.6 fps |
| 2000 falling, stacking boxes | 88.4 fps |
| 100 spheres sliding (Blitz collisions) | 171 fps |
| 300 spheres sliding (Blitz collisions) | 74.2 fps |
| 1000 spheres sliding (Blitz collisions) | 19.7 fps |

## Queries

Picking, navigation meshes, pathfinding, agents and terrain erosion (milliseconds per call, or frames per second).

| Benchmark | Result |
|---|---:|
| LinePick among 400 boxes (each) | 0.028 ms |
| GenerateNavMesh 190x190 m, 1 m cells | 633 ms |
| FindPath across the navmesh (each) | 0.758 ms |
| grid A* on 256x256 with walls (each) | 0.276 ms |
| 50 agents walking the navmesh | 154 fps |
| 200 agents walking the navmesh | 108 fps |
| 500 agents walking the navmesh | 65.4 fps |
| ErodeTerrain 256 grid, 100k raindrops | 256 ms |

## Rendering

Terrain with and without level of detail, particles, sprites and 2D drawing over the 3D view (frames per second).

| Benchmark | Result |
|---|---:|
| 1024x1024 terrain, full detail | 72.7 fps |
| 1024x1024 terrain, TerrainDetail 60000 | 184 fps |
| 20000 particles | 333 fps |
| 5000 camera-facing sprites, 1 m, overlapping | 66.2 fps |
| 5000 camera-facing sprites, 0.2 m | 69.7 fps |
| 1000 DrawImage per frame | 284 fps |
| 5000 DrawImage per frame | 190 fps |
| 10000 DrawImage per frame | 132 fps |
| 1000 Text calls per frame | 264 fps |

## Quality

One scene that leans on the graphics card (shadows from the sun and six lamps, ambient occlusion, bloom, anti-aliasing) at each `GraphicsQuality` level, with the card that drew it (frames per second). When the numbers are alike, the graphics card is not the limit.

| Benchmark | Result |
|---|---:|
| quality low on AMD Radeon 780M Graphics | 89.4 fps |
| quality medium on AMD Radeon 780M Graphics | 71.3 fps |
| quality high on AMD Radeon 780M Graphics | 51.7 fps |
| quality ultra on AMD Radeon 780M Graphics | 41.4 fps |
| quality low on NVIDIA GeForce RTX 5050 Laptop GPU | 144 fps |
| quality medium on NVIDIA GeForce RTX 5050 Laptop GPU | 144 fps |
| quality high on NVIDIA GeForce RTX 5050 Laptop GPU | 144 fps |
| quality ultra on NVIDIA GeForce RTX 5050 Laptop GPU | 144 fps |

## World

A world of 40,000 objects in 100 chunks, with only the chunks near the camera in view: the far chunks hidden (`HideEntity`), then stored (`StoreEntity`), and chunks swapped in and out (milliseconds, or frames per second).

| Benchmark | Result |
|---|---:|
| build a world of 40000 objects in 100 chunks | 70 ms |
| 40000 objects in memory, none shown | 54.5 fps |
| walking through it, 9 chunks (3600 objects) shown | 35.7 fps |
| 40000 objects stored, none in the scene | 267 fps |
| walking through it, far chunks stored | 176 fps |
| store or restore a chunk of 400 objects (each) | 0.040 ms |
| free a chunk of 400 objects and build another (each) | 0.55 ms |
| free the whole world | 27 ms |

## Demos

The example demos as they are, run for a fixed number of frames (frames per second).

| Benchmark | Result |
|---|---:|
| island demo, orbiting (2952 trees, 67 palms) | 78.4 fps |
| beach demo, standing on the sand | 123 fps |


## What was improved

The first run of this suite found slow spots; these were fixed and measured
again on the same machine (release builds, `--uncapped`).

| Benchmark | Before | After | Why it was slow |
|---|---:|---:|---|
| 50 agents walking a navmesh | 15.5 fps | 154 fps | Fyrox 1.0 finds the closest navmesh point by testing every triangle (its octree query is disabled), and every agent did this every frame. Navmeshes now have a grid index (`navquery.rs`). |
| 200 agents walking a navmesh | 4.3 fps | 108 fps | Same; agents that follow a moving target also re-plan at most four times a second. |
| 500 agents walking a navmesh | did not finish | 65 fps | Same. |
| `FindPath` across a 190 m navmesh | 2.40 ms | 0.76 ms | Paths are now A* over the mesh's triangles plus the funnel algorithm, without Fyrox's slow closest-point search. |
| `LinePick` among 400 objects | 5.05 ms | 0.028 ms | Every pick rebuilt every object's shape, and finding an entity's own meshes scanned every entity. Shapes are now cached until something changes, and the lookup only checks the entity's own children. |
| fib(30), recursive calls | 135 ms | 49 ms | See *The interpreter* below. |
| 20M integer loop iterations | 1189 ms | 283 ms | Same. |
| 3M float sin/cos/sqrt | 521 ms | 234 ms | Same. |
| quicksort 200k integers | 411 ms | 115 ms | Same. |
| 200k objects x 10 updates | 545 ms | 262 ms | Same. |
| 5000 camera-facing sprites | 13.6 fps | 66 fps | Every sprite had its own copy of the same four corners, so the renderer drew 5000 separate meshes. Sprites of the same size (`ScaleSprite`, `HandleSprite`) now share one quad and are drawn as instances. |
| 10000 `DrawImage` a frame | 53.9 fps | 132 fps | Every image, rectangle, oval and line was its own draw call. Drawing that follows with the same image (or the same colour) and the same `Viewport` now goes to the GPU together; the order on screen is unchanged. |
| 5000 `DrawImage` a frame | 101 fps | 190 fps | Same. |
| `CopyEntity` 40000 times | 3905 ms | 55 ms | Every copy searched every entity for terrains to rebuild, so each copy got slower as the world grew (20,000 copies took 655 ms, 40,000 took six times as long). The search now only happens once a terrain exists. |
| `FreeEntity` 40000 times | 4686 ms | 83 ms | Every `FreeEntity` went through every entity twice looking for mirrors. It now looks only at what it frees. |
| prime sieve to 5M | 1634 ms | 347 ms | Same, and the 5-million-element array took 120 MB (24 bytes an element); it now takes 20 MB. |

Other problems the suite found and that were fixed:

* **Particles were squares.** Fyrox's particle shader only uses a texture's
  red channel, so a soft round particle texture (white, shaped by alpha) was
  drawn as squares. Emitters now use their own copy of the shader that uses
  the texture's colour and alpha, as Blitz3D does.
* **Particle growth and spin were 60 times too slow** (`EmitterSize` grow,
  `EmitterSpin`), and particle speeds were wrong when frames did not last
  exactly 1/60 s.
* **Every transform change ran an iterative solver** to turn the rotation
  into a quaternion; it now uses the closed-form conversion.

### The interpreter

The language benchmarks were measured again after three changes to the
bytecode interpreter (before and after on the same machine, same day, median
of three runs of `neptune run benchmarks/language.nb`):

* **Superinstructions.** The compiler joins short runs of instructions into
  one: the test of an `If`, `While` or `For`, arithmetic with a variable or
  a constant, a field of an object, an array element. The integer loop went
  from 14 instructions an iteration to 7.
* **Numbers stay numbers.** Every push of a number used to build a whole
  24-byte value and copy it to the stack, and reading it back made the
  processor wait for that copy. Numbers are now written and read in place as
  4 bytes, and the top of the stack is kept in a processor register.
* **`Dim` arrays of numbers hold plain numbers**, 4 bytes an element instead
  of 24.

String building did not change (274 ms): `s = s + x` copies the whole string
every time, which is what that benchmark measures.

### Which graphics card

This laptop has two graphics cards, and every frame rate on this page
except the NVIDIA rows of the quality table was drawn by the slower,
power-saving one (AMD Radeon 780M, built into the processor): Windows gives
that card to OpenGL programs that do not ask for the other. NeptuneBASIC now
asks for the fast card, and `GraphicsCardName` (and the quality table) says
which one drew. The quality table shows both: the built-in card gains a
factor of two from ultra to low, while the NVIDIA card draws every level at
the 144 a second its driver holds this screen to. The other tables have not
been run again on the NVIDIA card; to measure the built-in card on purpose,
build with `NEPTUNE_GPU_EXPORTS=0`.

### Large games

* **A very large program compiles quickly.** `tools/gen_big_program.py`
  writes programs of any size that use what game code uses. `neptune check`
  takes 0.19 s for 14,000 lines, 0.44 s for 55,000 lines and 1.4 s for
  218,000 lines (8000 functions): the time grows in step with the size.
* **Hidden objects cost time; stored ones do not.** Every object in the
  scene is visited by the engine each frame, about 0.4 microseconds each,
  hidden or not: 40,000 hidden objects bring an empty scene from about 300
  frames a second to 54. `StoreEntity` takes an entity and its children out
  of the scene without freeing them, and `RestoreEntity` puts them back as
  they were. With the far chunks stored instead of hidden, the same walk
  through the world runs at 176 frames a second instead of 36, and storing
  or restoring a chunk of 400 objects takes 0.04 ms. `examples/bigworld.nb`
  shows it.

## What the numbers mean

* Frame rates are for the whole frame: the program's own BASIC code, the
  engine and the GPU. The demos run as shipped.
* Each result is the median of the runs listed above. On a laptop that is
  doing other work at the same time, single results can vary by a third.
* Thousands of separate objects are limited by the per-object work of the
  scene graph and renderer. For scenery that does not move, `EntityBatch`
  merges them: 20000 cubes run about three and a half times faster batched.
* Sprites draw fastest when many have the same size: sprites that each
  have a different `ScaleSprite` still cost a draw call each (about 14 fps
  for 5000). `ScaleEntity` does not have this cost. 2D drawing is fastest
  when the same image is drawn many times in a row; alternating between
  images, or changing `Color` between shapes, starts a new draw call.
* Still slow: `Text` lays its string out again every frame (about 2
  microseconds a call), and every sprite is a scene node of its own.
* Language times are for a bytecode VM with typed instructions and no JIT.
  The frame rates above were measured before the interpreter was made
  faster; programs that do a lot of their own work each frame gain from it.
* Results depend on the machine. Run `neptune bench` on yours to compare.
