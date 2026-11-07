# 13. 2D graphics

[Contents](../MANUAL.md) · Previous: [12. The game loop](12-game-loop.md) · Next: [14. 2D games](14-2d-games.md)

This chapter covers drawing in 2D: shapes, colours, text, fonts, images
and single pixels. Everything here also works on top of a 3D scene
(draw after `RenderWorld`).

- [The screen](#the-screen)
- [Colours and shapes](#colours-and-shapes)
- [Text and fonts](#text-and-fonts)
- [Images](#images)
- [Animation strips](#animation-strips)
- [Turning, scaling and fading images](#turning-scaling-and-fading-images)
- [Collisions between images](#collisions-between-images)
- [Drawing into images and textures](#drawing-into-images-and-textures)
- [Pixels](#pixels)

## The screen

Positions on the screen are measured in **pixels** from the top-left
corner: x goes right, y goes **down**. In an 800 × 600 window, (0, 0) is
the top-left corner and (799, 599) the bottom-right.

| Command | Does |
|---|---|
| `Cls` | clears the screen to the `ClsColor` colour |
| `ClsColor r, g, b` | sets that colour |
| `Origin x, y` | measures drawing from x, y instead of the corner |
| `Viewport x, y, w, h` | only draws inside that rectangle |

## Colours and shapes

`Color r, g, b` sets the colour for everything drawn after it. Each of
red, green and blue goes from 0 to 255: `Color 255, 0, 0` is red,
`Color 255, 255, 0` yellow, `Color 128, 128, 128` grey.

| Command | Draws |
|---|---|
| `Plot x, y` | one pixel |
| `Line x1, y1, x2, y2` | a line |
| `Rect x, y, w, h [, solid]` | a rectangle; `solid` 0 for just its outline |
| `Oval x, y, w, h [, solid]` | an ellipse inside that box (a circle if w = h) |

```basic
Graphics 800, 600
ClsColor 20, 24, 40
While Not KeyHit(KEY_ESCAPE)
    Cls
    Color 60, 160, 60 : Rect 0, 450, 800, 150          ; grass
    Color 255, 220, 0 : Oval 620, 60, 100, 100         ; the sun
    Color 140, 90, 50 : Rect 200, 300, 160, 150        ; a house
    Color 200, 40, 40
    Line 190, 300, 280, 220 : Line 280, 220, 370, 300  ; its roof
    Color 255, 255, 255 : Rect 250, 360, 50, 90, False ; a door outline
    Flip
Wend
```

## Text and fonts

`Text x, y, text$ [, center_x, center_y]` writes text in the current
colour and font. With `center_x` set to 1 the text is centred on x
(and `center_y` centres it on y):

```basic
Graphics 800, 600
font = LoadFont("Arial", 32, True)          ; name, height, bold
SetFont font
While Not KeyHit(KEY_ESCAPE)
    Cls
    Color 255, 255, 255
    Text 400, 300, "PAUSED", True, True      ; in the middle of the screen
    SetFont 0                                ; the built-in font
    Text 10, 10, "Width of PAUSED: " + StringWidth("PAUSED")
    SetFont font
    Flip
Wend
```

| Command | Does |
|---|---|
| `LoadFont(name$, height [, bold, italic, underline])` | loads a font; returns 0 if it can't be found |
| `SetFont font` | makes it the font for `Text` (0 is the built-in font) |
| `StringWidth(text$)`, `StringHeight(text$)` | the size text will have, in pixels |
| `FontWidth()`, `FontHeight()` | the size of the current font |

`Print` also works in a graphics window: it writes lines down the
screen, and `Locate x, y` moves where the next one goes.

## Images

An image is a picture loaded from a file (`.png`, `.jpg`, `.bmp`, ...):

```basic
Graphics 800, 600
ship = LoadImage("examples/media/ship.png")
If ship = 0 Then RuntimeError "ship.png is missing"
MidHandle ship                         ; draw it by its centre
x = 400 : y = 300
While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_LEFT) Then x = x - 4
    If KeyDown(KEY_RIGHT) Then x = x + 4
    Cls
    DrawImage ship, x, y
    Flip
Wend
```

- `DrawImage image, x, y [, frame]` draws an image, with its **mask
  colour** (black, unless set with `MaskImage`) left out so the
  background shows through. Images with transparency (PNG) are drawn
  with it.
- `DrawBlock` draws without leaving anything out (faster, for
  backgrounds).
- An image is drawn from its **handle**, the top-left corner unless you
  move it: `MidHandle image` puts it at the centre, which is better for
  things that turn; `HandleImage image, x, y` puts it anywhere.
  `AutoMidHandle True` does `MidHandle` for every image loaded after it.
- `ImageWidth(image)`, `ImageHeight(image)` give its size.
- `TileImage image` fills the whole screen with copies, for backgrounds.

## Animation strips

An animation is usually one image file with all its frames side by side.
`LoadAnimImage(file$, width, height, first, count)` cuts it into
`count` frames of `width` × `height` pixels, starting at frame `first`
(0 is the first):

```basic
Graphics 800, 600
coin = LoadAnimImage("examples/media/coin.png", 32, 32, 0, 4)
frame# = 0
While Not KeyHit(KEY_ESCAPE)
    frame = frame + 0.15                      ; about 9 frames a second
    If frame >= 4 Then frame = 0
    Cls
    DrawImage coin, 400, 300, Int(Floor(frame))
    Flip
Wend
```

The [2D animation commands](14-2d-games.md#animation) do this
bookkeeping for you.

## Turning, scaling and fading images

`DrawImageEx image, x, y, frame, rotation, scale_x, scale_y, alpha`
draws an image turned (degrees, clockwise), scaled and see-through
(alpha 0 invisible to 1 solid) without changing the image itself:

```basic
Graphics 800, 600
ship = LoadImage("examples/media/ship.png")
MidHandle ship
angle# = 0
While Not KeyHit(KEY_ESCAPE)
    angle = angle + 2
    Cls
    DrawImageEx ship, 400, 300, 0, angle, 1.5, 1.5, 0.7
    Flip
Wend
```

`RotateImage`, `ScaleImage` and `ResizeImage` change an image
permanently, which is slower: do it once while loading, not every frame.

## Collisions between images

| Command | True when |
|---|---|
| `ImagesOverlap(image1, x1, y1, image2, x2, y2)` | the images' rectangles overlap (fast) |
| `ImagesCollide(image1, x1, y1, frame1, image2, x2, y2, frame2)` | their solid pixels overlap (exact) |
| `ImageRectCollide(image, x, y, frame, rx, ry, rw, rh)` | a solid pixel is inside the rectangle |
| `RectsOverlap(x1, y1, w1, h1, x2, y2, w2, h2)` | two rectangles overlap |

```basic
Graphics 800, 600
ship = LoadImage("examples/media/ship.png")
rock = LoadImage("examples/media/ring.png")
x = 100
While Not KeyHit(KEY_ESCAPE)
    x = x + 2
    Cls
    DrawImage rock, 400, 300
    DrawImage ship, x, 300
    If ImagesCollide(ship, x, 300, 0, rock, 400, 300, 0) Then Text 10, 10, "CRASH!"
    Flip
Wend
```

Use the fast test first, and the exact one only when the fast one says
they might touch.

## Drawing into images and textures

Drawing normally goes to the screen, but `SetBuffer` can send it into an
image or a 3D texture instead: to build a picture once and reuse it, or
to make a texture that changes.

```basic
Graphics 800, 600
badge = CreateImage(64, 64)
SetBuffer ImageBuffer(badge)        ; draw into the image
Color 255, 200, 0 : Oval 0, 0, 64, 64
Color 0, 0, 0 : Text 32, 32, "1", True, True
SetBuffer BackBuffer()              ; back to the screen

While Not KeyHit(KEY_ESCAPE)
    Cls
    For i = 0 To 9
        DrawImage badge, 20 + i * 70, 260
    Next
    Flip
Wend
```

| Buffer | Is |
|---|---|
| `BackBuffer()` | the hidden screen that `Flip` shows (where drawing normally goes) |
| `FrontBuffer()` | the screen as it is now |
| `ImageBuffer(image [, frame])` | an image |
| `TextureBuffer(texture [, frame])` | a 3D texture |

`SaveBuffer(buffer, file$)` saves one to a file: `SaveBuffer(BackBuffer(),
"screenshot.png")` takes a screenshot.

## Pixels

| Command | Does |
|---|---|
| `ReadPixel(x, y [, buffer])` | the colour of a pixel, as one number (ARGB) |
| `WritePixel x, y, argb [, buffer]` | sets a pixel |
| `LockBuffer` / `UnlockBuffer`, `ReadPixelFast`, `WritePixelFast` | the same, faster, for many pixels at once |
| `GetColor x, y` | sets the drawing colour to a pixel's colour |
| `CopyRect sx, sy, w, h, dx, dy [, from, to]` | copies a rectangle of pixels |

An ARGB colour packs alpha, red, green and blue (0-255 each) into one
number: `(alpha Shl 24) Or (red Shl 16) Or (green Shl 8) Or blue`.

```basic
; a gradient made pixel by pixel, in an image
Graphics 800, 600
fade = CreateImage(256, 64)
buffer = ImageBuffer(fade)
LockBuffer buffer
For x = 0 To 255
    For y = 0 To 63
        WritePixelFast x, y, (255 Shl 24) Or (x Shl 16) Or (64 Shl 8) Or (255 - x), buffer
    Next
Next
UnlockBuffer buffer
While Not KeyHit(KEY_ESCAPE)
    Cls
    DrawImage fade, 272, 268
    Flip
Wend
```

Pixels are read from images and textures, not from the screen itself.

[Contents](../MANUAL.md) · Previous: [12. The game loop](12-game-loop.md) · Next: [14. 2D games](14-2d-games.md)
