# 12. The game loop

[Contents](../MANUAL.md) · Previous: [11. Organising programs](11-organising.md) · Next: [13. 2D graphics](13-2d-graphics.md)

Every game, from Pong to an open world, has the same shape: get things
ready, then go round a loop many times a second. Each time round, the
**game loop** reads the player's input, moves everything a little, and
draws a new picture. This chapter is about that loop.

- [Opening a window](#opening-a-window)
- [The loop](#the-loop)
- [Frames and Flip](#frames-and-flip)
- [Keeping time](#keeping-time)
- [Steady speed on every computer](#steady-speed-on-every-computer)
- [Game states](#game-states)
- [A complete small game](#a-complete-small-game)
- [Try it yourself](#try-it-yourself)

## Opening a window

| Command | Opens |
|---|---|
| `Graphics width, height [, depth, mode]` | a window for 2D drawing |
| `Graphics3D width, height [, depth, mode]` | a window for 3D (and 2D on top) |

`mode` is 0 or 2 for a window, 1 for fullscreen, 3 for a window that can
be resized with the picture scaled to fit. `depth` is ignored (pass 0).

```basic
Graphics3D 1280, 720              ; a 1280 x 720 window
AppTitle "My Game"                ; its title
Print GraphicsWidth() + " x " + GraphicsHeight()   ; once a window is open, Print writes in it
```

Other useful commands: `SetFullscreen True` switches to fullscreen at any
time (and `False` back), `IsFullscreen()` says which it is, and
`GraphicsWidth()` / `GraphicsHeight()` give the size.

## The loop

```basic
Graphics 800, 600

x = 0
While Not KeyHit(KEY_ESCAPE)       ; 1. keep going until Escape
    x = x + 2                      ; 2. update the game
    If x > 800 Then x = 0

    Cls                            ; 3. clear the picture
    Color 255, 220, 0
    Oval x, 280, 40, 40            ;    draw the game
    Flip                           ; 4. show it
Wend
```

Each time round:

1. **Check for quitting.** `KeyHit(KEY_ESCAPE)` is true once each time
   Escape is pressed. The loop also ends when the player closes the
   window.
2. **Update.** Read input and move things. Here the ball moves 2 pixels.
3. **Draw.** `Cls` clears the picture, then everything is drawn in its
   new position.
4. **Show.** `Flip` puts the finished picture on the screen.

In a 3D game, the drawing step is `RenderWorld`, which draws the 3D
scene; 2D drawing done after it (a score, a map) appears on top:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
light = CreateLight()
cube = CreateCube()
PositionEntity cube, 0, 0, 5
score = 0

While Not KeyHit(KEY_ESCAPE)
    TurnEntity cube, 1, 2, 0        ; update
    score = score + 1
    UpdateWorld                     ; animations, collisions, physics
    RenderWorld                     ; draw the 3D scene
    Text 10, 10, "Score: " + score  ; 2D on top
    Flip                            ; show
Wend
```

`UpdateWorld` advances animations, collisions and physics by one frame;
call it once each time round, before `RenderWorld`.

## Frames and Flip

Each picture is a **frame**. The number of frames a second is the
**frame rate**. `Flip` shows the frame and waits for the screen to be
ready for the next one, and NeptuneBASIC runs the loop at most 60 times a
second, so on most computers a simple loop like the ones above runs at a
steady 60 frames a second.

Everything is drawn into a hidden picture (the **back buffer**) and only
appears at `Flip`, so the player never sees a half-drawn frame.

## Keeping time

| Command | Gives |
|---|---|
| `MilliSecs()` | milliseconds since the program started |
| `Delay ms` | pauses the program |
| `CurrentTime()`, `CurrentDate()` | the clock time and date, as text |

`MilliSecs` measures how long things take, and times events:

```basic
start = MilliSecs()
Delay 500
Print "That took about " + (MilliSecs() - start) + " ms"
```

```basic
; something that happens every 2 seconds
Graphics 800, 600
nextSpawn = MilliSecs() + 2000
spawned = 0
While Not KeyHit(KEY_ESCAPE)
    If MilliSecs() >= nextSpawn
        spawned = spawned + 1
        nextSpawn = nextSpawn + 2000
    EndIf
    Cls
    Text 10, 10, "Enemies spawned: " + spawned
    Flip
Wend
```

## Steady speed on every computer

If a frame takes longer than usual (a slow computer, a busy scene), a
loop that moves things "2 pixels a frame" slows down. Two ways to keep
the game at the same speed everywhere:

**Move by time.** Work out how long the last frame took and move by
speed × time:

```basic
Graphics 800, 600
x# = 0
speed# = 200                      ; pixels per second
previous = MilliSecs()
While Not KeyHit(KEY_ESCAPE)
    now = MilliSecs()
    dt# = (now - previous) / 1000.0   ; seconds since the last frame
    previous = now
    x = x + speed * dt
    If x > 800 Then x = 0
    Cls
    Oval x, 280, 40, 40
    Flip
Wend
```

**Fixed updates.** A timer ticks a fixed number of times a second, and
the game updates once per tick, catching up when a frame was slow:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
cube = CreateCube()
PositionEntity cube, 0, 0, 5
timer = CreateTimer(60)                    ; 60 ticks a second

While Not KeyHit(KEY_ESCAPE)
    ticks = WaitTimer(timer)               ; waits for the next tick
    For t = 1 To Min(ticks, 4)             ; catch up, but not forever
        TurnEntity cube, 0, 2, 0           ; one fixed step
        UpdateWorld
    Next
    RenderWorld
    Flip
Wend
```

Fixed updates make physics and gameplay behave exactly the same on every
machine. `CaptureWorld` and the `tween` argument of `RenderWorld` can
smooth the drawing between steps when the updates are slower than the
frame rate.

## Game states

Most games have several screens: a title, the game, a pause menu, game
over. Keep the current one in a variable and choose what to do each
frame with `Select`:

```basic
Enum Screen
    Title
    Playing
    GameOver
End Enum

Graphics 800, 600
Local current As Screen = Screen.Title
score = 0

While Not KeyHit(KEY_ESCAPE)
    Cls
    Select current
        Case Screen.Title
            Text 300, 280, "Press Space to start"
            If KeyHit(KEY_SPACE) Then current = Screen.Playing : score = 0
        Case Screen.Playing
            score = score + 1
            Text 10, 10, "Score: " + score
            If score >= 300 Then current = Screen.GameOver
        Case Screen.GameOver
            Text 300, 280, "Game over! Score " + score + ". Space to play again"
            If KeyHit(KEY_SPACE) Then current = Screen.Title
    End Select
    Flip
Wend
```

## A complete small game

Catch the falling stars with the paddle. It uses almost everything in
part 2: variables, a type, functions, loops and decisions.

```basic
; catch.nb: catch the falling stars
Type Star
    Field x#, y#, speed#
End Type

Global paddleX# = 400
Global score = 0
Global missed = 0

Function DropStar()
    s.Star = New Star
    s\x = Rnd(20, 780)
    s\y = -10
    s\speed = Rnd(2, 5)
End Function

Function UpdateStars()
    For Each s In Star
        s\y = s\y + s\speed
        If s\y > 560 And s\y < 580 And Abs(s\x - paddleX) < 50
            score = score + 1
            Delete s
        ElseIf s\y > 600
            missed = missed + 1
            Delete s
        EndIf
    Next
End Function

Function DrawGame()
    Color 255, 230, 80
    For Each s In Star
        Oval s\x - 6, s\y - 6, 12, 12
    Next
    Color 80, 200, 255
    Rect paddleX - 50, 570, 100, 12
    Color 255, 255, 255
    Text 10, 10, "Caught: " + score + "   Missed: " + missed + " of 10"
End Function

Graphics 800, 600
AppTitle "Catch"
SeedRnd MilliSecs()
nextStar = 0

While Not KeyHit(KEY_ESCAPE) And missed < 10
    If KeyDown(KEY_LEFT) Then paddleX = paddleX - 7
    If KeyDown(KEY_RIGHT) Then paddleX = paddleX + 7
    paddleX = Clamp(paddleX, 50, 750)
    If MilliSecs() > nextStar Then DropStar() : nextStar = MilliSecs() + 700
    UpdateStars()
    Cls
    DrawGame()
    Flip
Wend
Print "You caught " + score + " stars"
```

## Try it yourself

1. **Bouncing ball.** Open an 800 x 600 window and draw an orange ball that
   moves across it. When it touches any side of the window it should bounce
   back. Escape quits.
   *Hint: keep a speed for across and a speed for down, and turn one around (`dx = -dx`) when the ball goes past an edge.*
2. **Bomb fuse.** Show a bar that shrinks over 10 seconds, with the seconds
   left written above it. When the time has run out, show `BOOM!` instead.
   Space lights the fuse again, and Escape quits.
   *Hint: remember `MilliSecs()` when the fuse is lit, and work out the seconds left from how much time has passed since.*
3. **Orb hunt.** Make a game with three screens: a title (`Space` to start),
   the game, and a win screen. In the game, steer a square with the arrow keys
   to touch a glowing orb. Each touch scores a point and moves the orb
   somewhere new. At 5 points, go to the win screen, where `Space` goes back
   to the title.
   *Hint: an `Enum` for the screens and a `Select` in the loop, like the game states example.*

### Answers

1.

```basic
Graphics 800, 600
x# = 400
y# = 300
dx# = 4
dy# = 3

While Not KeyHit(KEY_ESCAPE)
    x = x + dx
    y = y + dy
    If x < 20 Or x > 780 Then dx = -dx
    If y < 20 Or y > 580 Then dy = -dy

    Cls
    Color 255, 140, 40
    Oval x - 20, y - 20, 40, 40
    Flip
Wend
```

Each time round the loop the ball moves by `dx` and `dy`. The ball is 40
pixels wide, so its centre touching 20 pixels from a side means the edge of
the ball has reached the side. Turning the speed round with `-dx` makes it go
back the other way. The loop ends on Escape (or when the window is closed),
and `Flip` shows every frame.

2.

```basic
Graphics 800, 600
start = MilliSecs()

While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_SPACE) Then start = MilliSecs()
    remaining# = 10 - (MilliSecs() - start) / 1000.0

    Cls
    If remaining > 0
        Color 255, 160, 0
        Rect 100, 280, 600 * remaining / 10, 40
        Color 255, 255, 255
        Text 100, 250, "Fuse: " + FormatFloat(remaining, 1) + " seconds"
    Else
        Color 255, 60, 60
        Text 370, 290, "BOOM!"
    EndIf
    Color 255, 255, 255
    Text 10, 10, "Space lights the fuse again. Escape quits."
    Flip
Wend
```

`MilliSecs() - start` is the time since the fuse was lit, and dividing by
`1000.0` turns it into seconds. The bar's width is the fraction of the time
that is left, times its full width of 600. Space sets `start` to now, which
puts the full ten seconds back.

3.

```basic
Enum Screen
    Title
    Playing
    Won
End Enum

Graphics 800, 600
Local current As Screen = Screen.Title
px# = 400
py# = 300
orbX = 100
orbY = 100
score = 0

While Not KeyHit(KEY_ESCAPE)
    Cls
    Color 255, 255, 255
    Select current
        Case Screen.Title
            Text 290, 280, "Orb Hunt: press Space to start"
            If KeyHit(KEY_SPACE)
                score = 0
                current = Screen.Playing
            EndIf
        Case Screen.Playing
            If KeyDown(KEY_LEFT) Then px = px - 5
            If KeyDown(KEY_RIGHT) Then px = px + 5
            If KeyDown(KEY_UP) Then py = py - 5
            If KeyDown(KEY_DOWN) Then py = py + 5
            px = Clamp(px, 15, 785)
            py = Clamp(py, 15, 585)
            If Abs(px - orbX) < 25 And Abs(py - orbY) < 25
                score = score + 1
                orbX = Rand(40, 760)
                orbY = Rand(40, 560)
                If score >= 5 Then current = Screen.Won
            EndIf
            Color 255, 220, 0
            Oval orbX - 10, orbY - 10, 20, 20
            Color 80, 200, 255
            Rect px - 15, py - 15, 30, 30
            Color 255, 255, 255
            Text 10, 10, "Orbs: " + score + " of 5"
        Case Screen.Won
            Text 250, 280, "You found them all! Press Space for the title"
            If KeyHit(KEY_SPACE) Then current = Screen.Title
    End Select
    Flip
Wend
```

The variable `current` decides which part of the `Select` runs each frame, so
each screen has its own update and drawing code. Touching the orb is a
distance test on both directions at once, and `Clamp` keeps the square inside
the window. Changing `current` is all it takes to move to another screen.

[Contents](../MANUAL.md) · Previous: [11. Organising programs](11-organising.md) · Next: [13. 2D graphics](13-2d-graphics.md)
