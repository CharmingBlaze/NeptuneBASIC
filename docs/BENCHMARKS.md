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
| fib(30), recursive calls | 143 ms |
| 20M integer loop iterations | 1212 ms |
| 3M float sin/cos/sqrt | 609 ms |
| 100k string appends + 20k searches | 264 ms |
| quicksort 200k integers | 446 ms |
| 200k objects x 10 updates | 867 ms |
| prime sieve to 5M | 2876 ms |

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
| 5000 camera-facing sprites, 1 m, overlapping | 13.6 fps |
| 5000 camera-facing sprites, 0.2 m | 13.7 fps |
| 1000 DrawImage per frame | 242 fps |
| 5000 DrawImage per frame | 101 fps |
| 10000 DrawImage per frame | 53.9 fps |
| 1000 Text calls per frame | 244 fps |

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

## What the numbers mean

* Frame rates are for the whole frame: the program's own BASIC code, the
  engine and the GPU. The demos run as shipped.
* Each result is the median of the runs listed above. On a laptop that is
  doing other work at the same time, single results can vary by a third.
* Thousands of separate objects are limited by the per-object work of the
  scene graph and renderer. For scenery that does not move, `EntityBatch`
  merges them: 20000 cubes run about three and a half times faster batched.
* Still slow: 5000 camera-facing sprites (about 14 fps). The cost is per
  sprite on the CPU, not drawing (small sprites are no faster); this is the
  next thing to improve.
* Language times are for a bytecode VM with typed instructions and no JIT.
* Results depend on the machine. Run `neptune bench` on yours to compare.
