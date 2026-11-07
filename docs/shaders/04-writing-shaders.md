# 4. Writing shaders

[Contents](../SHADERS.md) · Previous: [3. Ready-made materials](03-materials.md) · Next: [5. The shader language](05-shader-language.md)

This chapter teaches you to write shaders, one small step at a time.
You don't need to have written one before. Each lesson adds one idea; by
the end you will be able to read the built-in shaders and write your own
effects.

- [Lesson 1: a shader in a string](#lesson-1-a-shader-in-a-string)
- [A little GLSL](#a-little-glsl)
- [Lesson 2: shaders in files](#lesson-2-shaders-in-files)
- [Lesson 3: surfaces and lighting](#lesson-3-surfaces-and-lighting)
- [Lesson 4: parameters](#lesson-4-parameters)
- [Lesson 5: textures](#lesson-5-textures)
- [Lesson 6: animation](#lesson-6-animation)
- [Lesson 7: lighting models](#lesson-7-lighting-models)
- [Lesson 8: glow](#lesson-8-glow)
- [Lesson 9: see-through surfaces](#lesson-9-see-through-surfaces)
- [Lesson 10: shadows and the deferred pass](#lesson-10-shadows-and-the-deferred-pass)
- [Lesson 11: moving vertices](#lesson-11-moving-vertices)
- [Lesson 12: doing the lighting yourself](#lesson-12-doing-the-lighting-yourself)
- [Lesson 13: keywords](#lesson-13-keywords)
- [Recipes](#recipes)

## Lesson 1: a shader in a string

The smallest useful shader colours every pixel of an object the same:

```basic
; a red shader, written in a BASIC string
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 0, -4
ball = CreateSphere(32)

src$ = "//! fragment" + Chr(10)
src$ = src$ + "void main() {" + Chr(10)
src$ = src$ + "    FragColor = vec4(1.0, 0.0, 0.0, 1.0);" + Chr(10)
src$ = src$ + "}"

red = CreateShader(src$)
If CompileShader(red) = 0 Then RuntimeError ShaderError(red)
ShaderApply red, ball

While Not KeyHit(KEY_ESCAPE)
    RenderWorld
    Flip
Wend
```

Step by step:

- `//! fragment` says "the fragment shader starts here". Lines starting
  with `//!` are **directives**: instructions to NeptuneBASIC.
- `void main() { ... }` is the fragment shader's program. The GPU runs it
  once for every pixel of the ball.
- `FragColor` is the colour of the pixel. `vec4(1.0, 0.0, 0.0, 1.0)` is
  red, green, blue and alpha, from 0 to 1: solid red.
- `CreateShader` makes the shader, `CompileShader` turns it into a GPU
  program (and returns 0 if there is a mistake, with the reason in
  `ShaderError`), and `ShaderApply` draws the ball with it.

There is no vertex shader: when you leave it out, NeptuneBASIC uses its
standard one, which places the vertices where the entity is (and handles
animated characters).

The ball is a flat red disc: there is no lighting yet. Lesson 3 fixes
that.

## A little GLSL

Shaders are written in GLSL. It looks like C: statements end with `;`,
blocks go in `{ }`, comments start with `//`. The parts you need most:

| GLSL | Means |
|---|---|
| `float x = 0.5;` | a number. Write `1.0`, not `1`, for numbers with a fractional part in mind |
| `int n = 3;` | a whole number |
| `vec2 uv = vec2(0.5, 0.25);` | two numbers (a position on a texture) |
| `vec3 c = vec3(1.0, 0.5, 0.0);` | three numbers (a colour, a position, a direction) |
| `vec4 c = vec4(c3, 1.0);` | four numbers (a colour with alpha) |
| `c.r`, `c.g`, `c.b`, `c.a` | the parts of a colour (also `.x .y .z .w`) |
| `c.rgb` | the first three parts, as a `vec3` |
| `c * 0.5` | maths works on whole vectors: this halves every part |
| `a * b` | with two vectors, multiplies part by part |

Useful functions (they work on numbers and on vectors):

| Function | Gives |
|---|---|
| `sin(x)`, `cos(x)` | waves between -1 and 1 (x in radians) |
| `abs(x)` | x without its sign |
| `fract(x)` | the part after the point: repeats 0 → 1, 0 → 1 ... |
| `floor(x)` | x rounded down |
| `mod(x, y)` | remainder |
| `min(a, b)`, `max(a, b)` | the smaller, the larger |
| `clamp(x, lo, hi)` | x kept between lo and hi |
| `mix(a, b, t)` | a when t is 0, b when t is 1, in between otherwise |
| `step(edge, x)` | 0 below edge, 1 above |
| `smoothstep(lo, hi, x)` | 0 below lo, 1 above hi, a smooth curve between |
| `length(v)` | the length of a vector |
| `distance(a, b)` | the distance between two points |
| `dot(a, b)` | how much two directions agree: 1 same, 0 at right angles, -1 opposite |
| `normalize(v)` | v made length 1 |
| `pow(x, y)` | x to the power y |
| `texture(tex, uv)` | the colour of a texture at uv |

A few rules that catch beginners:

- Every statement ends with `;`.
- Types must match: `vec3 c = 1.0;` is an error; write `vec3 c = vec3(1.0);`.
- Mixing `int` and `float` in one sum is allowed, but writing `1.0` keeps
  intent clear.
- Variables are declared before they are used.

## Lesson 2: shaders in files

Shaders are easier to write in their own files. Save this as `red.glsl`:

```glsl
// red.glsl
//! fragment
void main() {
    FragColor = vec4(1.0, 0.0, 0.0, 1.0);
}
```

and load it:

```basic
red = LoadShader("red.glsl")
ShaderHotReload red          ; recompile whenever red.glsl is saved
ShaderApply red, ball
```

With `ShaderHotReload`, you can keep the program running and edit the
file: every time you save, the shader is compiled again and the ball
changes. If you make a mistake, the error is printed (with the line
number), and the last version that worked stays on screen.

Every lesson from here on is a file you can hot-reload this way.

## Lesson 3: surfaces and lighting

Writing `FragColor` yourself means doing everything yourself, including
lighting. Usually you only want to say what the surface is like and let
NeptuneBASIC light it. That is what **surface shaders** do:

```glsl
// orange.glsl: a lit orange surface
//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = vec3(1.0, 0.5, 0.1);
}
```

Instead of `main`, write a function called `nb_surface`. It gets a
surface `s` with sensible starting values; change the ones you care
about. NeptuneBASIC adds everything else: the lights in the scene, the
ambient light, fog, shadows (lesson 10), and the shadow-casting version of
the object.

The surface has these fields:

| Field | Type | Starts as | Meaning |
|---|---|---|---|
| `s.albedo` | vec3 | white | the colour, 0-1 |
| `s.alpha` | float | 1 | opacity (lesson 9) |
| `s.normal` | vec3 | the surface's normal | the direction it faces |
| `s.metallic` | float | 0 | metal or not |
| `s.roughness` | float | 0.6 | matte (1) or shiny (0) |
| `s.ao` | float | 1 | darkening in creases |
| `s.emission` | vec3 | black | light the surface gives off (lesson 8) |
| `s.alphaCutoff` | float | 0.5 | for cut-out transparency |
| `s.specularColor` | vec3 | 0.5 grey | colour of highlights (Phong, Blinn, toon) |
| `s.shininess` | float | 32 | size of highlights: higher is smaller and sharper |
| `s.toonSteps` | float | 3 | toon light bands |
| `s.shadeColor` | vec3 | blue-grey | toon: colour of the dark side |

You can also read where the pixel is:

| Name | Type | Meaning |
|---|---|---|
| `nb_uv` | vec2 | texture position (the mesh's UVs) |
| `nb_uv2` | vec2 | the second set of UVs |
| `nb_worldPos` | vec3 | position in the world |
| `nb_normal` | vec3 | the surface direction, in the world |
| `nb_color` | vec4 | the vertex colour |

Try a stripy surface:

```glsl
// stripes.glsl
//! fragment
void nb_surface(inout NbSurface s) {
    // fract repeats 0..1; step makes it a hard edge half way
    float stripe = step(0.5, fract(nb_uv.x * 10.0));
    s.albedo = mix(vec3(1.0, 1.0, 1.0), vec3(0.8, 0.1, 0.1), stripe);
}
```

## Lesson 4: parameters

Parameters let BASIC change a shader without editing it. Declare them at
the top with `uniform`, give them a starting value, and use them:

```glsl
// stripes2.glsl: colours and count set from BASIC
uniform vec4 colorA = vec4(1.0, 1.0, 1.0, 1.0);
uniform vec4 colorB = vec4(0.8, 0.1, 0.1, 1.0);
uniform float count = 10.0;

//! fragment
void nb_surface(inout NbSurface s) {
    float stripe = step(0.5, fract(nb_uv.x * count));
    s.albedo = mix(colorA.rgb, colorB.rgb, stripe);
}
```

```basic
stripes = LoadShader("stripes2.glsl")
ShaderSetColor stripes, "colorB", 30, 90, 255     ; blue stripes (0-255)
ShaderSetFloat stripes, "count", 4                ; four of them
ShaderApply stripes, ball
```

Notice where the `uniform` lines are: before `//! fragment`. Anything
before the first stage marker is **common**: it goes into both the vertex
and the fragment shader.

The `ShaderSet...` command must match the parameter's type:

| GLSL type | BASIC command |
|---|---|
| `float` | `ShaderSetFloat s, "name", 1.5` |
| `int`, `uint` | `ShaderSetInt s, "name", 3` |
| `bool` | `ShaderSetBool s, "name", True` |
| `vec2` | `ShaderSetVector2 s, "name", x, y` |
| `vec3` | `ShaderSetVector3 s, "name", x, y, z` or `ShaderSetColor s, "name", r, g, b` |
| `vec4` | `ShaderSetVector4 s, "name", x, y, z, w` or `ShaderSetColor s, "name", r, g, b [, a]` |
| `mat3`, `mat4` | `ShaderSetMatrix3`, `ShaderSetMatrix4` (numbers in a string) |
| arrays (`float x[8]`) | `ShaderSetBuffer s, "name", "1, 2, 3"` |
| `sampler2D` | `ShaderSetTexture s, "name", texture` |
| `samplerCube` | `ShaderSetCubeMap s, "name", texture` |

`ShaderSetColor` takes 0-255 and the shader gets 0-1. A name the shader
does not declare is an error (`ShaderGetUniforms` lists the names it
does), and so is giving a texture to a number parameter or numbers to a
texture.

## Lesson 5: textures

A `sampler2D` parameter is a texture. `texture(name, uv)` reads its colour:

```glsl
// tinted.glsl: a texture, tinted
uniform sampler2D picture;
uniform vec4 tint = vec4(1.0, 1.0, 1.0, 1.0);
uniform float tiles = 1.0;

//! fragment
void nb_surface(inout NbSurface s) {
    vec4 t = texture(picture, nb_uv * tiles);   // * tiles repeats it
    s.albedo = t.rgb * tint.rgb;
}
```

```basic
tinted = LoadShader("tinted.glsl")
ShaderSetTexture tinted, "picture", LoadTexture("examples/media/checker.png")
ShaderSetFloat tinted, "tiles", 4
ShaderApply tinted, ball
```

A texture parameter that has not been set reads as white. (You can
change that: see `//! fallback` in [chapter 5](05-shader-language.md#parameters).)

Several textures work the same way: declare more `sampler2D` lines and
mix them however you like.

## Lesson 6: animation

`nb_time` is the number of seconds since the program started. Anything
that uses it moves:

```glsl
// lava.glsl: scrolling, pulsing lava
#include <neptune/noise.glsl>
uniform float speed = 0.2;

//! fragment
void nb_surface(inout NbSurface s) {
    vec2 p = nb_uv * 6.0 + vec2(0.0, nb_time * speed);   // scroll upwards
    float n = nb_fbm(p, 5);                               // layered noise, 0-1
    vec3 dark = vec3(0.3, 0.02, 0.0);
    vec3 hot = vec3(1.0, 0.6, 0.1);
    s.albedo = mix(dark, hot, n);
    // the hot parts glow, and pulse gently
    s.emission = hot * smoothstep(0.55, 0.8, n) * (1.5 + 0.5 * sin(nb_time * 2.0));
}
```

`#include <neptune/noise.glsl>` brings in NeptuneBASIC's noise functions
(`nb_fbm` adds layers of noise together). The whole library is listed in
[chapter 5](05-shader-language.md#the-function-library).

Other values the engine fills in for you: `nb_deltaTime` (length of the
last frame), `nb_resolution` (window size in pixels), `nb_ambient` (the
`AmbientLight` colour) and the camera fog.

To freeze time, or drive it yourself (for replays, or a slow-motion
effect), use `ShaderSetTime s, seconds`; `ShaderSetTime s, -1` goes back
to the clock.

## Lesson 7: lighting models

A surface shader is lit with the realistic PBR model unless you choose
another one with `//! lighting`:

```glsl
// toonball.glsl: cartoon lighting
//! lighting toon
uniform vec4 color = vec4(0.3, 0.7, 1.0, 1.0);
uniform float bands = 3.0;

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = color.rgb;
    s.toonSteps = bands;                // number of light bands
    s.shadeColor = vec3(0.2, 0.2, 0.5); // the dark side is dark blue
    s.specularColor = vec3(1.0);        // a white highlight...
    s.shininess = 64.0;                 // ...and a small one
}
```

| `//! lighting` | Looks like | Uses |
|---|---|---|
| `pbr` (default) | realistic | `albedo`, `metallic`, `roughness`, `ao` |
| `lambert` | matte | `albedo` |
| `phong` | shiny, classic | `albedo`, `specularColor`, `shininess` |
| `blinn` | shiny, softer highlights | `albedo`, `specularColor`, `shininess` |
| `toon` | flat bands | `albedo`, `toonSteps`, `shadeColor`, `specularColor`, `shininess` |
| `unlit` | no lighting: the colour as it is | `albedo` |

## Lesson 8: glow

`s.emission` is light the surface gives off by itself. It is added on
top of the lit colour, is never shadowed, and, if it is bright enough
(above 1), makes bloom.

```glsl
// neon.glsl: glowing stripes on a dark surface
uniform vec4 glow = vec4(0.1, 1.0, 0.8, 1.0);
uniform float brightness = 3.0;

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = vec3(0.05);
    float line = smoothstep(0.45, 0.5, abs(fract(nb_uv.y * 8.0) - 0.5));
    // emission is linear light: nb_linear converts a 0-1 colour to it
    s.emission = nb_linear(glow.rgb) * line * brightness;
}
```

Emission is in **linear** light, the scale lighting maths uses, while
colours you pick by eye (and `ShaderSetColor` values) are **sRGB**.
`nb_linear(colour)` converts. `s.albedo` is sRGB, like textures, so it
needs no conversion.

Add a bloom effect to see the glow spread:
`AddPostProcess LoadPostProcess("builtin:bloom"), camera`.

## Lesson 9: see-through surfaces

To see through a surface, turn blending on and set `s.alpha`:

```glsl
// ghost.glsl: see-through, more solid at the edges
//! blend alpha
uniform vec4 color = vec4(0.6, 0.8, 1.0, 1.0);

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = color.rgb;
    // nb_fresnel is near 0 facing the camera and near 1 at the edges
    s.alpha = 0.15 + 0.7 * nb_fresnel(s.normal, 2.0);
}
```

`//! blend` chooses how the surface mixes with what is behind it:

| `//! blend` | Effect | Good for |
|---|---|---|
| `none` (default) | solid | most things |
| `alpha` | mix by `s.alpha` | glass, ghosts, water |
| `add` | add its light to what is behind | fire, lasers, glows, magic |
| `multiply` | darken what is behind | stains, tinted glass, shadows |
| `premultiplied` | alpha with the colour already multiplied by alpha | particles from premultiplied textures |

For holes with hard edges (leaves, fences), cut the surface instead of
blending it. Cut-out surfaces stay solid, so they sort correctly and
cast shadows with the holes in them:

```glsl
// fence.glsl: holes from a texture's alpha
//! alpha_clip on
//! shadows on
uniform sampler2D picture;

//! fragment
void nb_surface(inout NbSurface s) {
    vec4 t = texture(picture, nb_uv);
    s.albedo = t.rgb;
    s.alpha = t.a;          // below s.alphaCutoff (0.5) is cut away
}
```

## Lesson 10: shadows and the deferred pass

Two directives control shadows:

- `//! shadows on` makes the object **cast** shadows.
- `//! pass gbuffer` draws it in the **deferred** pass, where it is lit by
  the engine's PBR lighting and **receives** shadows.

```glsl
// rock.glsl: a rock surface that casts and receives shadows
//! pass gbuffer
//! shadows on
#include <neptune/noise.glsl>

//! fragment
void nb_surface(inout NbSurface s) {
    float n = nb_fbm(nb_worldPos.xz * 0.8, 6);
    s.albedo = mix(vec3(0.35, 0.32, 0.3), vec3(0.6, 0.57, 0.52), n);
    s.roughness = 0.9;
}
```

The two passes are:

| | `forward` (default) | `gbuffer` |
|---|---|---|
| lighting | the lighting model you choose, from the 16 most important lights | PBR, from every light |
| receives shadows | no | yes |
| see-through (`//! blend`) | yes | no (it switches to forward, with a warning) |
| custom lighting (`//! lighting toon`, a fragment `main`, ...) | yes | no: always PBR |

So: realistic solid objects that should be shadowed, use `gbuffer`.
Stylized or see-through objects, use `forward`.

## Lesson 11: moving vertices

So far the shader has only coloured pixels. A **vertex shader** can move
the vertices themselves: waves, wobbling jelly, swaying plants, pulsing
hearts.

You don't have to write a whole vertex shader. Add a vertex stage with a
function called `nb_displace`, and NeptuneBASIC calls it for every vertex:

```glsl
// wobble.glsl: a wobbling, pulsing ball
uniform float amount = 0.1;
uniform float speed = 3.0;

//! vertex
void nb_displace(inout vec3 position, inout vec3 normal, vec2 uv) {
    // push each vertex out along its normal, in waves up the object
    position += normal * sin(position.y * 6.0 + nb_time * speed) * amount;
}

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = vec3(0.4, 1.0, 0.5);
}
```

`position` and `normal` are in the object's own space, before it is
moved, turned or scaled. Use a sphere with plenty of segments
(`CreateSphere(48)`): only vertices move, so there must be enough of them.

For effects that must line up between objects, like waves across many
pieces of sea or wind across a field, use `nb_displaceWorld` instead: it
gets world positions.

```glsl
// waves.glsl: waves that line up across every tile using the shader
//! vertex
void nb_displaceWorld(inout vec3 position, inout vec3 normal, vec2 uv) {
    position.y += sin(position.x * 0.5 + nb_time) * 0.3
                + sin(position.z * 0.7 + nb_time * 1.3) * 0.2;
}

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = vec3(0.1, 0.4, 0.6);
    s.roughness = 0.2;
}
```

Shadows use the same function, so moved objects cast moved shadows, and
the standard vertex program applies skeletal animation first, so it works
on animated characters too.

## Lesson 12: doing the lighting yourself

To light a surface your own way, write the fragment `main` yourself and
loop over the lights:

```glsl
// hardlight.glsl: lighting in two hard steps, from every light
#include <neptune/lighting.glsl>
uniform vec4 color = vec4(1.0, 0.8, 0.6, 1.0);

//! fragment
void main() {
    vec3 N = normalize(nb_normal);
    vec3 light = nb_linear(nb_ambient);              // start with the ambient light
    for (int i = 0; i < nb_lightCount(); ++i) {
        NbLight l = nb_light(i, nb_worldPos);        // light number i, seen from here
        float facing = dot(N, l.L);                  // l.L points towards the light
        light += l.color * l.atten * step(0.2, facing);
    }
    FragColor = vec4(nb_linear(color.rgb) * light, 1.0);
}
```

- `nb_lightCount()` is the number of lights affecting the object (up to
  16).
- `nb_light(i, position)` gives light `i`: `l.L` (the direction towards
  it), `l.color` (its colour, linear) and `l.atten` (how much of it
  arrives: distance and spot cone included).
- `FragColor` is **linear** colour: the engine draws in HDR and turns it
  into screen colour afterwards. So colours you picked are converted with
  `nb_linear`.

`nb_lambertTerm`, `nb_blinnTerm`, `nb_toonTerm` and `nb_pbrTerm` in the
lighting library do the maths of each lighting model for one light, if
you want to start from one.

A complete vertex shader can be written the same way; see
[chapter 5](05-shader-language.md#writing-the-vertex-main).

## Lesson 13: keywords

A **keyword** is a switch that turns parts of a shader on and off. In
the shader, test it with `#ifdef`:

```glsl
// crystal.glsl: GLOW and SEETHROUGH can be switched on from BASIC
//! blend alpha if SEETHROUGH
uniform vec4 color = vec4(0.5, 0.2, 1.0, 1.0);

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = color.rgb;
#ifdef GLOW
    s.emission = nb_linear(color.rgb) * 2.0;
#endif
#ifdef SEETHROUGH
    s.alpha = 0.4;
#endif
}
```

```basic
crystal = LoadShader("crystal.glsl")
ShaderSetKeyword crystal, "GLOW", True        ; glow on
ShaderSetKeyword crystal, "SEETHROUGH", True  ; see-through on
ShaderSetKeyword crystal, "GLOW", False       ; glow off again
```

A directive can depend on a keyword too: `//! blend alpha if SEETHROUGH`
only blends when the keyword is on (`if !NAME` when it is off).

Keywords are better than an `if` on a parameter when the switch changes
a lot: each combination is compiled into its own GPU program the first
time it is used, and switching back to one already used costs nothing.

## Recipes

Small shaders to copy and change.

**Rim glow** (a glowing edge, like a force field or a selected unit):

```glsl
// rimglow.glsl
uniform vec4 rimColor = vec4(0.3, 0.8, 1.0, 1.0);
uniform float power = 3.0;

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = vec3(0.1);
    s.emission = nb_linear(rimColor.rgb) * nb_fresnel(s.normal, power) * 2.0;
}
```

**Height colours** (a landscape coloured by height):

```glsl
// heightcolors.glsl
uniform float low = 0.0;
uniform float high = 20.0;

//! fragment
void nb_surface(inout NbSurface s) {
    float h = clamp((nb_worldPos.y - low) / (high - low), 0.0, 1.0);
    vec3 grass = vec3(0.3, 0.55, 0.2);
    vec3 rock = vec3(0.5, 0.45, 0.4);
    vec3 snow = vec3(1.0);
    s.albedo = h < 0.6 ? mix(grass, rock, h / 0.6) : mix(rock, snow, (h - 0.6) / 0.4);
}
```

**Scrolling texture** (conveyor belts, rivers, screens):

```glsl
// scroll.glsl
uniform sampler2D picture;
uniform vec2 speed = vec2(0.0, 0.5);

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = texture(picture, nb_uv + speed * nb_time).rgb;
}
```

**Damage flash** (set `flash` to 1 when hit, and let it fall to 0):

```glsl
// flash.glsl
uniform sampler2D picture;
uniform float flash = 0.0;

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = texture(picture, nb_uv).rgb;
    s.emission = vec3(4.0, 0.4, 0.3) * flash;
}
```

```basic
; when the enemy is hit: flash# = 1
; every frame:
flash# = flash# * 0.9
ShaderSetFloat enemyShader, "flash", flash#
```

**Grid floor** (a level-editor style grid):

```glsl
// gridfloor.glsl
#include <neptune/pattern.glsl>
uniform float cell = 1.0;

//! fragment
void nb_surface(inout NbSurface s) {
    float g = nb_grid(nb_worldPos.xz / cell, 0.03);
    s.albedo = mix(vec3(0.2), vec3(0.9), g);
}
```

**Two textures blended by height** (a path through grass):

```glsl
// blend2.glsl
uniform sampler2D grass;
uniform sampler2D path;
uniform sampler2D mask;    // white where the path is

//! fragment
void nb_surface(inout NbSurface s) {
    float m = texture(mask, nb_uv).r;
    vec3 a = texture(grass, nb_uv * 8.0).rgb;
    vec3 b = texture(path, nb_uv * 8.0).rgb;
    s.albedo = mix(a, b, m);
}
```

[Contents](../SHADERS.md) · Previous: [3. Ready-made materials](03-materials.md) · Next: [5. The shader language](05-shader-language.md)
