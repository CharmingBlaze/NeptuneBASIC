# 2. First steps

[Contents](../SHADERS.md) · Previous: [1. Introduction](01-introduction.md) · Next: [3. Ready-made materials](03-materials.md)

This chapter is a guided tour. Each lesson is a short, complete program:
type it into a file (or copy it), run it, then try changing the numbers.
The programs load `examples/media/checker.png`, so run them from the
NeptuneBASIC folder:

```
neptune run lesson1.nb
```

## Lesson 1: a scene to work with

Before changing how things look, we need something to look at: a camera,
a light, a floor and a ball.

```basic
; lesson 1: the scene the other lessons build on
Graphics3D 1280, 720

camera = CreateCamera()
PositionEntity camera, 0, 1.5, -5      ; a little up and back
CameraClsColor camera, 40, 46, 60       ; dark blue background

sun = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity sun, 45, 30, 0               ; shine down at an angle
AmbientLight 60, 60, 70                 ; a little light everywhere

floor = CreatePlane()
PositionEntity floor, 0, -1, 0

ball = CreateSphere(32)                 ; 32 segments: nice and round

While Not KeyHit(KEY_ESCAPE)
    TurnEntity ball, 0, 1, 0            ; spin, so we see every side
    RenderWorld
    Flip
Wend
```

The ball is drawn with the engine's normal material: plain white,
lit by the sun. The next lessons change that.

## Lesson 2: your first material

Add these lines after `ball = CreateSphere(32)`:

```basic
; lesson 2: gold
ball = CreateSphere(32)
gold = CreatePBRMaterial()        ; a new realistic material
SetAlbedo gold, 255, 200, 90      ; its colour: red, green, blue (0-255)
SetMetallic gold, 1               ; 1 = metal, 0 = not metal
SetRoughness gold, 0.25           ; 0 = mirror, 1 = matte
EntityMaterial ball, gold         ; draw the ball with it
```

Run it. The ball is now gold: metal tints its reflections with its own
colour, and the low roughness gives it a sharp highlight.

Try these, one at a time:

| Change | You get |
|---|---|
| `SetMetallic gold, 0` | yellow plastic |
| `SetRoughness gold, 0.8` | brushed, dull gold |
| `SetAlbedo gold, 230, 230, 235` with metallic 1 | silver or chrome |
| `SetAlbedo gold, 200, 30, 30` with metallic 0 and roughness 0.4 | red plastic |

These three settings (colour, metallic, roughness) make almost any
real-world material. [Chapter 3](03-materials.md#pbr-materials) explains
the rest.

## Lesson 3: textures on a material

A material can take its colour from a picture. Replace the gold lines with:

```basic
; lesson 3: a textured, bumpy ball
ball = CreateSphere(32)
checker = LoadTexture("examples/media/checker.png")

tiles = CreatePBRMaterial()
SetAlbedoMap tiles, checker       ; colour from the picture
SetRoughness tiles, 0.4

; a bumpy surface made from noise, with no picture needed
bumps = CreateProceduralShader(1)            ; 1 = noise
SetNoiseScale bumps, 6
SetNormalMap tiles, BakeProceduralTexture(bumps, 256, 256, True), 0.8

EntityMaterial ball, tiles
```

`BakeProceduralTexture` turns a noise pattern into a texture; with its
last argument `True` it makes a **normal map**, which fakes small bumps.
The ball is still perfectly round (look at its outline), but the light
makes it look dented.

## Lesson 4: a cartoon look

Not every game wants realism. Replace the material lines with:

```basic
; lesson 4: cartoon shading with an outline
ball = CreateSphere(32)
hero = CreateToonMaterial(SHADING_TOON)
SetAlbedo hero, 255, 140, 60      ; orange
SetToonSteps hero, 3              ; three flat bands of light
SetToonOutline hero, True, 0.04   ; a dark outline 0.04 units wide
SetRimLighting hero, True         ; a glow along the edges
EntityMaterial ball, hero
```

Swap `SHADING_TOON` for `SHADING_CEL` (two tones, like an animated film),
`SHADING_FLAT` (low-poly), `SHADING_PS1` (wobbly 1990s console) or
`SHADING_PIXEL` (pixel art) to see other looks.

## Lesson 5: a screen effect

Screen effects change the whole picture. Add these lines just before the
`While` loop, keeping any material you like:

```basic
; lesson 5: screen effects (put these before the While loop)
camera = CreateCamera()
crt = LoadPostProcess("builtin:crt")         ; an old-TV effect
AddPostProcess crt, camera                   ; switch it on for the camera

vignette = LoadPostProcess("builtin:vignette")
AddPostProcess vignette, camera              ; darken the corners as well
```

Effects run in the order you add them. Turn one off and on with
`SetPostProcessEnabled`; this loop does it with the space bar:

```basic
; lesson 5, continued: the space bar switches the CRT effect
Graphics3D 1280, 720
camera = CreateCamera()
crt = LoadPostProcess("builtin:crt")
AddPostProcess crt, camera
on = True
While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_SPACE) Then on = Not on : SetPostProcessEnabled crt, on
    RenderWorld
    Flip
Wend
```

Chapter 6 lists all 30 built-in effects.

## Lesson 6: shadows

The ball should cast a shadow on the floor. Shadows are set for the whole
scene; add these after creating the light:

```basic
; lesson 6: soft shadows (after CreateLight)
sun = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity sun, 45, 30, 0
SetShadowMode SHADOW_SOFT      ; soft edges (SHADOW_HARD for sharp ones)
SetShadowQuality 2             ; 0 (fast) to 3 (best)
```

PBR materials, toon materials and the normal material all cast and
receive shadows. [Chapter 7](07-shadows.md) covers the rest.

## Lesson 7: a built-in shader

Beyond materials, the engine has built-in shaders for special effects.
They work like materials, but you set their parameters by name:

```basic
; lesson 7: a hologram
ball = CreateSphere(32)
holo = LoadShader("builtin:hologram")
ShaderSetColor holo, "color", 255, 80, 200      ; pink instead of blue
ShaderSetFloat holo, "lineDensity", 30          ; fewer, thicker lines
ShaderApply holo, ball
```

`ShaderSetColor` and `ShaderSetFloat` take the shader, the name of the
parameter, and the value. [Chapter 3](03-materials.md#built-in-shaders)
lists every built-in shader and its parameters.

## Lesson 8: your own shader

Finally, a shader you write yourself. Make a file called `pulse.glsl`
next to your program:

```glsl
// pulse.glsl: a colour that pulses over time
uniform vec4 tint = vec4(1.0, 0.5, 0.2, 1.0);   // a parameter, orange by default
uniform float speed = 3.0;                      // another one

//! fragment
void nb_surface(inout NbSurface s) {
    // nb_time is the number of seconds since the program started
    float pulse = 0.5 + 0.5 * sin(nb_time * speed);
    s.albedo = tint.rgb * pulse;
}
```

and use it from BASIC:

```basic
; lesson 8: load your shader, watch the file for changes
ball = CreateSphere(32)
pulse = LoadShader("pulse.glsl")
If ShaderCompiled(pulse) = 0 Then RuntimeError ShaderError(pulse)
ShaderHotReload pulse                       ; recompile when the file is saved
ShaderSetColor pulse, "tint", 80, 160, 255  ; blue
ShaderApply pulse, ball
```

Run it, then edit `pulse.glsl` while it runs: change `3.0` to `10.0`, or
`sin` to `cos`, and save. The ball updates at once. If you make a
mistake, the old version keeps running and the error is printed with the
line number.

[Chapter 4](04-writing-shaders.md) teaches shader writing step by step.

## What next

- More looks without code: [3. Ready-made materials](03-materials.md).
- Writing shaders: [4. Writing shaders](04-writing-shaders.md).
- Screen effects: [6. Post-processing](06-post-processing.md).

[Contents](../SHADERS.md) · Previous: [1. Introduction](01-introduction.md) · Next: [3. Ready-made materials](03-materials.md)
