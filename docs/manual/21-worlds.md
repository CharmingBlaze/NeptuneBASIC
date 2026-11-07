# 21. Building worlds

[Contents](../MANUAL.md) · Previous: [20. AI and pathfinding](20-ai.md) · Next: [22. Files and data](22-files.md)

This chapter is about filling worlds and keeping them fast: placing many
copies of things, saving and loading levels, loading in the background,
several scenes at once, and the effects that make a world look finished
(particles, decals, detail levels, reflections).

- [Prefabs: one model, many copies](#prefabs-one-model-many-copies)
- [Saving and loading levels](#saving-and-loading-levels)
- [Loading in the background](#loading-in-the-background)
- [Several scenes](#several-scenes)
- [Changing art while the game runs](#changing-art-while-the-game-runs)
- [Rendering effects](#rendering-effects)

## Prefabs: one model, many copies

A **prefab** is a model loaded once and placed many times. Copies share
the model's meshes and textures, so a forest of a thousand trees costs
little more memory than one tree.

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 10, -60
light = CreateLight()

tree = LoadPrefab("examples/media/pyramid.gltf")
SeedRnd 1
For i = 1 To 500
    t = InstancePrefab(tree)
    PositionEntity t, Rnd(-100, 100), 0, Rnd(-100, 100)
    TurnEntity t, 0, Rnd(360), 0
Next

While Not KeyHit(KEY_ESCAPE)
    RenderWorld
    Flip
Wend
```

`CreatePrefab(entity)` makes a prefab from something built in code.
`EntityBatch mesh` draws the children of a mesh merged into a few draws,
for many small static things (rocks, fences, grass clumps).

## Saving and loading levels

`SaveScene file$` saves every entity in the world (except cameras) to a
scene file; `SaveEntity e, file$` saves one entity with its children.
`LoadScene(file$)` (the same as `LoadMesh`) loads it back as one entity,
and `FindChild` finds the named parts:

```basic
door = CreateCube()
NameEntity door, "door"
SaveScene "level1.rgs"
; later, or in another program:
level = LoadScene("level1.rgs")
door = FindChild(level, "door")
```

Scene files (`.rgs`) are the Fyrox engine's own format, so levels can
also be built in the Fyrox editor and loaded here.

For saving game progress (the score, the inventory, the player's
position), write your own file: see [chapter 22](22-files.md).

## Loading in the background

Loading a big level can take a few seconds. Background loading lets a
loading screen show progress meanwhile:

```basic
Graphics3D 1280, 720
fox = LoadMeshAsync("examples/media/Fox.glb")
music = LoadSoundAsync("theme.ogg")
grass = LoadTextureAsync("grass.png")
total = PendingLoads()
While PendingLoads() > 0
    Cls
    done = total - PendingLoads()
    Color 255, 255, 255
    Text 640, 340, "Loading...", True
    Rect 440, 360, 400 * done / total, 20
    Flip
Wend
If EntityLoaded(fox) = -1 Then RuntimeError "Could not load the fox"
```

The `...Async` commands (`LoadMeshAsync`, `LoadTextureAsync`,
`LoadImageAsync`, `LoadSoundAsync`) return a handle at once; the thing
appears when it has loaded. `PendingLoads()` counts what is still
loading, and `EntityLoaded`, `TextureLoaded`, `ImageLoaded` and
`SoundLoaded` say whether each one has arrived (1), is still on its way
(0) or failed (-1).

## Several scenes

A program can have several **scenes** at once, each with its own
entities, lights and physics. Scenes drawn later appear on top of earlier
ones: a 3D HUD or inventory over the game, or the next level loading
while this one plays.

```basic
Graphics3D 1280, 720
camera = CreateCamera()          ; in scene 1, the one Graphics3D makes

hud = CreateScene()
SetScene hud                     ; new entities go into the HUD scene
hudCamera = CreateCamera()
compass = CreateCone(16)
PositionEntity compass, 3, 2, 6
SetScene 1                       ; back to the game

paused = False
While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_P)
        paused = Not paused
        If paused Then HideScene 1 Else ShowScene 1   ; a hidden scene is frozen
    EndIf
    UpdateWorld
    RenderWorld
    Flip
Wend
```

`CurrentScene()`, `EntityScene(e)` and `FreeScene` complete the set.

## Changing art while the game runs

`WatchAssets True` reloads textures, images, sounds and models when their
files change on disk, so you can repaint a texture and see it in the
running game. `ReloadAssets` does it once. `AssetsReloaded()` says how
many were reloaded. Loading the same file twice shares it, so loading
in many places is cheap.

## Rendering effects

**Particles** (smoke, fire, sparks, rain, magic) come from an emitter:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 2, -8
smoke = CreateEmitter(LoadTexture("examples/media/ring.png", 2))
EmitterRate smoke, 40                          ; particles a second
EmitterLife smoke, 1.5, 3                      ; each lives 1.5 to 3 seconds
EmitterVelocity smoke, 0, 1.5, 0, 0.4          ; rising, spread out a little
EmitterSize smoke, 0.3, 0.5                    ; starting size, and growth
EmitterColor smoke, 200, 200, 200, 180
EmitterFadeColor smoke, 80, 80, 80, 0          ; fades to transparent grey
EmitterGravity smoke, 0.3, 0, 0                ; drifts in the wind

While Not KeyHit(KEY_ESCAPE)
    UpdateWorld
    RenderWorld
    Flip
Wend
```

An emitter is an entity: attach it to a torch, a rocket or a chimney.
`EmitterShape`, `EmitterSpin`, `EmitterMax`, `EmitterSoftness`,
`EmitterLocal`, `EmitterEnable` and `EmitterClear` fine-tune it.

**Decals** project a picture onto whatever is inside their box: blood
splats, scorch marks, footprints, graffiti.

```basic
splat = CreateDecal(LoadTexture("splat.png", 3))
PositionEntity splat, 4, 0, 2
ScaleEntity splat, 2, 1, 2
```

**Levels of detail** show simpler versions of a model further away, so
big scenes stay fast:

```basic
tree = CreatePivot()
detailed = LoadMesh("tree_detailed.glb", tree)
simple = LoadMesh("tree_simple.glb", tree)
EntityLOD tree, detailed, 0, 40          ; from 0 to 40 units away
EntityLOD tree, simple, 40, 0            ; from 40 units on (0: no far limit)
```

`EntityAutoFade e, near, far` fades an entity out with distance instead.

**Render to texture**: `CameraToTexture camera, texture` makes a camera
draw into a texture: a security monitor, a rear-view mirror, a portal.

**Camera looks**: `CameraExposure` (fixed or automatic exposure, as eyes
adjust between dark and bright places) and `CameraColorGrading` (a colour
look from a lookup image).

**Reflections**: `CreateReflectionProbe` takes a picture of the scene
around it, so shiny surfaces reflect the room they are in;
`EnvironmentLighting 1` lights and reflects from the sky box or probe.
For materials, screen effects and shadows, see [the shader
manual](../SHADERS.md).

[Contents](../MANUAL.md) · Previous: [20. AI and pathfinding](20-ai.md) · Next: [22. Files and data](22-files.md)
