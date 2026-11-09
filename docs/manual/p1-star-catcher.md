# Project 1: Star Catcher

[Contents](../MANUAL.md) · Next: [Project 2: Crystal Hunt](p2-crystal-hunt.md)

In this project you build a whole 2D game, one small step at a time. Stars
and rocks fall from the sky. Catch the stars with your basket, dodge the
rocks, and try to beat your best score, which the game remembers.

You need [chapters 2 to 6](02-values-and-variables.md), a first look at
[types](09-types.md), and [the game loop](12-game-loop.md). No pictures or
sound files are needed: everything is drawn with shapes.

Run the program after every step. Seeing each piece work, or finding the
mistake while it is still small, is most of what makes this easy.

- [Step 1: A window](#step-1-a-window)
- [Step 2: A basket that moves](#step-2-a-basket-that-moves)
- [Step 3: Steady speed](#step-3-steady-speed)
- [Step 4: One falling star](#step-4-one-falling-star)
- [Step 5: Many falling things](#step-5-many-falling-things)
- [Step 6: Catching](#step-6-catching)
- [Step 7: Rocks and lives](#step-7-rocks-and-lives)
- [Step 8: Title and game over](#step-8-title-and-game-over)
- [Step 9: Remembering the best score](#step-9-remembering-the-best-score)
- [The whole game](#the-whole-game)
- [Make it yours](#make-it-yours)

## Step 1: A window

Every game starts the same way. Open a window, then go round a loop until
the player presses Escape.

```basic
; Star Catcher, step 1
Const SCREEN_W = 800
Const SCREEN_H = 600

Graphics SCREEN_W, SCREEN_H
AppTitle "Star Catcher"

While Not KeyHit(KEY_ESCAPE)
    Cls
    Text 10, 10, "It works!"
    Flip
Wend
```

`Const` gives a number a name. Writing `SCREEN_W` rather than `800` says what
the number means, and if you change your mind about the size, you change one
line.

**Run it.** You should see a black window with a message. Escape closes it.

## Step 2: A basket that moves

The basket is a rectangle, and all we need to know about it is how far along
it is. Add these lines at the top, under the other `Const` lines:

<!-- nocheck -->
```basic
Const BASKET_W = 110
Const BASKET_H = 20
Const BASKET_Y = 550

Global basketX# = 345
```

The `#` makes `basketX` a number that can have a fraction. `Global` lets the
functions we write later use it.

Then replace the loop:

<!-- nocheck -->
```basic
While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_LEFT) Then basketX = basketX - 8
    If KeyDown(KEY_RIGHT) Then basketX = basketX + 8
    basketX = Clamp(basketX, 0, SCREEN_W - BASKET_W)

    Cls
    Color 90, 200, 255
    Rect basketX, BASKET_Y, BASKET_W, BASKET_H
    Flip
Wend
```

`KeyDown` is true for as long as a key is held. `Clamp` keeps a number
inside a range, so the basket stops at the edges of the window.

**Run it.** The arrow keys slide the basket, and it can't leave the screen.

## Step 3: Steady speed

"8 pixels a frame" has a hidden problem: on a slower computer there are fewer
frames, so the whole game runs slowly. The cure is to measure how long each
frame took, and move by *speed times time*.

Here is the whole program so far with that change. From now on, speeds are in
pixels per second.

```basic
; Star Catcher, step 3
Const SCREEN_W = 800
Const SCREEN_H = 600
Const BASKET_W = 110
Const BASKET_H = 20
Const BASKET_Y = 550
Const BASKET_SPEED = 520

Global basketX# = 345

Function UpdateGame(dt#)
    If KeyDown(KEY_LEFT) Then basketX = basketX - BASKET_SPEED * dt
    If KeyDown(KEY_RIGHT) Then basketX = basketX + BASKET_SPEED * dt
    basketX = Clamp(basketX, 0, SCREEN_W - BASKET_W)
End Function

Function DrawGame()
    Color 90, 200, 255
    Rect basketX, BASKET_Y, BASKET_W, BASKET_H
End Function

Graphics SCREEN_W, SCREEN_H
AppTitle "Star Catcher"

previous = MilliSecs()
While Not KeyHit(KEY_ESCAPE)
    now = MilliSecs()
    dt# = Min((now - previous) / 1000.0, 0.05)
    previous = now

    UpdateGame dt
    Cls
    DrawGame
    Flip
Wend
```

Three things changed:

- `dt` is the time since the last frame, in seconds. `MilliSecs()` counts
  thousandths of a second, so we divide by 1000.
- `Min(..., 0.05)` means that if the computer stalls for a moment, nothing
  jumps a huge distance in one frame.
- The loop now only calls `UpdateGame` and `DrawGame`. As the game grows, the
  loop stays short and each job has a place to live.

**Run it.** It should feel the same as before, only now it would feel the
same on any computer.

## Step 4: One falling star

Before handling many stars, get one right. Add two globals and make a star
fall in `UpdateGame`:

<!-- nocheck -->
```basic
Const ITEM_SIZE = 26
Global starX# = 400, starY# = 0

; at the end of UpdateGame:
    starY = starY + 300 * dt
    If starY > SCREEN_H Then
        starY = -ITEM_SIZE
        starX = Rand(0, SCREEN_W - ITEM_SIZE)
    EndIf

; at the start of DrawGame:
    Color 255, 220, 60
    Oval starX, starY, ITEM_SIZE, ITEM_SIZE
```

**Run it.** A yellow star falls, and comes back somewhere new each time.

## Step 5: Many falling things

One star needs two variables. Ten stars would need twenty. This is what a
**type** is for: describe one falling thing once, then make as many as you
like. Delete `starX`, `starY` and the code that uses them, then add:

<!-- nocheck -->
```basic
Type Item
    Field x#, y#, speed#
End Type

Global nextDrop# = 0.5          ; seconds until the next item

Function DropItem()
    i.Item = New Item
    i\x = Rand(0, SCREEN_W - ITEM_SIZE)
    i\y = -ITEM_SIZE
    i\speed = Rand(220, 320)
End Function
```

Add this to the end of `UpdateGame`:

<!-- nocheck -->
```basic
    nextDrop = nextDrop - dt
    If nextDrop <= 0 Then
        DropItem
        nextDrop = 0.8
    EndIf

    For i.Item = Each Item
        i\y = i\y + i\speed * dt
        If i\y > SCREEN_H Then Delete i
    Next
```

And draw them all in `DrawGame`:

<!-- nocheck -->
```basic
    Color 255, 220, 60
    For i.Item = Each Item
        Oval i\x, i\y, ITEM_SIZE, ITEM_SIZE
    Next
```

`New Item` makes one more item. `For i.Item = Each Item` visits every item
there is. `Delete i` removes one when it has gone off the screen; without
that, the game would keep tracking thousands of invisible stars.

**Run it.** Stars rain down at different speeds.

## Step 6: Catching

An item is caught when its square overlaps the basket. `RectsOverlap` answers
exactly that. Add a global `score`:

<!-- nocheck -->
```basic
Global score
```

Replace the `For` loop in `UpdateGame`:

<!-- nocheck -->
```basic
    For i.Item = Each Item
        i\y = i\y + i\speed * dt
        If RectsOverlap(i\x, i\y, ITEM_SIZE, ITEM_SIZE, basketX, BASKET_Y, BASKET_W, BASKET_H) Then
            score = score + 1
            Delete i
        ElseIf i\y > SCREEN_H Then
            Delete i
        EndIf
    Next
```

Show the score at the end of `DrawGame`:

<!-- nocheck -->
```basic
    Color 255, 255, 255
    Text 10, 10, "Score: " + score
```

**Run it.** You now have a game: catch stars, and the score goes up.

## Step 7: Rocks and lives

A game needs a way to lose. Add `Field isRock` to the `Item` type, a global
`lives = 3`, and this line to `DropItem`:

<!-- nocheck -->
```basic
    i\isRock = Rand(1, 4) = 1           ; one in four is a rock
```

`Rand(1, 4) = 1` is a comparison, and its answer (1 for true, 0 for false) is
what gets stored.

Now the rules. Catching a rock costs a life, and so does letting a star
fall. Letting a rock fall is what you want.

<!-- nocheck -->
```basic
        If RectsOverlap(i\x, i\y, ITEM_SIZE, ITEM_SIZE, basketX, BASKET_Y, BASKET_W, BASKET_H) Then
            If i\isRock Then
                lives = lives - 1
            Else
                score = score + 1
            EndIf
            Delete i
        ElseIf i\y > SCREEN_H Then
            If Not i\isRock Then lives = lives - 1
            Delete i
        EndIf
```

In `DrawGame`, draw rocks as grey squares:

<!-- nocheck -->
```basic
    For i.Item = Each Item
        If i\isRock Then
            Color 130, 120, 115
            Rect i\x, i\y, ITEM_SIZE, ITEM_SIZE
        Else
            Color 255, 220, 60
            Oval i\x, i\y, ITEM_SIZE, ITEM_SIZE
        EndIf
    Next
```

To make it get harder, fall faster and drop more often as the score climbs.
Change these two lines:

<!-- nocheck -->
```basic
    i\speed = Rand(220, 320) + score * 4
    nextDrop = Max(0.25, 0.8 - score * 0.01)
```

**Run it.** Rocks fall too. `lives` can go below zero and nothing happens
yet: that is the next step.

## Step 8: Title and game over

A finished game has more than one screen. Keep a number saying which one it
is on: a **state**. Add:

<!-- nocheck -->
```basic
Const STATE_TITLE = 0
Const STATE_PLAY = 1
Const STATE_OVER = 2

Global state = STATE_TITLE

Function StartGame()
    Delete Each Item
    basketX = (SCREEN_W - BASKET_W) / 2
    score = 0
    lives = 3
    nextDrop = 0.5
    state = STATE_PLAY
End Function
```

`Delete Each Item` clears away everything left over from the last game.

At the end of `UpdateGame`, the game ends when the lives run out:

<!-- nocheck -->
```basic
    If lives <= 0 Then state = STATE_OVER
```

The middle of the main loop now depends on the state:

<!-- nocheck -->
```basic
    Cls
    Select state
        Case STATE_TITLE
            DrawMessage "STAR CATCHER", "Left and Right move. Press Space to play."
            If KeyHit(KEY_SPACE) Then StartGame
        Case STATE_PLAY
            UpdateGame dt
            DrawGame
        Case STATE_OVER
            DrawGame
            DrawMessage "GAME OVER", "Score " + score + ". Press Space to play again."
            If KeyHit(KEY_SPACE) Then StartGame
    End Select
    Flip
```

And a helper to show the messages. The `True` centres the text on its `x`:

<!-- nocheck -->
```basic
Function DrawMessage(title$, hint$)
    Color 255, 255, 255
    Text SCREEN_W / 2, 240, title, True
    Text SCREEN_W / 2, 280, hint, True
End Function
```

**Run it.** You can play, lose, and play again.

## Step 9: Remembering the best score

The **settings** commands keep values in a small text file. `UserFolder` gives
a folder where a game is allowed to keep its files. Add near the top:

<!-- nocheck -->
```basic
Global saveFile$ = UserFolder("StarCatcher") + "/save.txt"

LoadSettings saveFile
Global best = GetSettingInt("best", 0)
```

`GetSettingInt` gives 0 the first time, when there is no file yet. Now replace
`state = STATE_OVER` with a call to this function:

<!-- nocheck -->
```basic
Function GameOver()
    If score > best Then
        best = score
        SetSetting "best", best
        SaveSettings saveFile
    EndIf
    state = STATE_OVER
End Function
```

Show it in `DrawGame` with `Text SCREEN_W - 120, 10, "Best: " + best`.

**Run it.** Get a score, close the game, run it again. Your best is still there.

## The whole game

Here is everything together. If your version misbehaves, compare it with this.

```basic
; Star Catcher: catch the stars, dodge the rocks.

Const SCREEN_W = 800
Const SCREEN_H = 600
Const BASKET_W = 110
Const BASKET_H = 20
Const BASKET_Y = 550
Const BASKET_SPEED = 520
Const ITEM_SIZE = 26

Const STATE_TITLE = 0
Const STATE_PLAY = 1
Const STATE_OVER = 2

Type Item
    Field x#, y#, speed#
    Field isRock
End Type

Global state = STATE_TITLE
Global basketX#, score, lives, nextDrop#
Global saveFile$ = UserFolder("StarCatcher") + "/save.txt"

LoadSettings saveFile
Global best = GetSettingInt("best", 0)

Function StartGame()
    Delete Each Item
    basketX = (SCREEN_W - BASKET_W) / 2
    score = 0
    lives = 3
    nextDrop = 0.5
    state = STATE_PLAY
End Function

Function DropItem()
    i.Item = New Item
    i\x = Rand(0, SCREEN_W - ITEM_SIZE)
    i\y = -ITEM_SIZE
    i\speed = Rand(220, 320) + score * 4
    i\isRock = Rand(1, 4) = 1           ; one in four is a rock
End Function

Function GameOver()
    If score > best Then
        best = score
        SetSetting "best", best
        SaveSettings saveFile
    EndIf
    state = STATE_OVER
End Function

Function UpdateGame(dt#)
    If KeyDown(KEY_LEFT) Then basketX = basketX - BASKET_SPEED * dt
    If KeyDown(KEY_RIGHT) Then basketX = basketX + BASKET_SPEED * dt
    basketX = Clamp(basketX, 0, SCREEN_W - BASKET_W)

    nextDrop = nextDrop - dt
    If nextDrop <= 0 Then
        DropItem
        nextDrop = Max(0.25, 0.8 - score * 0.01)
    EndIf

    For i.Item = Each Item
        i\y = i\y + i\speed * dt
        If RectsOverlap(i\x, i\y, ITEM_SIZE, ITEM_SIZE, basketX, BASKET_Y, BASKET_W, BASKET_H) Then
            If i\isRock Then
                lives = lives - 1
            Else
                score = score + 1
            EndIf
            Delete i
        ElseIf i\y > SCREEN_H Then
            If Not i\isRock Then lives = lives - 1
            Delete i
        EndIf
    Next

    If lives <= 0 Then GameOver
End Function

Function DrawGame()
    For i.Item = Each Item
        If i\isRock Then
            Color 130, 120, 115
            Rect i\x, i\y, ITEM_SIZE, ITEM_SIZE
        Else
            Color 255, 220, 60
            Oval i\x, i\y, ITEM_SIZE, ITEM_SIZE
        EndIf
    Next
    Color 90, 200, 255
    Rect basketX, BASKET_Y, BASKET_W, BASKET_H
    Color 255, 255, 255
    Text 10, 10, "Score: " + score
    Text 10, 30, "Lives: " + lives
    Text SCREEN_W - 120, 10, "Best: " + best
End Function

Function DrawMessage(title$, hint$)
    Color 255, 255, 255
    Text SCREEN_W / 2, 240, title, True
    Text SCREEN_W / 2, 280, hint, True
End Function

Graphics SCREEN_W, SCREEN_H
AppTitle "Star Catcher"

previous = MilliSecs()
While Not KeyHit(KEY_ESCAPE)
    now = MilliSecs()
    dt# = Min((now - previous) / 1000.0, 0.05)
    previous = now

    Cls
    Select state
        Case STATE_TITLE
            DrawMessage "STAR CATCHER", "Left and Right move. Press Space to play."
            If KeyHit(KEY_SPACE) Then StartGame
        Case STATE_PLAY
            UpdateGame dt
            DrawGame
        Case STATE_OVER
            DrawGame
            DrawMessage "GAME OVER", "Score " + score + ". Press Space to play again."
            If KeyHit(KEY_SPACE) Then StartGame
    End Select
    Flip
Wend
```

## Make it yours

The best way to learn is to change something. Some ideas, easiest first:

1. Change the colours, the speeds and the size of the basket until it feels right.
2. Move the basket with the mouse: `basketX = MouseX() - BASKET_W / 2`.
3. Add a green item that gives back a life. It is one more `Field` and one more `If`.
4. Add sound with `LoadSound` and `PlaySound` ([chapter 16](16-sound.md)).
5. Draw pictures instead of shapes with `LoadImage` and `DrawImage` ([chapter 13](13-2d-graphics.md)).
6. When you are happy with it, give it to a friend ([chapter 25](25-sharing.md)).

[Contents](../MANUAL.md) · Next: [Project 2: Crystal Hunt](p2-crystal-hunt.md)
