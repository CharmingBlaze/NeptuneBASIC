# Character controllers

A character controller moves a player or enemy through the world the way
games expect: it runs at a set speed, walks up slopes and stairs, stays stuck
to the ground going downhill, rides moving platforms, and forgives a jump
pressed a moment too late. NeptuneBASIC has one for 3D and one for 2D. They
share the same commands.

Try them first:

```bash
neptune run examples/charcontrol3d.nb
```

```bash
neptune run examples/charcontrol2d.nb
```

## Why not just use a physics body?

A rigid body (`EntityPhysics`) is pushed by forces, so it slides on slopes,
trips over edges, bounces off walls and is hard to stop exactly. That suits a
crate. A character needs to do just what the player asked, at once. A
controller doesn't use forces. Each frame it asks the world "what would I hit
going this way?", moves as far as it can and slides along whatever it met.

The world still needs to be solid to physics. Controllers collide with
`EntityPhysics` and 2D bodies: static scenery, terrain, `col_` objects from
`LoadLevelFile`, and tile maps with `TileMapBody2D`. They don't see objects
that only have `EntityType`. Triggers (sensors) never block them.

## The basic loop

You say what the player wants. The controller decides what actually happens.

```basic
Graphics3D 1280, 720, 32, 2

floor = CreateCube()
ScaleEntity floor, 50, 0.5, 50
PositionEntity floor, 0, -0.5, 0
EntityPhysics floor, 2

hero = CreateCube()
PositionEntity hero, 0, 1, 0
player = CreateCharController(hero, 0.4, 1.8)

camera = CreateCamera()
PositionEntity camera, 0, 4, -10

While Not KeyHit(KEY_ESCAPE)
    CharMove player, KeyDown(KEY_D) - KeyDown(KEY_A), 0, KeyDown(KEY_W) - KeyDown(KEY_S)
    If KeyHit(KEY_SPACE) Then CharJump player
    UpdateChar player

    UpdateWorld
    RenderWorld
    Flip
Wend
```

In 2D it is the same, except that the controller makes its own body and you
draw it wherever it is:

```basic
Graphics 960, 540, 0, 2

floor = CreateBody2D(1, 480, 520)
Body2DBox floor, 960, 40

player = CreateCharController2D(100, 400, 26, 44)

While Not KeyHit(KEY_ESCAPE)
    CharMove player, KeyDown(KEY_RIGHT) - KeyDown(KEY_LEFT)
    If KeyHit(KEY_SPACE) Then CharJump player
    UpdateChar player

    Cls
    Rect CharX(player) - 13, CharY(player) - CharHeight(player), 26, CharHeight(player)
    Flip
Wend
```

**Where is the character?** Both report the middle of the **feet**. In 3D that
is the entity's origin, which suits most models. In 2D, put the image's handle
at the bottom: `HandleImage hero, ImageWidth(hero) / 2, ImageHeight(hero)`.
Feet stay on the ground when the character crouches, and the middle of the
body would not.

Call `UpdateChar` once per frame, after giving it input. It measures the time
since the last update, or you can pass the seconds yourself to keep a fixed
step. Speeds are per second, so the game runs the same on every computer.

## Telling it what to do

| Command | What it does |
|---|---|
| `CharMove c, x, y, z` | Which way to go, in world directions, up to 1 long. Use a value below 1 to walk slowly (a gamepad stick does this for free). In 2D, `CharMove c, x`. `y` only counts when flying or climbing |
| `CharWalk c, ahead, side, yaw` | 3D: moves relative to a direction, usually the camera's yaw, so W is always away from the camera |
| `CharJump c` | Jump (when the button is **pressed**) |
| `CharHoldJump c, held` | Whether the button is **down**. Letting go early makes a shorter jump. If you never call this, every jump is full height |
| `CharSprint c, on`, `CharCrouch c, on` | Change speed. Crouching also lowers the head, and it stays down until there's room to stand |
| `CharDash c, x, y, z, speed, seconds` | A burst in a direction, ignoring gravity |
| `CharImpulse c, x, y, z` | A shove that fades: explosions and getting hit |
| `CharDrop c` | Fall through one-way platforms |
| `CharMode c, mode` | `CHAR_WALK`, `CHAR_FLY` (no gravity, all directions, still solid: ladders, swimming, jetpacks, top-down games) or `CHAR_NOCLIP` (through everything: testing and spectating) |
| `PositionChar c, x, y, z` | Teleport. For a 3D character, `PositionEntity` on its entity also works |
| `SetCharVelocity c, x, y, z` | Sets its speed directly: launch pads, for example |

## Asking what happened

The answers cover the last `UpdateChar`.

| Command | Tells you |
|---|---|
| `CharState$(c)` | One word to pick an animation by: `idle`, `walk`, `run`, `jump`, `fall`, `crouch`, `crawl`, `slide`, `wallslide`, `dash` or `fly` |
| `CharGrounded(c)` | Standing on something walkable |
| `CharLanded(c)`, `CharImpact#(c)` | It just landed, and how fast it was falling: dust, sounds and fall damage |
| `CharLeftGround(c)` | It just walked off an edge |
| `CharJumped(c)`, `CharWallJumped(c)` | A jump really happened: play the sound here, not when the key is pressed |
| `CharStepped(c)` | It stepped up a stair |
| `CharOnWall(c)`, `CharWallSliding(c)`, `CharWallNX#(c)`... | Pressing against a wall, and which way the wall faces |
| `CharOnCeiling(c)` | It bumped its head |
| `CharSliding(c)` | On a slope too steep to stand on |
| `CharCrouching(c)`, `CharDashing(c)` | Whether it is doing these now |
| `CharGroundObject(c)`, `CharGroundNX#(c)`... | The entity or 2D body underfoot, and which way the ground faces |
| `CharX#(c)`, `CharVX#(c)`, `CharHeight#(c)`... | Position, velocity and current height |
| `CharFacing(c)` | 2D: -1 or 1, the way it last moved. Ready for `DrawImageEx`'s x scale |
| `CharAirJumpsLeft(c)` | Air jumps still available |
| `CharBody(c)` | 2D: its body, for `Body2DTouching` and the like |

## Tuning the feel

Read or change any of these by name with `CharSet c, "name", value` and
`CharGet#(c, "name")`. In 3D they are in metres, and in 2D in pixels.

| Setting | 3D default | 2D default | What it changes |
|---|---|---|---|
| `speed` | 5 | 220 | Running speed |
| `sprint` | 1.6 | 1.5 | Speed multiplier when sprinting |
| `crouch` | 0.5 | 0.5 | Speed multiplier when crouching |
| `crouch_height` | 55% of height | 60% of height | How tall it is when crouching |
| `acceleration` | 50 | 2400 | How fast it reaches full speed on the ground |
| `friction` | 60 | 3000 | How fast it stops on the ground |
| `air_acceleration` | 18 | 1500 | Steering in the air |
| `air_friction` | 4 | 300 | Slowing in the air with no input |
| `gravity` | 20 | 1500 | Its own gravity (`PhysicsGravity` doesn't affect it) |
| `max_fall` | 40 | 900 | Fastest it can fall |
| `jump_speed` | 8 | 580 | Jump height: height = speed × speed ÷ (2 × gravity) |
| `jump_cut` | 0.5 | 0.5 | How much of the rise remains when the button is released |
| `air_jumps` | 0 | 0 | Extra jumps in the air: 1 is a double jump |
| `coyote_time` | 0.1 | 0.1 | Seconds after leaving an edge when a jump still works |
| `jump_buffer` | 0.12 | 0.12 | Seconds before landing when a press still counts |
| `max_slope` | 46 | 46 | Steepest walkable slope, in degrees |
| `step_height` | 0.35 | 0 | Tallest stair it walks straight up |
| `snap` | 0.3 | 8 | How far down it clings to the ground going downhill and over bumps |
| `wall_slide` | 0 (off) | 0 (off) | Falling speed when pressing against a wall |
| `wall_jump` | 0 (off) | 0 (off) | 1 allows jumping off walls |
| `wall_jump_up`, `wall_jump_out` | 8, 6 | 520, 300 | Speed of a wall jump, upward and away |
| `wall_jump_lock` | 0.2 | 0.15 | Seconds after a wall jump when steering is ignored, so it really leaves |
| `push` | 1 | 1 | Strength against dynamic bodies. Bodies heavier than this move slower. 0 never pushes |
| `knock_decay` | 20 | 900 | How fast `CharImpulse` fades |
| `face_rate` | 0 (off) | 0 (off) | 3D: degrees a second its entity turns to face the way it walks |
| `skin` | 0.02 | 0.5 | Tiny gap kept from surfaces. You should rarely need to change it |

### Why these settings matter

**Coyote time** and the **jump buffer** forgive being a few frames off, and
they are why a jump feels fair. Without them, people swear they pressed the
button. **Jump cut** lets one button make small hops and big leaps.
**Acceleration and friction** give weight: very high is snappy like an arcade
game, lower feels heavy and realistic. **Air acceleration** lower than
acceleration makes a jump a commitment.

To design a jump, choose the height and how long it should take to reach the
top, then: `gravity = 2 × height ÷ time²` and `jump_speed = gravity × time`.

## Common recipes

### Moving platforms

Make them kinematic and move them. Characters standing on them are carried,
turning platforms included, and jumping off keeps the platform's momentum.

```basic
lift = CreateCube()
EntityPhysics lift, 3
; each frame:
PositionEntity lift, 0, 2 + Sin(MilliSecs() * 0.05) * 2, 5
```

### One-way platforms

Solid from above, passable from below. Call this after giving a 2D body its
shapes. `EntityOneWay` does the same for 3D. Other physics bodies still
collide with them as normal. Use `CharDrop` to fall through.

```basic
ledge = CreateBody2D(1, 300, 400)
Body2DBox ledge, 180, 12
Body2DOneWay ledge
```

### Ladders and swimming

A ladder is an area where gravity stops and up and down work.

```basic
If onLadder And moveY <> 0 Then CharMode player, CHAR_FLY
CharMove player, moveX, moveY
```

Go back to `CHAR_WALK` when the character leaves the area. For swimming, also
use a lower `speed` and `acceleration`.

### Top-down games

Use `CHAR_FLY` from the start. There's no gravity and walls still stop you.

```basic
CharMode player, CHAR_FLY
CharMove player, moveX, moveY
```

### Wall jumps

```basic
CharSet player, "wall_slide", 110
CharSet player, "wall_jump", 1
```

### Fall damage

```basic
If CharLanded(player) And CharImpact(player) > 15
    health = health - (CharImpact(player) - 15) * 4
End If
```

### Animation

`CharState` gives the word and you choose what it plays:

```basic
Select CharState(player)
    Case "idle" : Anim2DPlayTag anim, "idle"
    Case "walk", "run" : Anim2DPlayTag anim, "run"
    Case "jump", "fall" : Anim2DPlayTag anim, "jump"
End Select
```

In 3D, `SetAnimParam model, "speed", Sqr(CharVX(player)^2 + CharVZ(player)^2)`
feeds an animation blend. Set `face_rate` so the model turns to face its way.

### Enemies

The same controller works for anything that walks. Give it input from your AI
instead of the keyboard, for example towards the next point of a path from
`FindPath`.

## In a multiplayer game

Put a controller on a network template, and every player gets a character that
works like this one, with no other change to your game. Make the controller and
set its settings **before** `NetTemplate`, which takes it as the pattern for all
the copies (the handle you got from `CreateCharController` stops working then).

```basic
hero = CreateCube()
pattern = CreateCharController(hero, 0.4, 1.8)
CharSet pattern, "speed", 7
NetTemplate "hero", hero

me = NetSpawn("hero", 0, 1, 0)
player = NetCharacter(me)        ; use this with CharMove, CharJump, UpdateChar
```

Your own character moves the moment you press a key. The host re-runs your
buttons to check them, so nobody can give themselves more speed, and if the two
ever disagree your character quietly slides to where the host says it is.
`NetCorrections(player)` counts how often that has happened, and it should
stay low.

* **Only movement goes to the host:** `CharMove`, `CharWalk`, `CharJump`,
  `CharHoldJump`, `CharSprint`, `CharCrouch` and `CharDrop`. Anything else that
  moves a character, like `CharDash`, `CharImpulse` or `PositionChar`, is undone
  by the host unless the host does it too. Let the host do it: send it a
  `NetSend`, and call the command on `NetCharacter(entity)` there.
* **The host's `CharSet` reaches the player's computer.** Use it for power-ups.
* Bumping into another player can cause a small correction, because you see them
  a moment in the past.
* 3D characters only, for now. `NetSpawn` has no 2D version yet.

[`examples/netcharacters.nb`](../examples/netcharacters.nb) shows all of this.

## Things to know

* New physics objects are only seen after the next frame is drawn, because the
  physics world updates once a frame. Give a level one `RenderWorld` before
  starting a character close above it, or start it a little higher.
* A 3D character's collision shape is a capsule. A 2D one is a box by default
  (flat feet, crisp on tiles), or `CHAR_CAPSULE` as the fifth parameter of
  `CreateCharController2D`, which rides over small bumps.
* Other characters block a character, like any other solid.
* `FreeEntity` on a 3D character's entity also frees its controller.
* Dynamic bodies are pushed by simply walking into them, but they cannot stand
  in for moving platforms. Use kinematic bodies for those.
