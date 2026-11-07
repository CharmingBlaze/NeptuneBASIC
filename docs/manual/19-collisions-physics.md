# 19. Collisions and physics

[Contents](../MANUAL.md) · Previous: [18. 3D models and looks](18-3d-models.md) · Next: [20. AI and pathfinding](20-ai.md)

Things in a game must not walk through walls, bullets must hit what they
are aimed at, and crates should tumble when knocked. NeptuneBASIC has
three tools for this:

| Tool | Good for |
|---|---|
| **Collisions** | characters walking and sliding along walls and floors, under your control |
| **Picking** | "what is along this line?": bullets, clicking on things, line of sight |
| **Physics** | objects that move by themselves under gravity: falling, rolling, stacking, swinging |

- [Collisions](#collisions)
- [Reading what was hit](#reading-what-was-hit)
- [Picking](#picking)
- [Physics](#physics)
- [Joints](#joints)
- [Ragdolls](#ragdolls)

## Collisions

Collisions work with **types**: numbers you give entities to say what
kind of thing they are. Then rules say which types stop which.

```basic
Const TYPE_PLAYER = 1, TYPE_WALL = 2

Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 8, -12
CameraLookAt camera, 0, 0, 0
light = CreateLight()

player = CreateSphere(16)
EntityType player, TYPE_PLAYER
EntityRadius player, 1                     ; the player is a ball of radius 1

wall = CreateCube()
ScaleEntity wall, 1, 2, 6
PositionEntity wall, 5, 0, 0
EntityType wall, TYPE_WALL

; players stop at walls, sliding along them
Collisions TYPE_PLAYER, TYPE_WALL, 2, 2

While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_RIGHT) Then MoveEntity player, 0.1, 0, 0
    If KeyDown(KEY_LEFT) Then MoveEntity player, -0.1, 0, 0
    If KeyDown(KEY_UP) Then MoveEntity player, 0, 0, 0.1
    If KeyDown(KEY_DOWN) Then MoveEntity player, 0, 0, -0.1
    UpdateWorld                            ; collisions are worked out here
    RenderWorld
    Flip
Wend
```

`Collisions from_type, to_type, method, response` sets up a rule.
Entities of `from_type` are the ones that move; they are checked against
entities of `to_type`.

| Method | Checks |
|---|---|
| 1 | sphere against sphere (`EntityRadius` on both) |
| 2 | sphere against the real shape (every triangle) of the other entity |
| 3 | sphere against a box (`EntityBox` on the other) |

| Response | The mover |
|---|---|
| 1 | stops |
| 2 | slides along the surface (walking along walls) |
| 3 | slides, but doesn't slide down slopes (walking up hills) |

`EntityRadius e, radius [, y_radius]` sets the mover's size: with a
y radius it is an ellipsoid, the usual shape for a standing character.
Collisions are checked during `UpdateWorld`, along the way the entity
moved since the last one. To teleport something without it colliding on
the way, call `ResetEntity` after moving it.

Gravity for a collision-based character is just moving it down every
frame; the floor stops it:

```basic
; in the game loop, before UpdateWorld:
TranslateEntity player, 0, -0.2, 0       ; fall, unless something is underneath
```

## Reading what was hit

After `UpdateWorld`:

| Command | Gives |
|---|---|
| `EntityCollided(e, type)` | the entity of that type it hit, or 0 |
| `CountCollisions(e)` | how many collisions it had |
| `CollisionEntity(e, i)` | what collision i hit (counting from 1) |
| `CollisionX/Y/Z(e, i)` | where |
| `CollisionNX/NY/NZ(e, i)` | the direction the hit surface faces |

```basic
; is the player standing on the ground? A hit whose surface faces up.
Function OnGround(player)
    For i = 1 To CountCollisions(player)
        If CollisionNY(player, i) > 0.7 Then Return True
    Next
    Return False
End Function
```

```basic
; collect coins: type 3, removed when touched
Const TYPE_PLAYER = 1, TYPE_COIN = 3
player = CreateSphere()
Collisions TYPE_PLAYER, TYPE_COIN, 1, 1
score = 0
; each frame, after UpdateWorld:
coin = EntityCollided(player, TYPE_COIN)
If coin Then FreeEntity coin : score = score + 1
```

## Picking

**Picking** finds the first pickable entity along a line. First mark the
entities that can be picked with `EntityPickMode e, mode` (1 sphere, 2
real shape, 3 box):

```basic
Graphics3D 1280, 720
camera = CreateCamera()
light = CreateLight()
For i = 1 To 5
    target = CreateCube()
    PositionEntity target, i * 3 - 9, 0, 10
    EntityPickMode target, 2
Next

While Not KeyHit(KEY_ESCAPE)
    If MouseHit(1)
        hit = CameraPick(camera, MouseX(), MouseY())    ; what is under the mouse?
        If hit Then EntityColor hit, 255, 0, 0
    EndIf
    RenderWorld
    Flip
Wend
```

| Command | Picks |
|---|---|
| `CameraPick(camera, x, y)` | under a point on the screen (clicking on things) |
| `EntityPick(e, range)` | straight ahead of an entity (a gun, a sensor) |
| `LinePick(x, y, z, dx, dy, dz [, radius])` | along any line |
| `EntityVisible(a, b)` | 1 if nothing blocks the line between two entities (line of sight) |

All return the entity hit, or 0. `PickedX()`, `PickedY()`, `PickedZ()`
give where it was hit, and `PickedNX()`... the direction of the surface
there: put a bullet hole at the point, facing that way.

## Physics

**Physics** moves objects realistically by itself: gravity pulls them,
they collide, bounce, slide, roll and stack. Make an entity a physics
**body** with `EntityPhysics`:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 6, -14
CameraLookAt camera, 0, 2, 0
light = CreateLight()
TurnEntity light, 50, 30, 0

ground = CreateCube()
ScaleEntity ground, 20, 0.5, 20
EntityPhysics ground, PHYS_STATIC            ; doesn't move

For y = 0 To 4                               ; a tower of crates
    crate = CreateCube()
    ScaleEntity crate, 0.5, 0.5, 0.5
    PositionEntity crate, 0, 1 + y * 1.01, 0
    EntityPhysics crate, PHYS_DYNAMIC, SHAPE_BOX, 5      ; 5 kg
Next

ball = CreateSphere(16)
PositionEntity ball, -10, 2, 0
EntityPhysics ball, PHYS_DYNAMIC, SHAPE_SPHERE, 20

While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_SPACE) Then ApplyImpulse ball, 300, 20, 0   ; knock the tower down
    UpdateWorld
    RenderWorld
    Flip
Wend
```

`EntityPhysics entity, type [, shape, mass]`:

| Type | Moves |
|---|---|
| `PHYS_DYNAMIC` | by physics |
| `PHYS_STATIC` | never: floors, walls, levels |
| `PHYS_KINEMATIC` | only where you move it, pushing dynamic bodies out of the way: lifts, doors |

| Shape | For |
|---|---|
| `SHAPE_AUTO` | chosen from the entity |
| `SHAPE_BOX`, `SHAPE_SPHERE`, `SHAPE_CAPSULE`, `SHAPE_CYLINDER` | simple shapes, the fastest |
| `SHAPE_HULL` | a dynamic body shaped like the model (its outline, without dents) |
| `SHAPE_MESH` | the exact model; static bodies only (levels) |

A physics body can still be moved with `PositionEntity` and the other
entity commands. To push it the physical way:

| Command | Does |
|---|---|
| `ApplyImpulse e, x, y, z` | a sudden kick: a hit, a jump, an explosion |
| `ApplyForce e, x, y, z` | a steady push, while it is applied each frame: engines, wind |
| `ApplyTorque e, x, y, z` | a twist |
| `SetVelocity e, x, y, z` | sets its speed directly |
| `VelocityX/Y/Z(e)` | how fast it moves |
| `EntityMass`, `EntityFriction`, `EntityBounce`, `EntityDamping`, `EntityGravityScale` | how it behaves |
| `LockRotation e, ...` | stops it tipping over (characters) |
| `EntityTouching(a, b)` | 1 while two bodies touch |
| `PhysicsRayCast(x, y, z, dx, dy, dz)` | the first body along a line |
| `PhysicsGravity x, y, z` | the world's gravity |

## Joints

**Joints** connect two bodies: a door on a hinge, a wheel on an axle, a
chain of links.

```basic
post = CreateCube()
EntityPhysics post, PHYS_STATIC
panel = CreateCube()
ScaleEntity panel, 1, 2, 0.1
PositionEntity panel, 1, 0, 0
EntityPhysics panel, PHYS_DYNAMIC, SHAPE_BOX, 10

; a door hinged at the post, turning about the upright (y) axis
door = CreateHingeJoint(post, panel, 0, 0, 0, 0, 1, 0)
JointLimits door, -100, 100        ; it opens 100 degrees either way
JointTarget door, 90               ; swings open, like a spring
```

| Command | Makes |
|---|---|
| `CreateHingeJoint(a, b, x, y, z, ax, ay, az)` | a hinge at a point, turning about an axis |
| `CreateBallJoint(a, b, x, y, z)` | a ball joint that turns any way: chains, ropes |
| `JointLimits j, min, max` | how far it can turn |
| `JointMotor j, speed [, max_force]` | turns it like a motor: wheels, fans |
| `JointTarget j, target [, stiffness, max_force]` | pulls it towards an angle like a spring |

The same commands with `2D` on the end work on 2D physics bodies.

## Ragdolls

A **ragdoll** turns an animated character into connected physics bodies
that fall and flop realistically, for when a character is knocked over:

```basic
fox = LoadAnimMesh("examples/media/Fox.glb")
Animate fox, 1
CreateRagdoll fox                 ; builds the bodies; still animating
; when the fox is hit:
RagdollEnable fox                 ; goes limp and falls
RagdollImpulse fox, 0, 3, 5       ; knocked backwards
; to get up again:
RagdollEnable fox, False
Animate fox, 1
```

`examples/joints.nb` and `examples/physics.nb` show joints, motors and
ragdolls in action.

[Contents](../MANUAL.md) · Previous: [18. 3D models and looks](18-3d-models.md) · Next: [20. AI and pathfinding](20-ai.md)
