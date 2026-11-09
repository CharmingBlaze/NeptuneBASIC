# 14. 2D games

[Contents](../MANUAL.md) · Previous: [13. 2D graphics](13-2d-graphics.md) · Next: [15. Input](15-input.md)

The previous chapter drew pictures. This one adds what 2D games are made
of: levels built from tiles, a camera that follows the player,
animations, physics, and simple buttons and sliders.
`examples/platformer2d.nb` puts all of it together in a playable level.

- [Tile maps](#tile-maps)
- [Autotiling](#autotiling)
- [A scrolling camera](#a-scrolling-camera)
- [Animation](#animation)
- [2D physics](#2d-physics)
- [Simple buttons and sliders](#simple-buttons-and-sliders)

## Tile maps

A **tile map** is a grid of cells, each showing one frame of a tile
image. Levels made of tiles are quick to build, easy to change, and cheap
to draw (only the tiles on screen are drawn).

```basic
Graphics 800, 600
tiles = LoadAnimImage("tiles.png", 32, 32, 0, 48)   ; 48 tiles of 32 x 32
map = CreateTileMap(tiles, 100, 20)                  ; 100 x 20 cells, all empty

For x = 0 To 99
    SetTile map, x, 19, 1        ; a floor of tile 1 along the bottom row
Next
SetTile map, 10, 15, 5           ; a block

While Not KeyHit(KEY_ESCAPE)
    Cls
    DrawTileMap map, 0, 0        ; top-left corner at 0, 0
    Flip
Wend
```

| Command | Does |
|---|---|
| `CreateTileMap(image, width, height)` | a map of width × height empty cells |
| `SetTile map, x, y, tile` / `GetTile(map, x, y)` | sets / reads the frame in a cell (-1 is empty) |
| `DrawTileMap map, x, y` | draws it with its top-left corner at x, y |
| `TileAt(map, px, py)` | the tile under a point in pixels: how the game knows what is solid |
| `TileMapCellX(map, px)`, `TileMapCellY(map, py)` | the cell under a point |
| `SaveTileMap(map, file$)`, `LoadTileMap(file$, image)` | saves and loads a map |

Testing the ground under the player:

To draw levels in the Tiled editor and animate characters in Aseprite, see
[2D levels and sprites from other tools](../TOOLS2D.md).

```basic
; px, py is the player's position, relative to the map's corner
map = CreateTileMap(CreateImage(32, 32, 4), 50, 20)
px = 100 : py = 300
If TileAt(map, px, py + 17) >= 0 Then Print "standing on something"
```

## Autotiling

Drawing edges and corners tile by tile is slow work. **Autotiling**
picks each tile's frame from its neighbours: paint a cell with a
*terrain* and it chooses the right edge, corner or middle piece, and
fixes up the cells around it.

```basic
tiles = LoadAnimImage("grass_tiles.png", 32, 32, 0, 16)
map = CreateTileMap(tiles, 60, 20)
TileMapTerrain map, 1, 0, 16      ; terrain 1 uses frames 0-15, the 16-frame layout
For x = 5 To 20
    AutoTile map, x, 12, 1        ; paint a platform; edges are chosen for you
Next
```

`TileMapTerrain map, terrain, first_frame, mode` sets the layout: `16`
looks at four neighbours (16 frames: one for each combination of
up, right, down and left); `47` looks at all eight for the 47-frame
"blob" layout with inner corners.

## A scrolling camera

`Camera2D x, y [, zoom, rotation]` makes 2D drawing use **world
coordinates**: the world point x, y is drawn at the centre of the screen.
Point it at the player every frame and the world scrolls. Turn it off to
draw the score in screen coordinates on top.

```basic
Graphics 800, 600
map = CreateTileMap(CreateImage(32, 32, 4), 200, 20)
playerX# = 400 : playerY# = 300
While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_RIGHT) Then playerX = playerX + 5
    If KeyDown(KEY_LEFT) Then playerX = playerX - 5
    Cls
    Camera2D playerX, playerY, 1.5      ; follow the player, zoomed in 1.5 times
    DrawTileMap map, 0, 0
    Oval playerX - 16, playerY - 16, 32, 32
    Camera2DEnable False                ; back to screen pixels
    Text 10, 10, "Distance: " + Int(playerX)
    Flip
Wend
```

`ScreenToWorld2DX/Y` turn a screen point (such as the mouse) into a
world point, and `WorldToScreen2DX/Y` the other way.

## Animation

An **Anim2D** plays the frames of an animation image for you:

```basic
Graphics 800, 600
hero = LoadAnimImage("hero.png", 32, 48, 0, 8)
walk = CreateAnim2D(hero, 0, 3, 10)      ; frames 0-3, 10 a second, looping
jump = CreateAnim2D(hero, 4, 7, 12, 3)   ; frames 4-7, played once
current = walk
While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_SPACE) Then current = jump : Anim2DPlay jump, 4, 7, 12, 3
    If current = jump And Not Anim2DPlaying(jump) Then current = walk
    Cls
    DrawAnim2D current, 400, 300
    Flip
Wend
```

`mode` is 1 to loop (the default), 2 to go back and forth, 3 to play
once. `Anim2DFrame(anim)` says which frame is showing, and
`Anim2DPause anim, True` pauses it.

## 2D physics

The physics engine moves bodies under gravity, makes them collide,
bounce, slide and roll, so you don't have to work it out yourself.
Positions are in pixels, with y down.

```basic
Graphics 800, 600
Physics2DGravity 0, 900                  ; downwards, in pixels per second per second

floor = CreateBody2D(1, 400, 580)        ; type 1: static, never moves
Body2DBox floor, 800, 40

For i = 0 To 9
    crate = CreateBody2D(0, 300 + i * 25, 100 - i * 40)   ; type 0: dynamic
    Body2DBox crate, 30, 30
    Body2DBounce crate, 0.3
Next

ball = CreateBody2D(0, 500, 50)
Body2DCircle ball, 15
Body2DBounce ball, 0.8                   ; a bouncy ball

While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_SPACE) Then ApplyImpulse2D ball, 0, -400   ; kick it upwards
    Cls
    Color 255, 255, 255
    DrawBodies2D                         ; outlines of every body
    Flip
Wend
```

The physics world steps by itself every frame.

**Bodies** are created with `CreateBody2D(type, x, y)`:

| Type | Moves | Use for |
|---|---|---|
| 0 dynamic | under forces and gravity | players, crates, balls |
| 1 static | never | ground, walls |
| 2 kinematic | only where you move it | moving platforms, doors |

**Shapes** give a body its size: `Body2DBox`, `Body2DCircle`,
`Body2DCapsule` (best for characters, which slide over bumps),
`Body2DPolygon` and `Body2DSegment`. `TileMapBody2D(map)` makes the solid
tiles of a tile map into one static body.

**Moving bodies**:

| Command | Does |
|---|---|
| `SetVelocity2D body, vx, vy` | sets its speed directly (good for player control) |
| `ApplyImpulse2D body, x, y` | a sudden kick (jumps, explosions) |
| `ApplyForce2D body, x, y` | a steady push, for this step |
| `PositionBody2D body, x, y`, `RotateBody2D body, angle` | moves it instantly |
| `Body2DX(body)`, `Body2DY(body)`, `Body2DAngle(body)`, `Body2DVX`, `Body2DVY` | where it is and how fast it moves |

**Materials**: `Body2DFriction` (0 is ice), `Body2DBounce` (0 to 1),
`Body2DDensity` (heavier), `Body2DFixedRotation body, True` (characters
that shouldn't tip over), `Body2DGravityScale` (0 floats).

**Contacts**: `Body2DTouching(body, other)` is true while two bodies
touch (other 0: anything). `Body2DSensor body, True` makes a body that
detects touches without blocking anything: coins, checkpoints, triggers.
`RayCast2D(x1, y1, x2, y2)` finds the first body along a line (line of
sight, or "is there ground below me").

A platformer player:

```basic
Physics2DGravity 0, 1200
player = CreateBody2D(0, 100, 300)
Body2DCapsule player, 20, 30
Body2DFixedRotation player, True
; each frame:
speed = (KeyDown(KEY_RIGHT) - KeyDown(KEY_LEFT)) * 250
SetVelocity2D player, speed, Body2DVY(player)          ; keep falling speed
onGround = RayCast2D(Body2DX(player), Body2DY(player) + 16, Body2DX(player), Body2DY(player) + 22) <> 0
If onGround And KeyHit(KEY_SPACE) Then ApplyImpulse2D player, 0, -500
```

Joints connect 2D bodies (hinges, springs, ropes): see the `...2D`
commands under [Joints and ragdolls](../COMMANDS.md#joints-and-ragdolls).

## Simple buttons and sliders

The `UI...` commands draw simple controls and handle the mouse, with one
call each frame. They are ideal for debug panels, settings and level
editors. (For full windows, lists and menus, see
[chapter 24](24-gui.md).)

```basic
Graphics 800, 600
volume# = 0.8
fullscreen = False
name$ = "Player"
While Not KeyHit(KEY_ESCAPE)
    Cls
    UIPanel 20, 20, 260, 200, "Settings"
    volume = UISlider(40, 70, 200, volume, 0, 1)
    fullscreen = UICheckBox(40, 110, "Fullscreen", fullscreen)
    name = UITextBox(40, 140, 200, name)
    If UIButton(40, 180, 100, 28, "Apply") Then SetFullscreen fullscreen
    UILabel 320, 30, "Volume " + Int(volume * 100) + "%"
    Flip
Wend
```

Each control returns its new value: assign it back to the variable.
`UIButton` is true in the frame it is clicked. `UIHover()` is true when
the mouse is over a control, so the game can ignore that click.
`UITheme r, g, b` sets the accent colour.

[Contents](../MANUAL.md) · Previous: [13. 2D graphics](13-2d-graphics.md) · Next: [15. Input](15-input.md)
