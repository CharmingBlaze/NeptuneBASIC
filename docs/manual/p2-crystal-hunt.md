# Project 2: Crystal Hunt

[Contents](../MANUAL.md) · Previous: [Project 1: Star Catcher](p1-star-catcher.md)

Now a game in 3D. You steer a ball around a field dotted with stone pillars,
collecting spinning crystals before the clock runs out.

It helps to have done [Project 1](p1-star-catcher.md) and read
[3D basics](17-3d-basics.md). As before, no media files are needed: everything
is built from simple shapes, and you run the program after each step.

- [Step 1: A world to look at](#step-1-a-world-to-look-at)
- [Step 2: A player that drives](#step-2-a-player-that-drives)
- [Step 3: A camera that follows](#step-3-a-camera-that-follows)
- [Step 4: Crystals to collect](#step-4-crystals-to-collect)
- [Step 5: Pillars you can't walk through](#step-5-pillars-you-cant-walk-through)
- [Step 6: A clock and an ending](#step-6-a-clock-and-an-ending)
- [The whole game](#the-whole-game)
- [Make it yours](#make-it-yours)

## Step 1: A world to look at

A 3D scene needs three things before you can see anything: a **camera**, a
**light**, and something to look at.

```basic
; Crystal Hunt, step 1
Graphics3D 1280, 720
AppTitle "Crystal Hunt"

camera = CreateCamera()
PositionEntity camera, 0, 12, -20
RotateEntity camera, 30, 0, 0
CameraClsColor camera, 120, 170, 220

AmbientLight 110, 110, 130
sun = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity sun, 55, 30, 0

ground = CreatePlane()
EntityColor ground, 70, 130, 80

While Not KeyHit(KEY_ESCAPE)
    UpdateWorld
    RenderWorld
    Flip
Wend
```

Positions are `x, y, z`: x is right, y is up, z is forward. The camera is 12
up and 20 back. `RotateEntity camera, 30, 0, 0` tips it to look down, because
a positive pitch looks down. `AmbientLight` is the soft light that reaches
everywhere, so the shadows aren't pure black.

**Run it.** A green field under a blue sky.

## Step 2: A player that drives

Add a sphere for the player, before the loop:

<!-- nocheck -->
```basic
Const TURN_SPEED = 150          ; degrees per second
Const WALK_SPEED = 11           ; units per second

player = CreateSphere()
EntityColor player, 255, 170, 60
PositionEntity player, 0, 1, 0
```

Inside the loop, measure the time as you did in Project 1, then drive:

<!-- nocheck -->
```basic
    now = MilliSecs()
    dt# = Min((now - previous) / 1000.0, 0.05)
    previous = now

    If KeyDown(KEY_LEFT) Then TurnEntity player, 0, TURN_SPEED * dt, 0
    If KeyDown(KEY_RIGHT) Then TurnEntity player, 0, -TURN_SPEED * dt, 0
    If KeyDown(KEY_UP) Then MoveEntity player, 0, 0, WALK_SPEED * dt
    If KeyDown(KEY_DOWN) Then MoveEntity player, 0, 0, -WALK_SPEED * dt
```

Remember to put `previous = MilliSecs()` just before the loop.

`MoveEntity` moves in the direction the entity is *facing*, which is exactly
what driving needs. `PositionEntity` would place it at a spot in the world
instead. Positive yaw turns left.

**Run it.** The ball drives around. The camera stays put, so it soon escapes.

## Step 3: A camera that follows

Here is one of the most useful ideas in 3D. Make the camera a **child** of
the player, and wherever the player goes and turns, the camera goes too.
Replace the camera lines with these, after the player is created:

<!-- nocheck -->
```basic
camera = CreateCamera(player)
PositionEntity camera, 0, 5, -11
RotateEntity camera, 18, 0, 0
CameraClsColor camera, 120, 170, 220
```

A child's position is measured from its parent, so this means "5 above and 11
behind the player", whichever way the player faces.

Also stop the player leaving the field, at the end of the driving code:

<!-- nocheck -->
```basic
    PositionEntity player, Clamp(EntityX(player), -38, 38), 1, Clamp(EntityZ(player), -38, 38)
```

**Run it.** Now you are behind the ball, and the world turns around you.

## Step 4: Crystals to collect

Twelve crystals is a job for a type, as stars were in Project 1. This one
holds the entity. Put this at the top of the program:

<!-- nocheck -->
```basic
Const CRYSTALS = 12

Type Crystal
    Field mesh
End Type

Global player, found
```

The `Global` means functions can use `player`. Now write two functions:

<!-- nocheck -->
```basic
Function ScatterCrystals()
    For n = 1 To CRYSTALS
        c.Crystal = New Crystal
        c\mesh = CreateCone()
        ScaleEntity c\mesh, 0.6, 1, 0.6
        PositionEntity c\mesh, Rnd(-34, 34), 1.2, Rnd(-34, 34)
        EntityColor c\mesh, 90, 230, 255
        EntityShininess c\mesh, 1
    Next
End Function

Function UpdateCrystals(dt#)
    For c.Crystal = Each Crystal
        TurnEntity c\mesh, 0, 120 * dt, 0
        If EntityDistance(player, c\mesh) < 2 Then
            FreeEntity c\mesh
            Delete c
            found = found + 1
        EndIf
    Next
End Function
```

Call `ScatterCrystals` before the loop and `UpdateCrystals dt` inside it.

Notice that a collected crystal needs **two** removals. `FreeEntity` takes the
cone out of the 3D world, and `Delete` removes our record of it. Forget the
first and the crystal stays on screen. Forget the second and you'd try to
measure the distance to something that no longer exists, which is an error.

Show the count after `RenderWorld`, so it is drawn on top of the 3D picture:

<!-- nocheck -->
```basic
    Color 255, 255, 255
    Text 16, 16, "Crystals: " + found + " / " + CRYSTALS
```

**Run it.** Drive into crystals and watch the count go up.

## Step 5: Pillars you can't walk through

Collisions work with numbered groups. You say which group each entity is in,
and which groups stop which. Add:

<!-- nocheck -->
```basic
Const TYPE_PLAYER = 1
Const TYPE_PILLAR = 2
```

After creating the player:

<!-- nocheck -->
```basic
EntityType player, TYPE_PLAYER
EntityRadius player, 1
Collisions TYPE_PLAYER, TYPE_PILLAR, 2, 2
```

`Collisions` reads as: when the player meets a pillar, detect it as a sphere
against a box (method 2) and slide along it (response 2). Collisions are
worked out by `UpdateWorld`, which is why it has to be in the loop.

Then scatter some pillars, before the loop:

<!-- nocheck -->
```basic
For n = 1 To 14
    pillar = CreateCube()
    ScaleEntity pillar, 1.5, 4, 1.5
    PositionEntity pillar, Rnd(-32, 32), 4, Rnd(-32, 32)
    EntityColor pillar, 120, 110, 100
    EntityType pillar, TYPE_PILLAR
Next
```

A cube is 2 units across, so scaling it by 4 makes it 8 tall, and placing it
at a height of 4 stands it on the ground.

**Run it.** Drive at a pillar. You slide around it rather than passing through.

## Step 6: A clock and an ending

Finally, the part that makes it a game. Add `TIME_LIMIT`, `timeLeft#` and
`playing`:

<!-- nocheck -->
```basic
Const TIME_LIMIT = 45
Global timeLeft#, playing
```

To start again, everything has to go back to the beginning. Collect that in
one function, and call it before the loop:

<!-- nocheck -->
```basic
Function StartRound()
    ScatterCrystals
    PositionEntity player, 0, 1, 0
    ResetEntity player
    RotateEntity player, 0, 0, 0
    found = 0
    timeLeft = TIME_LIMIT
    playing = True
End Function
```

`ResetEntity` matters here. Collisions look at how far an entity moved since
the last `UpdateWorld`, so without it, jumping back to the start counts as a
long journey through anything in the way.

`ScatterCrystals` must now clear away crystals left from the last round. Add
this at its start:

<!-- nocheck -->
```basic
    For c.Crystal = Each Crystal
        FreeEntity c\mesh
    Next
    Delete Each Crystal
```

The game only moves while `playing` is true:

<!-- nocheck -->
```basic
    If playing Then
        ; the driving code and UpdateCrystals go here
        timeLeft = timeLeft - dt
        If found = CRYSTALS Or timeLeft <= 0 Then playing = False
    ElseIf KeyHit(KEY_SPACE) Then
        StartRound
    EndIf
```

And after `RenderWorld`, say how it went:

<!-- nocheck -->
```basic
    Text 16, 36, "Time: " + Int(Ceil(Max(timeLeft, 0)))
    If Not playing Then
        If found = CRYSTALS Then
            Text GraphicsWidth() / 2, 320, "YOU FOUND THEM ALL!", True
        Else
            Text GraphicsWidth() / 2, 320, "OUT OF TIME", True
        EndIf
        Text GraphicsWidth() / 2, 350, "Press Space to play again", True
    EndIf
```

**Run it.** You can win, lose and play again.

## The whole game

In the finished program the driving code and the world building have moved
into functions, so the main loop reads like a summary of the game.

```basic
; Crystal Hunt: collect every crystal before time runs out.

Const CRYSTALS = 12
Const TIME_LIMIT = 45
Const TURN_SPEED = 150
Const WALK_SPEED = 11
Const WORLD_EDGE = 38

Const TYPE_PLAYER = 1
Const TYPE_PILLAR = 2

Type Crystal
    Field mesh
End Type

Global player, camera
Global found, timeLeft#, playing

Function BuildWorld()
    CameraClsColor camera, 120, 170, 220
    AmbientLight 110, 110, 130
    sun = CreateLight(LIGHT_DIRECTIONAL)
    TurnEntity sun, 55, 30, 0

    ground = CreatePlane()
    EntityColor ground, 70, 130, 80

    For n = 1 To 14
        pillar = CreateCube()
        ScaleEntity pillar, 1.5, 4, 1.5
        PositionEntity pillar, Rnd(-32, 32), 4, Rnd(-32, 32)
        EntityColor pillar, 120, 110, 100
        EntityType pillar, TYPE_PILLAR
    Next
End Function

Function ScatterCrystals()
    For c.Crystal = Each Crystal
        FreeEntity c\mesh
    Next
    Delete Each Crystal
    For n = 1 To CRYSTALS
        c.Crystal = New Crystal
        c\mesh = CreateCone()
        ScaleEntity c\mesh, 0.6, 1, 0.6
        PositionEntity c\mesh, Rnd(-34, 34), 1.2, Rnd(-34, 34)
        EntityColor c\mesh, 90, 230, 255
        EntityShininess c\mesh, 1
    Next
End Function

Function StartRound()
    ScatterCrystals
    PositionEntity player, 0, 1, 0
    ResetEntity player
    RotateEntity player, 0, 0, 0
    found = 0
    timeLeft = TIME_LIMIT
    playing = True
End Function

Function UpdatePlayer(dt#)
    If KeyDown(KEY_LEFT) Then TurnEntity player, 0, TURN_SPEED * dt, 0
    If KeyDown(KEY_RIGHT) Then TurnEntity player, 0, -TURN_SPEED * dt, 0
    If KeyDown(KEY_UP) Then MoveEntity player, 0, 0, WALK_SPEED * dt
    If KeyDown(KEY_DOWN) Then MoveEntity player, 0, 0, -WALK_SPEED * dt
    PositionEntity player, Clamp(EntityX(player), -WORLD_EDGE, WORLD_EDGE), 1, Clamp(EntityZ(player), -WORLD_EDGE, WORLD_EDGE)
End Function

Function UpdateCrystals(dt#)
    For c.Crystal = Each Crystal
        TurnEntity c\mesh, 0, 120 * dt, 0
        If EntityDistance(player, c\mesh) < 2 Then
            FreeEntity c\mesh
            Delete c
            found = found + 1
        EndIf
    Next
End Function

Graphics3D 1280, 720
AppTitle "Crystal Hunt"

player = CreateSphere()
EntityColor player, 255, 170, 60
EntityType player, TYPE_PLAYER
EntityRadius player, 1
Collisions TYPE_PLAYER, TYPE_PILLAR, 2, 2

camera = CreateCamera(player)
PositionEntity camera, 0, 5, -11
RotateEntity camera, 18, 0, 0

BuildWorld
StartRound

previous = MilliSecs()
While Not KeyHit(KEY_ESCAPE)
    now = MilliSecs()
    dt# = Min((now - previous) / 1000.0, 0.05)
    previous = now

    If playing Then
        UpdatePlayer dt
        UpdateCrystals dt
        timeLeft = timeLeft - dt
        If found = CRYSTALS Or timeLeft <= 0 Then playing = False
    ElseIf KeyHit(KEY_SPACE) Then
        StartRound
    EndIf

    UpdateWorld
    RenderWorld

    Color 255, 255, 255
    Text 16, 16, "Crystals: " + found + " / " + CRYSTALS
    Text 16, 36, "Time: " + Int(Ceil(Max(timeLeft, 0)))
    If Not playing Then
        If found = CRYSTALS Then
            Text GraphicsWidth() / 2, 320, "YOU FOUND THEM ALL!", True
        Else
            Text GraphicsWidth() / 2, 320, "OUT OF TIME", True
        EndIf
        Text GraphicsWidth() / 2, 350, "Press Space to play again", True
    EndIf
    Flip
Wend
```

## Make it yours

1. Tune `WALK_SPEED`, `TIME_LIMIT` and the number of crystals.
2. Make the crystals bob up and down: give `PositionEntity` a height using `Sin(MilliSecs() * 0.2)`.
3. Add a rare golden crystal worth extra time. It is one more `Field`.
4. Add a hazard that drives towards the player using `PointEntity` and `MoveEntity`.
5. Replace the ball with a real model using `LoadAnimMesh` ([chapter 18](18-3d-models.md)).
6. Use a proper walking character with jumping and stairs: see [Character controllers](../CHARACTERS.md).

[Contents](../MANUAL.md) · Previous: [Project 1: Star Catcher](p1-star-catcher.md)
