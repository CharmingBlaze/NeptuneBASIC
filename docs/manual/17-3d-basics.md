# 17. 3D basics

[Contents](../MANUAL.md) · Previous: [16. Sound and movies](16-sound.md) · Next: [18. 3D models and looks](18-3d-models.md)

A 3D game is a world of **entities**: models, cameras, lights, and
invisible markers. This chapter is about putting entities in the world,
moving and turning them, and seeing them through a camera.

- [The 3D world](#the-3d-world)
- [Entities](#entities)
- [Positions](#positions)
- [Turning](#turning)
- [Scaling](#scaling)
- [Parents and children](#parents-and-children)
- [Cameras](#cameras)
- [Lights](#lights)
- [Distances and directions](#distances-and-directions)
- [A first 3D scene](#a-first-3d-scene)

## The 3D world

Positions in the world have three numbers, **x**, **y** and **z**:

- **x** goes to the right,
- **y** goes up,
- **z** goes forward, into the screen.

The units are yours to choose; most games take 1 unit as 1 metre.
Rotations are three angles in degrees:

- **pitch** tips up and down (positive looks down),
- **yaw** turns left and right (positive turns left),
- **roll** tilts sideways.

## Entities

Everything in a 3D world is an entity, and each command that makes one
returns its **handle**, a number you keep to use it later:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
light = CreateLight()
box = CreateCube()          ; a cube from -1 to 1: 2 units wide
ball = CreateSphere(32)     ; radius 1; 32 is how smooth it is
can = CreateCylinder(24)
spike = CreateCone(24)
floor = CreatePlane()       ; an endless flat floor at y = 0
marker = CreatePivot()      ; invisible: a point to attach things to
```

| Command | Does |
|---|---|
| `HideEntity e`, `ShowEntity e` | hides (not drawn, no collisions) and shows it again |
| `CopyEntity(e)` | a copy (models loaded once can be copied many times cheaply) |
| `FreeEntity e` | removes it and its children for good |
| `NameEntity e, name$`, `EntityName(e)` | gives it a name, reads it back |
| `EntityColor e, r, g, b`, `EntityAlpha e, alpha` | its colour and see-through-ness |

## Positions

```basic
Graphics3D 1280, 720
camera = CreateCamera()
box = CreateCube()
PositionEntity box, 0, 0, 5       ; put it at x 0, y 0, z 5
MoveEntity box, 0, 0, 1           ; forward 1, the way the box faces
TranslateEntity box, 0, 1, 0      ; up 1 in the world, whatever way it faces
Print EntityX(box) + ", " + EntityY(box) + ", " + EntityZ(box)
```

| Command | Moves |
|---|---|
| `PositionEntity e, x, y, z` | to a place |
| `MoveEntity e, x, y, z` | along its **own** directions: z is "forward" for it |
| `TranslateEntity e, x, y, z` | along the **world's** directions |
| `EntityX(e)`, `EntityY(e)`, `EntityZ(e)` | where it is |

`MoveEntity` is how characters and vehicles move: turn them to face a
direction, then move forward.

## Turning

| Command | Does |
|---|---|
| `RotateEntity e, pitch, yaw, roll` | sets its rotation |
| `TurnEntity e, pitch, yaw, roll` | turns it by that much from where it is |
| `PointEntity e, target` | turns it to face another entity |
| `LookAt e, x, y, z` | turns it to face a point |
| `EntityPitch(e)`, `EntityYaw(e)`, `EntityRoll(e)` | its rotation |

```basic
; a tank: left/right turn it, up/down drive it forward and back
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 10, -15
CameraLookAt camera, 0, 0, 0
light = CreateLight()
ground = CreatePlane()
EntityColor ground, 80, 120, 60
tank = CreateCube()
ScaleEntity tank, 1, 0.5, 1.5
PositionEntity tank, 0, 0.5, 0

While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_LEFT) Then TurnEntity tank, 0, 3, 0
    If KeyDown(KEY_RIGHT) Then TurnEntity tank, 0, -3, 0
    If KeyDown(KEY_UP) Then MoveEntity tank, 0, 0, 0.2
    If KeyDown(KEY_DOWN) Then MoveEntity tank, 0, 0, -0.1
    RenderWorld
    Flip
Wend
```

## Scaling

`ScaleEntity e, x, y, z` stretches an entity: 1 is its normal size, 2
twice as big along that direction. A cube scaled 1, 0.5, 1.5 becomes a
flat, long box.

## Parents and children

An entity can be **attached** to another, its **parent**. A child moves,
turns and scales with its parent, and its own position is measured from
the parent. Attach a gun to a hand, a hand to an arm; a camera to a
car; moons to planets:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 5, -12
CameraLookAt camera, 0, 0, 0
light = CreateLight()

sun = CreateSphere(32)
EntityColor sun, 255, 200, 0
earth = CreateSphere(16, sun)          ; the sun is earth's parent
ScaleEntity earth, 0.4, 0.4, 0.4
PositionEntity earth, 6, 0, 0          ; 6 units from the sun
moon = CreateSphere(8, earth)          ; the earth is the moon's parent
PositionEntity moon, 3, 0, 0
ScaleEntity moon, 0.4, 0.4, 0.4

While Not KeyHit(KEY_ESCAPE)
    TurnEntity sun, 0, 0.5, 0          ; the earth goes round the sun...
    TurnEntity earth, 0, 3, 0          ; ...and the moon round the earth
    RenderWorld
    Flip
Wend
```

Most create commands take a parent as their last argument;
`EntityParent child, parent` attaches one later (0 detaches it).
`PositionEntity`, `RotateEntity`, `EntityX` and the rest work relative to
the parent, unless you add `True` at the end for world values:
`EntityX(moon, True)`.

## Cameras

A camera is the player's eye. A scene needs at least one.

| Command | Does |
|---|---|
| `CreateCamera([parent])` | a camera |
| `CameraClsColor cam, r, g, b` | the background colour |
| `CameraRange cam, near, far` | how near and far it can see (1 to 1000 to start with) |
| `CameraZoom cam, zoom` | zooms in (above 1) |
| `CameraViewport cam, x, y, w, h` | draws into part of the screen: split-screen, mini-maps |
| `CameraFogMode cam, 1`, `CameraFogColor`, `CameraFogRange cam, near, far` | fog |
| `CameraProjMode cam, 2` | flat (orthographic) view, for strategy and puzzle games |

A third-person camera follows the player from behind by being its child:

```basic
player = CreateCube()
camera = CreateCamera(player)       ; attached to the player
PositionEntity camera, 0, 3, -8     ; behind and above
TurnEntity camera, 15, 0, 0         ; looking down a little
```

Split screen is two cameras with half the screen each:

```basic
Graphics3D 1280, 720
left = CreateCamera()
CameraViewport left, 0, 0, 640, 720
right = CreateCamera()
CameraViewport right, 640, 0, 640, 720
```

## Lights

```basic
sun = CreateLight(LIGHT_DIRECTIONAL)      ; like the sun: everywhere, from one direction
TurnEntity sun, 50, 30, 0
lamp = CreateLight(LIGHT_POINT)           ; a bulb: all directions from a point
PositionEntity lamp, 0, 3, 0
LightColor lamp, 255, 180, 100            ; warm
LightRange lamp, 10
torch = CreateLight(LIGHT_SPOT)           ; a cone
LightConeAngles torch, 20, 35             ; full brightness inside 20 degrees, fading out to 35
AmbientLight 40, 40, 50                   ; a little light everywhere, so shadows aren't black
```

A light is an entity, so it can be moved, turned and attached: a spot
light that is a child of the camera is a torch. Directional lights only
care about which way they point; point lights about where they are;
spot lights about both. `SetLightDirection light, x, y, z` points a light
along a direction. Lights can cast shadows: see
[chapter 7 of the shader manual](../shaders/07-shadows.md).

## Distances and directions

| Command | Gives |
|---|---|
| `EntityDistance(a, b)` | the distance between two entities |
| `DeltaYaw(a, b)`, `DeltaPitch(a, b)` | how far `a` must turn to face `b` |
| `TFormPoint x, y, z, from, to` then `TFormedX()`... | a point in one entity's space seen from another (0 is the world) |
| `TFormVector`, `TFormNormal` | the same for directions |
| `VectorYaw(x, y, z)`, `VectorPitch(x, y, z)` | the angles of a direction |

An enemy that turns towards the player and comes closer:

```basic
player = CreateCube()
enemy = CreateSphere()
PositionEntity enemy, 10, 0, 10
; each frame:
TurnEntity enemy, 0, DeltaYaw(enemy, player) * 0.1, 0   ; turn a tenth of the way
If EntityDistance(enemy, player) > 2 Then MoveEntity enemy, 0, 0, 0.05
```

## A first 3D scene

```basic
; a small scene: walk around with the arrow keys
Graphics3D 1280, 720
AppTitle "First scene"

camera = CreateCamera()
PositionEntity camera, 0, 1.7, -10      ; eye height
CameraClsColor camera, 140, 180, 230    ; sky blue
CameraFogMode camera, 1
CameraFogColor camera, 140, 180, 230
CameraFogRange camera, 20, 80

sun = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity sun, 45, 45, 0
AmbientLight 70, 70, 80

ground = CreatePlane()
EntityColor ground, 90, 140, 70

SeedRnd 7
For i = 1 To 40
    tree = CreateCone(12)
    ScaleEntity tree, 1, Rnd(2, 4), 1
    PositionEntity tree, Rnd(-40, 40), 2, Rnd(-40, 40)
    EntityColor tree, 40, Rand(90, 140), 50
Next

While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_LEFT) Then TurnEntity camera, 0, 2, 0
    If KeyDown(KEY_RIGHT) Then TurnEntity camera, 0, -2, 0
    If KeyDown(KEY_UP) Then MoveEntity camera, 0, 0, 0.15
    If KeyDown(KEY_DOWN) Then MoveEntity camera, 0, 0, -0.15
    RenderWorld
    Flip
Wend
```

[Contents](../MANUAL.md) · Previous: [16. Sound and movies](16-sound.md) · Next: [18. 3D models and looks](18-3d-models.md)
