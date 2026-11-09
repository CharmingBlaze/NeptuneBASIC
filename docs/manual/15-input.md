# 15. Input

[Contents](../MANUAL.md) · Previous: [14. 2D games](14-2d-games.md) · Next: [16. Sound and movies](16-sound.md)

Games are played with the keyboard, the mouse and gamepads. This chapter
shows how to read each.

- [Held down, or just pressed?](#held-down-or-just-pressed)
- [The keyboard](#the-keyboard)
- [Typing text](#typing-text)
- [The mouse](#the-mouse)
- [Mouse look](#mouse-look)
- [Gamepads and joysticks](#gamepads-and-joysticks)
- [Supporting several kinds of input](#supporting-several-kinds-of-input)
- [Try it yourself](#try-it-yourself)

## Held down, or just pressed?

Every kind of input can be read two ways:

| Question | Keyboard | Mouse | Gamepad |
|---|---|---|---|
| Is it held down right now? | `KeyDown(key)` | `MouseDown(button)` | `JoyDown(button [, port])` |
| How many times was it pressed since I last asked? | `KeyHit(key)` | `MouseHit(button)` | `JoyHit(button [, port])` |

Use **held** for things that last while the key is held: walking,
steering, charging a shot. Use **hit** for things that happen once per
press: jumping, firing a single shot, opening a menu. Calling `KeyHit`
uses the press up, so ask about each key in one place each frame.

```basic
Graphics 800, 600
x = 400 : jumps = 0
While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_LEFT) Then x = x - 5      ; moves while held
    If KeyDown(KEY_RIGHT) Then x = x + 5
    If KeyHit(KEY_SPACE) Then jumps = jumps + 1   ; once per press
    Cls
    Oval x - 20, 280, 40, 40
    Text 10, 10, "Jumps: " + jumps
    Flip
Wend
```

## The keyboard

Keys are numbered by their position on the keyboard (their **scan
code**); constants name them:

| Keys | Constants |
|---|---|
| letters | `KEY_A` ... `KEY_Z` |
| digits | `KEY_0` ... `KEY_9` |
| arrows | `KEY_UP`, `KEY_DOWN`, `KEY_LEFT`, `KEY_RIGHT` |
| common | `KEY_SPACE`, `KEY_ENTER`, `KEY_ESCAPE`, `KEY_TAB`, `KEY_BACKSPACE` |
| modifiers | `KEY_LSHIFT`, `KEY_RSHIFT`, `KEY_LCONTROL`, `KEY_RCONTROL`, `KEY_LALT`, `KEY_RALT` |
| function keys | `KEY_F1` ... `KEY_F12` |

Because they are positions, `KEY_W` is the key in the W position on
any keyboard layout, which keeps WASD controls in the same place for
everyone.

`WaitKey()` stops until a key is pressed and returns its code, for
"press any key" screens. `FlushKeys` forgets presses that haven't been
read yet (useful when changing screens, so a press on the old screen
doesn't count on the new one).

## Typing text

For text entry (a name, a chat message), read **characters** rather than
keys. `GetKey()` returns the character code of the next key typed, or 0:

```basic
Graphics 800, 600
typed$ = ""
While Not KeyHit(KEY_ESCAPE)
    c = GetKey()
    If c = 8 And Len(typed) > 0 Then typed = Left(typed, Len(typed) - 1)   ; backspace
    If c >= 32 Then typed = typed + Chr(c)                                 ; a printable character
    Cls
    Text 10, 10, "Your name: " + typed + "_"
    Flip
Wend
```

`GetKey` gives capitals, punctuation and accented letters as typed.
For a ready-made text box, see `UITextBox` ([chapter 14](14-2d-games.md#simple-buttons-and-sliders))
or `GuiTextBox` ([chapter 24](24-gui.md)).

## The mouse

| Command | Gives |
|---|---|
| `MouseX()`, `MouseY()` | the pointer's position in the window, in pixels |
| `MouseDown(button)`, `MouseHit(button)` | button 1 left, 2 right, 3 middle (`MOUSE_LEFT`, `MOUSE_RIGHT`, `MOUSE_MIDDLE`) |
| `MouseZ()`, `MouseZSpeed()` | the wheel's position, and how far it moved since last asked |
| `MouseXSpeed()`, `MouseYSpeed()` | how far the mouse moved since last asked |
| `HidePointer`, `ShowPointer` | hides or shows the pointer |
| `MoveMouse x, y` | moves the pointer |

```basic
; paint with the left button, clear with the right
Graphics 800, 600
canvas = CreateImage(800, 600)
While Not KeyHit(KEY_ESCAPE)
    If MouseDown(1)
        SetBuffer ImageBuffer(canvas)
        Color 255, 200, 0
        Oval MouseX() - 5, MouseY() - 5, 10, 10
        SetBuffer BackBuffer()
    EndIf
    If MouseHit(2) Then canvas = CreateImage(800, 600)
    Cls
    DrawBlock canvas, 0, 0
    Flip
Wend
```

## Mouse look

First-person games turn the camera with the mouse. `CaptureMouse True`
locks the pointer to the window and hides it; then `MouseXSpeed` and
`MouseYSpeed` say how far the mouse moved each frame:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
CaptureMouse True
pitch# = 0
While Not KeyHit(KEY_ESCAPE)
    TurnEntity camera, 0, -MouseXSpeed() * 0.2, 0, True    ; turn left and right
    pitch = Clamp(pitch + MouseYSpeed() * 0.2, -80, 80)    ; look up and down, within limits
    RotateEntity camera, pitch, EntityYaw(camera), 0
    RenderWorld
    Flip
Wend
CaptureMouse False
```

## Gamepads and joysticks

Gamepads (Xbox, PlayStation and compatible controllers) are numbered from
port 0. `CountJoys()` says how many are connected.

| Command | Gives |
|---|---|
| `JoyX([port])`, `JoyY([port])` | the left stick, each from -1 to 1 |
| `JoyU([port])`, `JoyV([port])` | the right stick |
| `JoyZ([port])` | the triggers: right minus left, -1 to 1 |
| `JoyHat([port])` | the d-pad as an angle (0 up, 90 right, ...), or -1 when not pressed |
| `JoyDown(button [, port])`, `JoyHit(button [, port])` | buttons |
| `JoyXDir([port])` ... | a stick's direction as -1, 0 or 1 |

Sticks never rest at exactly 0, so ignore small movements (a **dead
zone**):

```basic
Graphics 800, 600
x# = 400 : y# = 300
While Not KeyHit(KEY_ESCAPE)
    If CountJoys() > 0
        sx# = JoyX(0) : sy# = JoyY(0)
        If Abs(sx) < 0.15 Then sx = 0       ; dead zone
        If Abs(sy) < 0.15 Then sy = 0
        x = x + sx * 6
        y = y + sy * 6
        If JoyHit(1, 0) Then Print "button 1"
    EndIf
    Cls
    Oval x - 15, y - 15, 30, 30
    Flip
Wend
```

## Supporting several kinds of input

Read every input in one function that turns it into **actions**, and let
the rest of the game use only the actions. Then the game works the same
with keys, mouse or gamepad, and controls are easy to change:

```basic
Global moveX#, wantsJump

Function ReadControls()
    moveX = KeyDown(KEY_RIGHT) - KeyDown(KEY_LEFT) + KeyDown(KEY_D) - KeyDown(KEY_A)
    wantsJump = KeyHit(KEY_SPACE) Or KeyHit(KEY_W)
    If CountJoys() > 0
        If Abs(JoyX(0)) > 0.15 Then moveX = JoyX(0)
        If JoyHit(1, 0) Then wantsJump = True
    EndIf
    moveX = Clamp(moveX, -1, 1)
End Function
```

## Try it yourself

1. **Sprinter.** Draw a square that moves with the arrow keys *or* W, A, S and
   D, and runs twice as fast while Shift is held. It must not leave the window.
   Escape quits.
   *Hint: `KeyDown` is true every frame the key is held, so add up the directions pressed, then multiply by the speed.*
2. **Secret code.** Show a box where the player types a code. Backspace deletes
   a letter. When Enter is pressed, show `Door unlocked!` if the code is
   `NEPTUNE` (any capitals), or `Wrong code` and an empty box if it is not.
   *Hint: `GetKey()` gives the typed character, and Enter does not give one in the printable range, so use `KeyHit(KEY_ENTER)` for that.*
3. **Target practice.** A round target appears at a random place. Click it
   with the left button to score a point and make it jump to a new place. You
   have 15 seconds; when time is up, stop and show the final score.
   *Hint: a click hits when the distance from `MouseX()`, `MouseY()` to the target centre is less than its radius.*

### Answers

1.

```basic
Graphics 800, 600
x# = 400
y# = 300

While Not KeyHit(KEY_ESCAPE)
    speed# = 4
    If KeyDown(KEY_LSHIFT) Or KeyDown(KEY_RSHIFT) Then speed = 8

    moveX = KeyDown(KEY_RIGHT) - KeyDown(KEY_LEFT) + KeyDown(KEY_D) - KeyDown(KEY_A)
    moveY = KeyDown(KEY_DOWN) - KeyDown(KEY_UP) + KeyDown(KEY_S) - KeyDown(KEY_W)
    x = Clamp(x + moveX * speed, 15, 785)
    y = Clamp(y + moveY * speed, 15, 585)

    Cls
    Color 80, 220, 120
    Rect x - 15, y - 15, 30, 30
    Color 255, 255, 255
    Text 10, 10, "Arrows or WASD to move, Shift to run, Escape to quit"
    Flip
Wend
```

`KeyDown` gives 1 while a key is held and 0 otherwise, so right minus left
is 1, -1 or 0, and the sum of both key sets works too. Multiplying by the
speed moves the square, and `Clamp` keeps its centre far enough from each
edge that the whole square stays in the window.

2.

```basic
Graphics 800, 600
typed$ = ""
message$ = "Type the code and press Enter"

While Not KeyHit(KEY_ESCAPE)
    c = GetKey()
    If c = 8 And Len(typed) > 0
        typed = Left(typed, Len(typed) - 1)
    ElseIf c >= 32
        typed = typed + Chr(c)
    EndIf

    If KeyHit(KEY_ENTER)
        If Upper(typed) = "NEPTUNE"
            message = "Door unlocked!"
        Else
            message = "Wrong code"
        EndIf
        typed = ""
    EndIf

    Cls
    Color 255, 255, 255
    Text 300, 240, message
    Rect 300, 280, 200, 30, False
    Text 308, 288, typed + "_"
    Flip
Wend
```

`GetKey` returns one typed character at a time (or 0), so each frame adds at
most one letter. Backspace is character 8, and anything of 32 or more can be
shown on screen. Enter is not one of those, so it is read with `KeyHit`.
`Upper` makes the comparison ignore capitals, and the box is emptied either
way so the player can try again.

3.

```basic
Graphics 800, 600
SeedRnd MilliSecs()

radius = 30
targetX = 400
targetY = 300
score = 0
start = MilliSecs()

While Not KeyHit(KEY_ESCAPE)
    remaining = 15 - (MilliSecs() - start) / 1000
    playing = remaining > 0

    If playing And MouseHit(1)
        dx = MouseX() - targetX
        dy = MouseY() - targetY
        If Sqr(dx * dx + dy * dy) < radius
            score = score + 1
            targetX = Rand(radius, 800 - radius)
            targetY = Rand(60, 600 - radius)
        EndIf
    EndIf

    Cls
    Color 255, 255, 255
    If playing
        Text 10, 10, "Score: " + score + "    Time left: " + remaining
        Color 255, 70, 70
        Oval targetX - radius, targetY - radius, radius * 2, radius * 2
        Color 255, 255, 255
        Oval targetX - 12, targetY - 12, 24, 24
        Color 255, 70, 70
        Oval targetX - 4, targetY - 4, 8, 8
    Else
        Text 400, 280, "Time is up! Final score: " + score, True, True
    EndIf
    Flip
Wend
```

`MouseHit(1)` is true once for each click, so holding the button does not
score again. The distance formula from earlier chapters tells whether the
click landed inside the circle. The time left is worked out from `MilliSecs`;
when it reaches 0, `playing` is false, so clicks stop counting and the final
message replaces the target.

[Contents](../MANUAL.md) · Previous: [14. 2D games](14-2d-games.md) · Next: [16. Sound and movies](16-sound.md)
