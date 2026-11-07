## What was improved

The first run of this suite found slow spots; these were fixed and measured
again on the same machine (release builds, `--uncapped`).

| Benchmark | Before | After | Why it was slow |
|---|---:|---:|---|
| 50 agents walking a navmesh | 15.5 fps | 165 fps | Fyrox 1.0 finds the closest navmesh point by testing every triangle (its octree query is disabled); every agent did this every frame. Navmeshes now have a grid index (`navquery.rs`). |
| 200 agents walking a navmesh | 4.3 fps | 101 fps | Same; agents that follow a moving target also re-plan at most four times a second. |
| 500 agents walking a navmesh | did not finish | 61 fps | Same. |
| `FindPath` across a 190 m navmesh | 2.40 ms | 0.69 ms | Paths are now A* over the mesh's triangles plus the funnel algorithm, without Fyrox's slow closest-point search. |
| `LinePick` among 400 objects | 5.05 ms | 0.027 ms | Every pick rebuilt every object's shape, and looking up an entity's own meshes scanned every entity. Shapes are now cached until something changes, and the lookup only checks the entity's own children. |

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
  engine, and the GPU. The demos run as shipped.
* Thousands of separate objects are limited by the per-object work of the
  scene graph and renderer. For scenery that does not move, `EntityBatch`
  merges them: 20000 cubes run about four times faster batched.
* Language times are for a bytecode VM with typed instructions; there is no
  JIT.
* Results depend on the machine. Run `neptune bench` on yours to compare.
