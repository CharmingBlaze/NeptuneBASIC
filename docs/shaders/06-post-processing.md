# 6. Post-processing

[Contents](../SHADERS.md) · Previous: [5. The shader language](05-shader-language.md) · Next: [7. Shadows](07-shadows.md)

Post-processing effects change the finished picture: glow around bright
lights, blur in the distance, an old-TV look, rain on the lens. They work
on the image, not on objects, so one effect changes everything the camera
sees.

- [Using effects](#using-effects)
- [Built-in effects](#built-in-effects)
- [One-call weather and atmosphere](#one-call-weather-and-atmosphere)
- [Lens effects](#lens-effects)
- [Order, cameras and switching](#order-cameras-and-switching)
- [HDR and the final image](#hdr-and-the-final-image)
- [Routing: inputs, outputs and resolution](#routing-inputs-outputs-and-resolution)
- [Writing an effect](#writing-an-effect)
- [The engine's own effects](#the-engines-own-effects)

## Using effects

```basic
; bloom and a vignette on the main camera
Graphics3D 1280, 720
camera = CreateCamera()

bloom = LoadPostProcess("builtin:bloom")
SetPostProcessParameter bloom, "threshold", "0.6"    ; glow from dimmer things too
SetPostProcessParameter bloom, "intensity", "1.2"
AddPostProcess bloom, camera

vignette = LoadPostProcess("builtin:vignette")
AddPostProcess vignette, camera

While Not KeyHit(KEY_ESCAPE)
    RenderWorld
    Flip
Wend
```

1. `LoadPostProcess("builtin:name")` makes an effect (or loads your own
   from a file).
2. `SetPostProcessParameter effect, name$, value$` changes its settings.
   The value is text: one number (`"0.6"`), several for a colour or
   vector (`"1, 0.5, 0"`, each 0-1), or a texture handle.
3. `AddPostProcess effect, camera` adds it to the camera's chain. Camera
   0 adds it to every camera.

The effects of a camera form a **chain**: each one works on the picture
the one before it made.

To change settings more conveniently, get the effect's shader with
`PostProcessShader` and use the usual `ShaderSet...` commands:

```basic
vignette = LoadPostProcess("builtin:vignette")
v = PostProcessShader(vignette)
ShaderSetFloat v, "intensity", 0.7
ShaderSetColor v, "color", 40, 0, 0        ; a dark red edge
```

## Built-in effects

Load any of these with `LoadPostProcess("builtin:name")`. Colours are
0-1 here; set them with `ShaderSetColor` on `PostProcessShader(effect)`
to use 0-255.

**Light and colour**

| Effect | What it does | Parameters (default) |
|---|---|---|
| `bloom` | glow around bright parts | `threshold` (0.7: how bright a pixel must be to glow), `intensity` (0.8), `radius` (6 pixels) |
| `exposure` | brightens or darkens the HDR image | `exposure` (0, in stops: +1 is twice as bright) |
| `tonemap` | turns HDR into screen colours | `mode` (0 ACES, 1 Reinhard, 2 Uncharted 2, 3 none), `exposure` (0) |
| `colorgrade` | full colour correction | `lift` (0,0,0: shadows), `gammaRGB` (1,1,1: mid-tones), `gain` (1,1,1: highlights), `temperature` (0: + warmer, − cooler), `tint` (0: + magenta, − green), `contrast` (1), `saturation` (1), `brightness` (0) |
| `gamma` | gamma | `gamma` (1) |
| `contrast` | contrast | `contrast` (1.2) |
| `saturation` | colourfulness; 0 is black and white | `saturation` (1.3) |
| `brightness` | brighter or darker | `brightness` (0.1) |
| `invert` | a negative | none |

**Lens and camera**

| Effect | What it does | Parameters (default) |
|---|---|---|
| `vignette` | darker corners | `intensity` (0.45), `smoothness` (0.45), `color` (0,0,0) |
| `chromatic` | colour fringes at the edges, like a cheap lens | `amount` (0.004) |
| `grain` | film grain | `amount` (0.06), `size` (1.5) |
| `dof` | depth of field: blur in front of and behind the focus | `focusDistance` (10 units), `focusRange` (6: how deep the sharp zone is), `maxBlur` (6 pixels) |
| `motionblur` | blur from camera movement; objects moving on their own are not blurred | `amount` (1), `maxLength` (0.08: the longest streak, as a part of the screen) |
| `godrays` | shafts of light from the sun, cut by whatever is in the way | `amount` (0.6), `color` (1,0.92,0.75), `sun` (0.4,0.6,0.7,0), `reach` (0.9: how far along the way to the sun each pixel looks), `spread` (0.9: how far the glow extends around the sun, in screen heights), `skyDistance` (0) |
| `lensflare` | glow, streak and coloured circles from a bright light | `amount` (0.7), `size` (1), `color` (1,0.92,0.8), `sun` (0.4,0.6,0.7,0), `skyDistance` (0) |
| `blur` | a simple blur | `radius` (2 pixels) |
| `gaussian` | a smoother blur | `radius` (3 pixels) |
| `radialblur` | zoom blur from a point | `amount` (0.08), `center` (0.5, 0.5: the screen centre) |
| `sharpen` | sharper edges | `amount` (0.6) |
| `fxaa` | smooths jagged edges (anti-aliasing) | `subpix` (0.75), `edgeThreshold` (0.125) |

**Styles**

| Effect | What it does | Parameters (default) |
|---|---|---|
| `crt` | an old TV: curved screen, scan lines, RGB mask | `curvature` (0.12), `scanlines` (0.35), `mask` (0.15), `vignette` (0.3) |
| `pixelate` | big pixels | `pixelSize` (6 pixels) |
| `outline` | outlines around every object, from depth and normals | `thickness` (1 pixel), `color` (0,0,0), `depthThreshold` (0.1), `normalThreshold` (0.4) |
| `distortion` | wavy picture | `amount` (0.01), `scale` (8), `speed` (1) |
| `heathaze` | shimmering air in the distance | `amount` (0.006), `speed` (2), `scale` (40), `startDistance` (10 units) |

**Scene and atmosphere**

| Effect | What it does | Parameters (default) |
|---|---|---|
| `fog` | fog that is thicker low down, from the depth of the scene | `density` (0.03), `color` (0.65,0.72,0.8), `heightFalloff` (0.08: how fast it thins with height), `start` (0: distance where it starts), `skyFog` (1: 0 leaves the sky clear), `skyHorizon` (0: with `skyFog` 0, how far above the horizon the sky still fades into the fog) |
| `underwater` | blue-green, wavy and foggy when the camera is below the water | `waterLevel` (0), `color` (0.1,0.35,0.45), `density` (0.08), `distortion` (0.004) |
| `rain` | rain streaks | `amount` (0.5), `speed` (1), `angle` (0.15) |
| `snow` | falling snow | `amount` (0.5), `speed` (0.3), `size` (1) |
| `ssao` | screen-space ambient occlusion: dark creases | `radius` (0.6 units), `intensity` (1) |
| `ssr` | screen-space reflections on shiny floors | `intensity` (0.5), `maxDistance` (20 units), `thickness` (0.4) |

`sun` in `godrays` and `lensflare` is `x, y, z, w` in world space: with
`w` 0, the direction towards the sun; with `w` 1, a position. Light gets
through only where nothing was drawn (the camera's clear colour or a sky
box), or where the scene is further away than a positioned light. A big
sky sphere or dome is a real object and would block it: set `skyDistance`
to a distance just inside the dome, and anything beyond that counts as sky.

Effects that read depth (`dof`, `fog`, `godrays`, `heathaze`, `lensflare`,
`outline`, `ssao`, `ssr`, `underwater`) see every object, including water and see-through
surfaces. `outline`, `ssao` and `ssr` also read normals, which only
deferred surfaces (PBR materials, `//! pass gbuffer` shaders) provide.

A few looks built from them:

```basic
camera = CreateCamera()

; a dream: soft, bright and glowing
AddPostProcess LoadPostProcess("builtin:bloom"), camera
AddPostProcess LoadPostProcess("builtin:gaussian"), camera
AddPostProcess LoadPostProcess("builtin:brightness"), camera

; a security camera
bw = LoadPostProcess("builtin:saturation")
SetPostProcessParameter bw, "saturation", "0"
AddPostProcess bw, camera
AddPostProcess LoadPostProcess("builtin:grain"), camera
AddPostProcess LoadPostProcess("builtin:crt"), camera

; a tilt-shift miniature
focus = LoadPostProcess("builtin:dof")
SetPostProcessParameter focus, "focusDistance", "15"
SetPostProcessParameter focus, "focusRange", "2"
AddPostProcess focus, camera
saturate = LoadPostProcess("builtin:saturation")
SetPostProcessParameter saturate, "saturation", "1.5"
AddPostProcess saturate, camera
```

## One-call weather and atmosphere

These set up the matching effect on a camera in one call. Calling one
again changes the same effect; an amount of 0 switches it off.

```basic
camera = CreateCamera()
SetRainEffect camera, 0.7                        ; heavy rain
SetSnowEffect camera, 0.4, 0.2                   ; light, slow snow
SetAtmosphericFog camera, 0.02, 180, 190, 200    ; thin grey-blue fog
SetUnderwaterEffect camera, True, 0          ; the sea surface is at y = 0

sun = CreateLight(1)
SetDepthOfField camera, 20                       ; sharpest 20 units away
SetGodRays camera, 0.7, sun                      ; follows the light by itself
SetLensFlare camera, 0.8, sun
```

| Command | Effect |
|---|---|
| `SetRainEffect camera, amount [, speed]` | rain streaks, amount 0-1 |
| `SetSnowEffect camera, amount [, speed]` | falling snow, amount 0-1 |
| `SetAtmosphericFog camera, density [, r, g, b, height_falloff]` | height fog in a colour (0-255) |
| `SetUnderwaterEffect camera, enable [, water_level, r, g, b]` | the underwater look whenever the camera is below `water_level` |
| `SetDepthOfField camera, focus_distance [, focus_range, max_blur]` | blur in front of and behind the focus; a distance of 0 turns it off |
| `SetGodRays camera, amount [, source, r, g, b, sky_distance]` | light shafts from `source`: a directional light, or any other entity for a position |
| `SetLensFlare camera, amount [, source, size, sky_distance]` | a lens flare from `source`, hidden when something is in front of it |

## Lens effects

Depth of field, god rays and lens flares copy what a real camera does.
`examples/lensfx.nb` shows all three, each on its own key.

### Depth of field

```basic
SetDepthOfField camera, 20, 8, 10
```

The three numbers are the focus distance, the focus range and the
strongest blur in pixels:

- Things within half of the range of the focus distance are fully sharp.
  Here, that is from 16 to 24 units away.
- The blur grows from there and is full one and a half ranges from the
  focus. Here, that is nearer than 8 units or further than 32.
- A small range looks like a close-up or a miniature. A large range looks
  like an ordinary view with only a soft distance.

Sharp objects keep clean edges against a blurred background, and a blurred
object close to the camera spreads softly over what is behind it.

The focus often needs to follow what the player looks at. Pick from the
middle of the screen and move the focus part of the way each frame, so it
eases over instead of jumping. Objects need `EntityPickMode` for this:

```basic
; in the game loop
hit = CameraPick(camera, GraphicsWidth() / 2, GraphicsHeight() / 2)
If hit Then
    dx# = PickedX() - EntityX(camera, True)
    dy# = PickedY() - EntityY(camera, True)
    dz# = PickedZ() - EntityZ(camera, True)
    wanted# = Sqr(dx * dx + dy * dy + dz * dz)
    focus = focus + (wanted - focus) * 0.1
EndIf
SetDepthOfField camera, focus, 8, 10
```

### God rays

```basic
sun = CreateLight(1)
RotateEntity sun, 25, 180, 0     ; shines back towards a camera facing +Z
SetGodRays camera, 0.7, sun
```

A directional light is treated as a sun infinitely far away, in the
opposite direction to where it shines. Any other entity is a light at that
place. The effect checks where the source is every frame, so moving or
rotating it is all you need to do.

The shafts fade out as the source leaves the screen and vanish when it is
behind the camera, because the effect only knows what the camera can see.

Try an amount from 0.3 to 1. Call `SetGodRays camera, 0` to turn the effect
off. It keeps its source, so `SetGodRays camera, 0.7` turns it back on.

### Lens flare

```basic
SetLensFlare camera, 0.8, sun, 1.2      ; a little bigger than usual
```

The source works just as it does for god rays. The flare dims as objects
cover the source, so it fades smoothly when the sun passes behind a tree.

### Sky domes and see-through objects

Light gets through only where nothing was drawn or, for a positioned
light, where the scene is further away than the light. That has two
effects on your game:

- A big sky sphere is an ordinary object, so it blocks the sun. Sky boxes
  and the camera's clear colour do not.
- Water, glass and other see-through surfaces also block it, as they are
  in the depth buffer.

For a sky sphere, pass a distance just inside it as the last argument.
Anything further away than that counts as sky:

```basic
; the dome is 400 units away
SetGodRays camera, 0.7, sun, 255, 235, 190, 350
SetLensFlare camera, 0.8, sun, 1, 350
```

If you load `builtin:godrays` or `builtin:lensflare` yourself, set the
`skyDistance` parameter instead. Those effects do not follow a light by
themselves, so give them the sun's direction, each frame if the light
moves:

```basic
rays = LoadPostProcess("builtin:godrays")
SetPostProcessParameter rays, "skyDistance", "350"
AddPostProcess rays, camera

; the direction towards the sun: the opposite of where the light shines
TFormVector 0, 0, -1, sun, 0
SetPostProcessParameter rays, "sun", TFormedX() + "," + TFormedY() + "," + TFormedZ() + ",0"
```

### Order and cost

Effects run in the order they were made. Call `SetDepthOfField` before
`SetLensFlare`, so that the flare is drawn on top of the blur and stays
crisp.

Each effect does its work for every pixel on the screen, so they cost more
at high resolutions:

| Effect | Samples per pixel |
|---|---|
| depth of field | 41 of both the picture and the depth, whenever it is on |
| god rays | 48 of the depth, while the source is on or near the screen |
| lens flare | 13 of the depth, while the source is on or just off the screen |

As a guide, on a laptop GeForce RTX 5050 at 1600x900, depth of field and
god rays took about 0.5 milliseconds of each frame, and lens flare less
than 0.2. A 60 frames a second game has 16.7 milliseconds per frame.
Integrated graphics will be several times slower.

On slow machines, turn off depth of field first. An amount or distance of
0 means the effect costs nothing.

## Order, cameras and switching

```basic
camera = CreateCamera()
bloom = LoadPostProcess("builtin:bloom")
grain = LoadPostProcess("builtin:grain")
AddPostProcess bloom, camera
AddPostProcess grain, camera

SetPostProcessOrder grain, 10          ; lower orders run first
SetPostProcessEnabled bloom, False     ; off, but still in the chain
SetPostProcessEnabled bloom, True      ; on again
RemovePostProcess grain, camera        ; out of the chain (it still exists)
FreePostProcess grain                  ; gone for good
SetPostProcess camera, bloom           ; make bloom the camera's only effect
SetPostProcess camera, 0               ; no effects on the camera
```

| Command | Effect |
|---|---|
| `AddPostProcess e [, camera]` | add to a camera's chain (0: every camera) |
| `RemovePostProcess e [, camera]` | take it out of a chain (0: every chain) |
| `SetPostProcess camera, e` | make `e` the only effect of the camera; 0 clears the chain |
| `PostProcessCamera e, camera` | run `e` only for this camera |
| `SetPostProcessEnabled e, on` | switch on or off without removing |
| `SetPostProcessOrder e, order` | position in the chain: lower runs first. Effects start in the order they were made |
| `FreePostProcess e` | remove from every chain and free |

Effects belong to cameras, so a split-screen game can give each player
their own (a red flash for the player who was hit).

## HDR and the final image

The scene is drawn in **HDR**: colours can be brighter than white. Then
the engine **tone maps** it into colours a screen can show. An effect
runs in one of two places:

- **After tone mapping** (priority 0 or more, the default): on the final
  picture, in screen colours 0-1. Most effects belong here.
- **Before tone mapping** (a negative priority): on the HDR picture, in
  linear light where bright parts can be far above 1. `exposure` and
  `tonemap` run here.

```basic
glow = LoadPostProcess("my_hdr_glow.glsl")
PostProcessPriority glow, -1             ; run on the HDR image
```

## Routing: inputs, outputs and resolution

By default each effect reads the picture so far and writes the next one.
This can be changed:

```basic
camera = CreateCamera()
fx = LoadPostProcess("builtin:blur")
AddPostProcess fx, camera
PostProcessInput fx, POST_ORIGINAL       ; read the scene before any effect
PostProcessResolution fx, 0.5            ; draw at half size: four times cheaper

; draw an effect into a texture instead of the screen
screen = CreateTexture(512, 512)
mirror = LoadPostProcess("builtin:invert")
AddPostProcess mirror, camera
PostProcessOutput mirror, screen         ; the camera's view, inverted, into a texture
monitor = CreateCube()
EntityTexture monitor, screen            ; shown on a cube
```

| Command | Effect |
|---|---|
| `PostProcessInput e, source` | `POST_PREVIOUS` (0, default): the picture so far. `POST_ORIGINAL` (-1): the scene before any effect. Another effect: its result, if that effect draws at a reduced resolution |
| `PostProcessOutput e, texture` | draw into the texture instead of the chain; the chain goes on without it. 0 draws into the chain again |
| `PostProcessResolution e, scale` | draw at a fraction (0.05-1) of the screen size, for cheap blurs and glows |

An effect that draws into a texture can do jobs a compute shader would
do elsewhere: work something out for every pixel and store it, for
objects or other effects to use. Give the texture to another effect as a
parameter: `SetPostProcessParameter other, "data", Str(texture)`.

## Writing an effect

An effect is a shader with a function `nb_post`. It gets the screen
position (0-1) of a pixel and returns its new colour:

```glsl
// scanlines.glsl: LoadPostProcess("scanlines.glsl")
uniform float strength = 0.25;

//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;              // the picture so far
    float line = 0.5 + 0.5 * sin(uv.y * nb_resolution.y * 1.5);
    c *= 1.0 - strength * line;                              // darken every other line
    return vec4(c, 1.0);
}
```

Load it with `LoadPostProcess("scanlines.glsl")` (a `.glsl` file is made a
post effect for you), or from a string with `CreatePostProcess(text$)`.
Its `uniform` lines become parameters, like any shader's.

An effect can read:

| Name | Type | Holds |
|---|---|---|
| `nb_screenTexture` | sampler2D | the picture so far (its input) |
| `nb_originalTexture` | sampler2D | the scene before any effect |
| `nb_depthTexture` | sampler2D | the depth of everything drawn |
| `nb_normalTexture` | sampler2D | world-space normals of deferred surfaces |
| `nb_resolution` | vec2 | the size it draws at, in pixels |
| `nb_time` | float | seconds since the program started |
| `nb_cameraPos` | vec3 | the camera's position |
| `nb_clip` | vec2 | the camera's near and far distance |
| `nb_viewProj` | mat4 | world position → screen position, for this frame |
| `nb_invViewProj` | mat4 | screen position and depth → world position |
| `nb_prevViewProj` | mat4 | last frame's camera, for motion effects |

and use these helpers:

| Function | Returns |
|---|---|
| `nb_texel()` | the size of one pixel in uv units: for reading neighbours |
| `nb_luma(c)` | the brightness of a colour |
| `nb_linearDepth(uv)` | the distance from the camera of what is at uv |
| `nb_worldPosAt(uv)` | the world position of what is at uv |
| `nb_normalAt(uv)` | the world normal at uv (deferred surfaces) |

The shader library (`#include <neptune/noise.glsl>` and the others) works
in effects too.

More examples:

```glsl
// edges.glsl: dark lines where the brightness changes quickly
uniform float strength = 4.0;

//! fragment
vec4 nb_post(vec2 uv) {
    vec2 t = nb_texel();
    float l = nb_luma(texture(nb_screenTexture, uv + vec2(-t.x, 0.0)).rgb);
    float r = nb_luma(texture(nb_screenTexture, uv + vec2(t.x, 0.0)).rgb);
    float u = nb_luma(texture(nb_screenTexture, uv + vec2(0.0, t.y)).rgb);
    float d = nb_luma(texture(nb_screenTexture, uv + vec2(0.0, -t.y)).rgb);
    float edge = clamp((abs(l - r) + abs(u - d)) * strength, 0.0, 1.0);
    return vec4(texture(nb_screenTexture, uv).rgb * (1.0 - edge), 1.0);
}
```

```glsl
// depthtint.glsl: things far away turn blue
uniform vec4 farColor = vec4(0.3, 0.4, 0.8, 1.0);
uniform float farDistance = 60.0;

//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    float d = clamp(nb_linearDepth(uv) / farDistance, 0.0, 1.0);
    return vec4(mix(c, farColor.rgb, d * 0.6), 1.0);
}
```

```glsl
// damage.glsl: a red pulse at the edges; set "hurt" from 0 to 1
uniform float hurt = 0.0;

//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    float edge = smoothstep(0.3, 0.8, length(uv - 0.5) * 1.4);
    return vec4(mix(c, vec3(0.8, 0.0, 0.0), edge * hurt), 1.0);
}
```

`nb_viewProj` finds where a place in the world is on the screen. World
positions inside an effect, including `nb_cameraPos` and `nb_worldPosAt`,
are the engine's, which has `x` the other way round from your program, so
flip `x` on anything your program passes in:

```glsl
// marker.glsl: a ring around a place in the world; set "place" to x, y, z
uniform vec3 place = vec3(0.0, 1.0, 10.0);
uniform float radius = 0.05;

//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    vec4 clip = nb_viewProj * vec4(-place.x, place.y, place.z, 1.0);
    if (clip.w <= 0.0) return vec4(c, 1.0);          // behind the camera
    vec2 p = clip.xy / clip.w * 0.5 + 0.5;
    vec2 aspect = vec2(nb_resolution.x / nb_resolution.y, 1.0);
    float d = abs(length((uv - p) * aspect) - radius);
    return vec4(mix(c, vec3(1.0, 0.8, 0.1), 1.0 - smoothstep(0.002, 0.004, d)), 1.0);
}
```

```basic
damage = LoadPostProcess("damage.glsl")
camera = CreateCamera()
AddPostProcess damage, camera
hurtShader = PostProcessShader(damage)
; when the player is hit: ShaderSetFloat hurtShader, "hurt", 1
```

## The engine's own effects

Fyrox has a few effects of its own, switched with other commands:

| Command | Controls |
|---|---|
| `PostEffect` | Fyrox's bloom, ambient occlusion, anti-aliasing (FXAA), shadows and light scattering, each on or off |
| `GraphicsQuality` | an overall quality preset, low to ultra |
| `CameraExposure` | fixed or automatic exposure for tone mapping |
| `CameraColorGrading` | a colour grading look-up image (256 × 16) |

See [the command reference](../COMMANDS.md) for those. They work
together with the effects in this chapter: Fyrox's bloom and the
`bloom` effect, for example, can both be on.

[Contents](../SHADERS.md) · Previous: [5. The shader language](05-shader-language.md) · Next: [7. Shadows](07-shadows.md)
