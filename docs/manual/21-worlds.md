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
- [Try it yourself](#try-it-yourself)

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

## Try it yourself

These use shapes made by the program, and the picture `examples/media/ring.png`
that comes with the repository (run them from the folder that holds `examples`).

1. **Forest.** Build one tree in code (a brown trunk and a green cone of
   leaves), make it a prefab, and plant 150 copies of it at random places on
   a green floor, each a different size. Walk through the forest with the arrow
   keys.
   *Hint: `CreatePrefab` hides the original and keeps it for `InstancePrefab` to copy.*
2. **Campfire.** Make a campfire out of two crossed logs, a particle emitter
   for the flames, and an orange light that makes the ground glow. Space puts
   the fire out and lights it again.
   *Hint: `EmitterEnable` starts and stops the flames, and `HideEntity` and `ShowEntity` work on the light.*
3. **HUD scene.** Spin a ring of eight cubes in the main scene. In a second
   scene, which has its own camera and light, show a spinning gold cone in the
   corner of the screen. Pressing P hides the whole main scene (leaving only
   the cone) and pressing P again brings it back.
   *Hint: build the second scene with `CreateScene` and `SetScene`, then go back with `SetScene 1`; the second scene needs its own light.*

### Answers

1.

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 1.7, -20
CameraClsColor camera, 150, 190, 230
CameraFogMode camera, 1
CameraFogColor camera, 150, 190, 230
CameraFogRange camera, 20, 90
light = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity light, 50, 30, 0
AmbientLight 80, 80, 90

floor = CreatePlane()
EntityColor floor, 70, 140, 70

; one tree, built in code
tree = CreatePivot()
trunk = CreateCylinder(8, 1, tree)
ScaleEntity trunk, 0.3, 1.5, 0.3
PositionEntity trunk, 0, 1.5, 0
EntityColor trunk, 110, 70, 40
leaves = CreateCone(12, 1, tree)
ScaleEntity leaves, 1.5, 2, 1.5
PositionEntity leaves, 0, 4.5, 0
EntityColor leaves, 40, 130, 50
prefab = CreatePrefab(tree)

SeedRnd 5
For i = 1 To 150
    t = InstancePrefab(prefab)
    PositionEntity t, Rnd(-60, 60), 0, Rnd(-60, 60)
    size# = Rnd(0.6, 1.6)
    ScaleEntity t, size, size, size
    TurnEntity t, 0, Rnd(360), 0
Next

While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_LEFT) Then TurnEntity camera, 0, 2, 0
    If KeyDown(KEY_RIGHT) Then TurnEntity camera, 0, -2, 0
    If KeyDown(KEY_UP) Then MoveEntity camera, 0, 0, 0.2
    If KeyDown(KEY_DOWN) Then MoveEntity camera, 0, 0, -0.2
    RenderWorld
    Flip
Wend
```

The tree is built once from a pivot with two children, and `CreatePrefab` makes
it into the original that is copied (and hides it, so it does not stand in the
middle of the world). Each `InstancePrefab` is a new copy that can be placed,
scaled and turned on its own, while the copies share the tree's meshes, so 150
of them cost little. Fog hides the far edge of the forest.

2.

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 3, -8
CameraLookAt camera, 0, 1, 0
CameraClsColor camera, 5, 8, 25
AmbientLight 25, 25, 40

ground = CreatePlane()
EntityColor ground, 50, 60, 50

log1 = CreateCylinder(8)
ScaleEntity log1, 0.2, 1.2, 0.2
RotateEntity log1, 0, 0, 80
PositionEntity log1, 0, 0.25, 0
EntityColor log1, 90, 55, 30
log2 = CreateCylinder(8)
ScaleEntity log2, 0.2, 1.2, 0.2
RotateEntity log2, 80, 0, 0
PositionEntity log2, 0, 0.45, 0
EntityColor log2, 80, 50, 25

fire = CreateEmitter(LoadTexture("examples/media/ring.png", 2))
PositionEntity fire, 0, 0.6, 0
EmitterRate fire, 60
EmitterLife fire, 0.6, 1.2
EmitterVelocity fire, 0, 2, 0, 0.4
EmitterSize fire, 0.5, 0
EmitterColor fire, 255, 170, 40, 220
EmitterFadeColor fire, 255, 40, 0, 0

glow = CreateLight(LIGHT_POINT)
PositionEntity glow, 0, 1.5, 0
LightColor glow, 255, 140, 50
LightRange glow, 12

burning = True
While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_SPACE)
        burning = Not burning
        EmitterEnable fire, burning
        If burning Then ShowEntity glow Else HideEntity glow
    EndIf
    UpdateWorld
    RenderWorld
    Text 10, 10, "Space puts the fire out or lights it again"
    Flip
Wend
```

The flames are particles: each lives for under a second, rises, and fades
from orange to red and then to see-through. Particles are created and moved
during `UpdateWorld`, so it is called every frame. `Not burning` flips the
switch each time Space is pressed; the emitter stops making new flames (the
ones alive fade away), and the light is hidden so the ground goes dark.

3.

```basic
Graphics3D 1280, 720

; scene 1: the main scene
camera = CreateCamera()
PositionEntity camera, 0, 6, -12
CameraLookAt camera, 0, 0, 0
CameraClsColor camera, 20, 30, 60
light = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity light, 50, 30, 0

ring = CreatePivot()
For i = 0 To 7
    cube = CreateCube(ring)
    ScaleEntity cube, 0.6, 0.6, 0.6
    PositionEntity cube, Cos(i * 45) * 5, 0, Sin(i * 45) * 5
    EntityColor cube, 60 + i * 25, 200 - i * 20, 255
Next

; scene 2: the HUD, with its own camera and light
hud = CreateScene()
SetScene hud
hudCamera = CreateCamera()
hudLight = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity hudLight, 30, 30, 0
coin = CreateCone(16)
EntityColor coin, 255, 210, 40
PositionEntity coin, 3, 2, 6
SetScene 1

hidden = False
While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_P)
        hidden = Not hidden
        If hidden Then HideScene 1 Else ShowScene 1
    EndIf
    If Not hidden Then TurnEntity ring, 0, 1, 0

    SetScene hud
    TurnEntity coin, 0, 3, 0
    SetScene 1

    UpdateWorld
    RenderWorld
    Text 10, 10, "P hides and shows the main scene"
    Flip
Wend
```

Each scene has its own entities, lights and cameras, so the HUD scene gets
its own camera and light. `SetScene` chooses the scene that new entities go
into and that commands work on; the program switches to the HUD scene to turn
the cone, and goes back to scene 1 afterwards. The HUD scene is drawn over
the main one with a see-through background. `HideScene 1` stops scene 1 being
drawn or updated, and `ShowScene 1` brings it back exactly as it was. The ring
is only turned while the scene is showing, since it is unchanged while hidden.

[Contents](../MANUAL.md) · Previous: [20. AI and pathfinding](20-ai.md) · Next: [22. Files and data](22-files.md)
