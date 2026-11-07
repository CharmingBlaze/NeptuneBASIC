# 5. The shader language

[Contents](../SHADERS.md) · Previous: [4. Writing shaders](04-writing-shaders.md) · Next: [6. Post-processing](06-post-processing.md)

This chapter is the reference for NeptuneBASIC's shader language: GLSL
3.30 with a few additions that write the repetitive parts for you. If you
are new to shaders, do [chapter 4](04-writing-shaders.md) first.

- [The layout of a shader](#the-layout-of-a-shader)
- [Directives](#directives)
- [Passes](#passes)
- [Surface shaders](#surface-shaders)
- [Fragment main](#fragment-main)
- [Vertex hooks](#vertex-hooks)
- [Writing the vertex main](#writing-the-vertex-main)
- [Parameters](#parameters)
- [Values filled in for you](#values-filled-in-for-you)
- [Vertex attributes](#vertex-attributes)
- [Helpers](#helpers)
- [The function library](#the-function-library)
- [Including files](#including-files)
- [Keywords](#keywords)
- [Lighting](#lighting)
- [Fyrox built-ins](#fyrox-built-ins)
- [Coordinates in shaders](#coordinates-in-shaders)

## The layout of a shader

```glsl
// pulse.glsl
//! lighting blinn                      // 1. directives (options)
//! shadows on
#include <neptune/noise.glsl>           // 2. includes

uniform vec4 tint = vec4(1.0, 0.5, 0.2, 1.0);   // 3. parameters
uniform float speed = 2.0;
uniform sampler2D pattern;

float wave(float x) { return 0.5 + 0.5 * sin(x); }   // 4. shared functions

//! vertex                              // 5. the vertex stage (optional)
void nb_displace(inout vec3 position, inout vec3 normal, vec2 uv) {
    position += normal * wave(position.y * 4.0 + nb_time * speed) * 0.05;
}

//! fragment                            // 6. the fragment stage
void nb_surface(inout NbSurface s) {
    vec3 p = texture(pattern, nb_uv).rgb;
    s.albedo = tint.rgb * p * (0.7 + 0.3 * wave(nb_time * speed));
    s.shininess = 64.0;
}
```

- Everything before the first `//! vertex` or `//! fragment` line is
  **common**: it is put into both stages. Parameters, includes and
  functions both stages use go there. `//! common` goes back to the
  common part later in the file.
- `//! vertex` starts the vertex stage. Leave it out to use the standard
  vertex program (it handles static and animated meshes, shadows and
  instancing). With the standard program, a vertex stage holds only hook
  functions (`nb_displace`, `nb_displaceWorld`); with a `main` it replaces
  the standard program.
- `//! fragment` (or `//! pixel`) starts the fragment stage. It holds
  either `void nb_surface(inout NbSurface s)` or `void main()`.
- A shader needs a fragment stage.

The same text can be loaded from a file (`LoadShader("pulse.glsl")`),
given as a string (`CreateShader(text$)`), or built stage by stage
(`ShaderSource`, `ShaderVertexSource`, `ShaderFragmentSource`).

## Directives

Directives are lines starting with `//!`. They can go anywhere, but are
usually at the top.

| Directive | Values | Default | Effect |
|---|---|---|---|
| `//! pass` | `forward`, `gbuffer` (or `deferred`), `post` | `forward` | which pass the shader draws in: see [Passes](#passes) |
| `//! lighting` | `pbr`, `lambert` (or `diffuse`), `phong`, `blinn`, `toon` (or `cel`), `unlit` (or `none`) | `pbr` | the lighting model for `nb_surface` in the forward pass |
| `//! blend` | `none`, `alpha`, `add`, `multiply`, `premultiplied` | `none` | how the surface mixes with what is behind it |
| `//! cull` | `back`, `front`, `none` | `back` | which sides of triangles are not drawn; `none` draws both (leaves, cloth, glass) |
| `//! depth_write` | `on`, `off` | on, off when blending | whether the surface hides things drawn after it |
| `//! depth_test` | `on`, `off` | `on` | whether things in front hide the surface; `off` draws it on top of everything |
| `//! shadows` | `on`, `off` | `off` | whether the surface casts shadows |
| `//! alpha_clip` | `on`, `off` | `off` | throw away pixels with `s.alpha` below `s.alphaCutoff` |
| `//! fallback name kind` | a texture name, then `white`, `black` or `normal` | `white` | what an unset texture reads as |
| `//! default name values` | a parameter name, then numbers | | its starting value (mainly for WGSL, where declarations cannot have one) |
| `//! include "file"` | a file name | | the same as `#include` |
| `//! vertex`, `//! fragment`, `//! common` | | | start a part of the file |

`on`/`off` can also be `true`/`false`, `yes`/`no` or `1`/`0`.

Any directive can end with `if NAME` or `if !NAME` to apply only when a
[keyword](#keywords) is on, or off:

```glsl
//! blend alpha if GHOST
//! shadows on if !GHOST
```

Geometry and compute stages (`//! geometry`, `//! compute`) are errors
that explain why: see [chapter 1](01-introduction.md#what-the-engine-can-do).

## Passes

A pass is one way of drawing an object. NeptuneBASIC writes the passes a
shader needs from its directives.

| Pass | Lit by | Shadows | Blending | Use for |
|---|---|---|---|---|
| `forward` | the `//! lighting` model, from the 16 most important visible lights, plus ambient and fog | casts (with `//! shadows on`), does not receive | yes | stylized looks, see-through surfaces, custom lighting, unlit effects |
| `gbuffer` | the engine's PBR lighting, from every light, plus the sky box or reflection probe with `EnvironmentLighting 1` | casts and receives | no | realistic solid surfaces |
| `post` | not lit: a screen effect | | | [chapter 6](06-post-processing.md) |

The forward pass is drawn after the solid scene, so it can read the
scene's depth (`fyrox_sceneDepth`), which water and force fields use to
find where they meet other objects.

A `gbuffer` shader with `//! blend` is moved to the forward pass, with a
note in `ShaderLog`. In the `gbuffer` pass, use `nb_surface`: a fragment
`main` there would have to write Fyrox's G-buffer outputs itself. The
lighting fields that do not belong to PBR (`shininess`, `toonSteps`, ...)
are ignored there.

With `//! shadows on`, the shader also gets passes for the shadow maps of
directional, spot and point lights. They use its vertex hooks (so moved
vertices cast moved shadows) and its `nb_surface` when `//! alpha_clip` is
on (so cut-out holes cast holes).

## Surface shaders

A surface shader describes the surface, and NeptuneBASIC lights it:

```glsl
//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = vec3(0.8, 0.2, 0.2);
    s.roughness = 0.3;
}
```

The function gets a surface with default values and changes them:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `albedo` | vec3 | `1, 1, 1` | colour, sRGB 0-1 (like textures and colours from BASIC) |
| `alpha` | float | `1` | opacity; used with `//! blend` or `//! alpha_clip on` |
| `normal` | vec3 | the interpolated normal, flipped on back faces | world-space direction the surface faces; change it for bump effects (`nb_normalFromMap`) |
| `metallic` | float | `0` | PBR: 0 not metal, 1 metal |
| `roughness` | float | `0.6` | PBR: 0 mirror, 1 matte |
| `ao` | float | `1` | ambient occlusion: 0 fully occluded |
| `emission` | vec3 | `0, 0, 0` | linear light added on top; above 1 makes bloom |
| `alphaCutoff` | float | `0.5` | with `//! alpha_clip on`, pixels with alpha below this are not drawn |
| `specularColor` | vec3 | `0.5, 0.5, 0.5` | highlight colour (phong, blinn, toon) |
| `shininess` | float | `32` | highlight size (phong, blinn, toon): higher is smaller |
| `toonSteps` | float | `3` | toon: number of light bands |
| `shadeColor` | vec3 | `0.45, 0.45, 0.6` | toon: colour the dark side is multiplied by |

In the fragment stage (with the standard vertex program) these values
are available, interpolated across each triangle:

| Name | Type | Meaning |
|---|---|---|
| `nb_uv` | vec2 | first texture coordinates |
| `nb_uv2` | vec2 | second texture coordinates (lightmaps) |
| `nb_worldPos` | vec3 | world position |
| `nb_normal` | vec3 | world-space normal (not normalized after interpolation: use `normalize`) |
| `nb_tangent`, `nb_binormal` | vec3 | world-space tangent and binormal, for normal maps |
| `nb_color` | vec4 | vertex colour, 0-1 |
| `nb_data0` ... `nb_data3` | vec4 | the extra values set with `VertexData` |

## Fragment main

Instead of `nb_surface`, the fragment stage can have its own `main`. It
writes `FragColor` (declared for you), in **linear** colour: the scene is
drawn in HDR and tone mapped afterwards. Use `nb_linear` on colours
picked in sRGB.

```glsl
//! fragment
void main() {
    float d = length(nb_uv - 0.5);
    FragColor = vec4(nb_linear(vec3(1.0, 0.5, 0.0)) * (1.0 - d), 1.0);
}
```

A fragment `main` is drawn in the forward pass. `nb_shade(surface)` in
the lighting library lights an `NbSurface` with the shader's lighting
model, if you want to do part of the work yourself:

```glsl
#include <neptune/lighting.glsl>
//! lighting toon
//! fragment
void main() {
    NbSurface s = nb_defaultSurface();
    s.albedo = vec3(0.9, 0.4, 0.2);
    vec3 c = nb_shade(s);                 // lit with the toon model
    FragColor = vec4(nb_applyFog(c), 1.0);
}
```

## Vertex hooks

With the standard vertex program, a vertex stage can define either or
both of these functions:

```glsl
//! vertex
// object space: before the entity's position, rotation and scale
void nb_displace(inout vec3 position, inout vec3 normal, vec2 uv) {
    position += normal * 0.1 * sin(nb_time + position.y * 5.0);
}
// world space: after them
void nb_displaceWorld(inout vec3 position, inout vec3 normal, vec2 uv) {
    position.y += sin(position.x + nb_time) * 0.2;
}
```

- `nb_displace` runs first, in the object's own space, after skeletal
  animation. Use it for effects that belong to the object (wobble, pulse).
- `nb_displaceWorld` runs in world space. Use it for effects that must
  match up between objects (waves, wind).
- Both may change `normal` too, so lighting follows the new shape.
- Shadow passes run the same hooks.
- The vertex stage can read the [vertex attributes](#vertex-attributes)
  and parameters as well as `uv`.

## Writing the vertex main

For full control, write the vertex `main` yourself. Declare the
attributes you need (or include them all), compute `gl_Position`, and
pass values to the fragment stage with `out` / `in`:

```glsl
// uvcolors.glsl: a complete vertex and fragment shader
//! vertex
#include <neptune/attributes.glsl>    // vertexPosition, vertexNormal, ...
#include <neptune/skinning.glsl>      // nb_skinMatrix()
out vec2 uv;
void main() {
    vec4 p = nb_skinMatrix() * vec4(vertexPosition, 1.0);   // skeletal animation
    uv = vertexTexCoord;
    gl_Position = fyrox_instanceData.worldViewProjection * p;
}

//! fragment
in vec2 uv;
void main() {
    FragColor = vec4(uv, 0.0, 1.0);
}
```

- `fyrox_instanceData.worldViewProjection` takes a position from object
  space to the screen; `fyrox_instanceData.worldMatrix` takes it to world
  space.
- `nb_skinMatrix()` is the identity for meshes without bones, so it is
  safe to use always.
- With your own vertex main, the `nb_*` interpolated values and the
  helpers that use them (`nb_fresnel`, `nb_viewDir`, ...) are not
  declared: pass what you need yourself.
- Instanced drawing needs nothing special: Fyrox fills
  `fyrox_instanceData` for each instance.

## Parameters

Every `uniform` line becomes a parameter:

```glsl
uniform float speed = 2.0;                  // float, int, uint, bool
uniform vec4 tint = vec4(1.0, 0.5, 0.2, 1.0);  // vec2, vec3, vec4
uniform mat4 projector;                     // mat3, mat4 (identity by default)
uniform float weights[16];                  // arrays: float, int, vec2-4, mat4
uniform sampler2D detail;                   // a texture
uniform sampler2D bumpNormal; //! fallback normal
uniform sampler3D volume;                   // a 3D texture
uniform samplerCube sky;                    // a cube map
uniform sampler2DArray layers;              // an array texture
```

- The value after `=` is the default. Without one, numbers start at 0 and
  matrices at the identity.
- Use parameters by name in the code. NeptuneBASIC gathers them into a
  uniform block for you, so they are not real GLSL `uniform` variables:
  they cannot be assigned to.
- A texture that is not set reads as **white**. Names containing
  "normal" read as a **flat normal** instead. `//! fallback black` (after
  the declaration, or as `//! fallback name black` on its own line)
  chooses black, white or normal.
- `sampler2DArray` textures are stored as 3D textures, because Fyrox has
  no array sampler: read them with `nb_textureArray(name, uv, layer)` and
  set them with `ShaderSetTextureArray`.

From BASIC, each type has its command (see
[lesson 4](04-writing-shaders.md#lesson-4-parameters)), or use
`ShaderSetParameter s, name$, value$` for any type from text.
`ShaderGetParameter$(s, name$)` reads a value back. Setting a parameter
the shader does not declare, or giving a texture to a number parameter
(or numbers to a texture), is an error that names the parameter.

**Arrays** are set from a list of numbers: `ShaderSetBuffer s, "weights",
"0.1, 0.2, 0.4"`. For an array of vectors, give all their numbers one
after another. Each call replaces the whole array.

**Matrices** are 9 or 16 numbers, column by column:
`ShaderSetMatrix4 s, "m", "1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1"`.

## Values filled in for you

These parameters are set every frame by the engine, for any shader that
uses them. You do not declare them.

| Name | Type | Value |
|---|---|---|
| `nb_time` | float | seconds since the program started (`ShaderSetTime` overrides it) |
| `nb_deltaTime` | float | length of the last frame, in seconds |
| `nb_resolution` | vec2 | the window size in pixels (`ShaderSetResolution` overrides it) |
| `nb_ambient` | vec3 | the `AmbientLight` colour, sRGB 0-1 |
| `nb_fogColor` | vec3 | the camera's fog colour, sRGB 0-1 |
| `nb_fogRange` | vec2 | the camera's fog near and far distance; 0, 0 means no fog |

`ShaderSetCamera` and `ShaderSetLight` fill these, if the shader
declares them (as single values, or arrays for several lights):

| Name | Type | Value |
|---|---|---|
| `nb_cameraPosition` | vec3 | camera position |
| `nb_viewMatrix`, `nb_projectionMatrix`, `nb_viewProjection` | mat4 | camera matrices |
| `nb_lightPosition` | vec3 | light position |
| `nb_lightDirection` | vec3 | direction towards the light |
| `nb_lightColor` | vec3 | light colour, 0-1 |
| `nb_lightRange` | float | light range |

## Vertex attributes

Values stored in each vertex of a mesh, read in the vertex stage:

| Name | Type | Location | Holds |
|---|---|---|---|
| `vertexPosition` | vec3 | 0 | position |
| `vertexTexCoord` | vec2 | 1 | first UV set |
| `vertexNormal` | vec3 | 2 | normal |
| `vertexTangent` | vec4 | 3 | tangent; w is ±1 (handedness) |
| `boneWeights` | vec4 | 4 | skinning weights |
| `boneIndices` | vec4 | 5 | skinning bones |
| `vertexSecondTexCoord` | vec2 | 6 | second UV set |
| `vertexColor` | vec4 | 7 | vertex colour (`VertexColor`), 0-1 |
| `vertexData0` ... `vertexData3` | vec4 | 8-11 | extra values from `VertexData` |

`VertexData surface, vertex, slot, x [, y, z, w]` stores up to four extra
`vec4`s per vertex (slots 0-3), for anything a shader needs per vertex:
wind weights, colours for a second layer, animation phases.

## Helpers

Always available in the fragment stage (with the standard vertex program):

| Function | Returns |
|---|---|
| `nb_linear(c)` | an sRGB colour (vec3) converted to linear |
| `nb_srgb(c)` | a linear colour converted to sRGB |
| `nb_viewDir()` | the direction from the pixel towards the camera |
| `nb_fresnel(normal, power)` | 0 facing the camera, rising to 1 at the edges; higher power keeps it to the edges |
| `nb_normalFromMap(sample, strength)` | a world-space normal from a normal-map texture sample (`texture(map, nb_uv).rgb`) |
| `nb_screenUV()` | the pixel's position on the screen, 0-1 |
| `nb_textureArray(tex, uv, layer)` | layer `layer` of an array texture |
| `nb_defaultSurface()` | an `NbSurface` with the default values |

A normal-mapped surface:

```glsl
// bumpy.glsl
uniform sampler2D colorMap;
uniform sampler2D normalMap;
uniform float strength = 1.0;

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = texture(colorMap, nb_uv).rgb;
    s.normal = nb_normalFromMap(texture(normalMap, nb_uv).rgb, strength);
}
```

## The function library

Include a library file with `#include <neptune/name.glsl>`.

**`lighting`**: lights and lighting models.

| Function | Returns |
|---|---|
| `nb_lightCount()` | number of lights affecting the object (at most 16) |
| `nb_light(i, position)` | light `i` as an `NbLight`: `color` (linear), `L` (direction towards the light), `atten` (0-1, distance and cone) |
| `nb_lambertTerm(N, L)` | matte lighting, 0-1 |
| `nb_phongTerm(N, L, V, shininess)` | Phong highlight |
| `nb_blinnTerm(N, L, V, shininess)` | Blinn-Phong highlight |
| `nb_toonTerm(ndl, steps)` | `ndl` (0-1) cut into bands |
| `nb_pbrTerm(N, V, albedo, metallic, roughness, light)` | Cook-Torrance (GGX) lighting from one light |
| `nb_shade(surface)` | the whole surface lit with the `//! lighting` model, linear |
| `nb_applyFog(colour)` | the camera fog applied to a linear colour |

`N` is the normal, `L` the direction to the light, `V` the direction to
the camera (`nb_viewDir()`).

**`noise`**: random-looking but smooth patterns. All return 0-1.

| Function | Returns |
|---|---|
| `nb_hash12(p)`, `nb_hash22(p)`, `nb_hash33(p)` | random numbers from a position (1, 2 or 3 of them) |
| `nb_valueNoise(p)` | value noise |
| `nb_perlin(p)`, `nb_perlin3(p)` | Perlin noise in 2D and 3D |
| `nb_simplex(p)` | simplex noise |
| `nb_voronoi(p)` | Voronoi cells: x distance to the nearest cell point, y to the second nearest, z a random id per cell |
| `nb_noise(type, p)` | noise by type: 0 value, 1 Perlin, 2 simplex, 3 Voronoi, 4 cellular edges |
| `nb_fbm(p, octaves)` | Perlin noise in layers (fractal Brownian motion) |
| `nb_fbmEx(type, p, octaves, persistence, lacunarity)` | the same with every option |
| `NB_HEIGHT_NORMAL(fn, uv, eps, strength)` | a normal from a height function, for procedural bumps |

**`pattern`**: repeating patterns and gradients, 0-1.

| Function | Returns |
|---|---|
| `nb_rotate(p, degrees)` | p turned around the origin |
| `nb_checker(p)` | 0 or 1 in a checkerboard of unit squares |
| `nb_grid(p, width)` | 1 on the lines of a unit grid (fragment stage only) |
| `nb_stripes(x, width)` | 1 for the first `width` (0-1) of each unit |
| `nb_dots(p, radius)` | 1 inside a dot in each unit square |
| `nb_linearGradient(uv, degrees)` | 0 → 1 across uv in a direction |
| `nb_radialGradient(uv)` | 0 at the centre to 1 at the edge |
| `nb_angularGradient(uv, degrees)` | 0 → 1 around the centre |

**`color`**: colour tools.

| Function | Returns |
|---|---|
| `nb_hsv2rgb(hsv)`, `nb_rgb2hsv(rgb)` | convert between RGB and hue, saturation, value (all 0-1) |
| `nb_posterize(c, levels)` | the colour cut down to `levels` steps |
| `nb_luminance(c)` | brightness |
| `nb_saturate(c, amount)` | 0 grey, 1 unchanged, above 1 more colourful |

**`procedural`** includes `noise`, `pattern` and `color` together.
**`attributes`** declares the [vertex attributes](#vertex-attributes),
and **`skinning`** gives `nb_skinMatrix()`, for your own vertex `main`.

A rainbow that cycles:

```glsl
// rainbow.glsl
#include <neptune/color.glsl>
//! fragment
void nb_surface(inout NbSurface s) {
    float hue = fract(nb_uv.x + nb_time * 0.1);
    s.albedo = nb_hsv2rgb(vec3(hue, 0.8, 1.0));
}
```

## Including files

```glsl
#include <neptune/noise.glsl>     // the built-in library
#include "common/wind.glsl"       // your own file
```

- Your own files are found relative to the file that includes them.
- Each file is included once, however many times it is named, so library
  files can include each other freely.
- With `ShaderHotReload`, saving an included file recompiles every
  shader that includes it.
- Errors in included files give the included file's name and line.

## Keywords

`ShaderSetKeyword shader, "NAME", True` defines `NAME` at the top of the
shader (as `#define NAME 1`) and recompiles it; `False` removes it. Test
keywords with `#ifdef NAME` / `#ifndef NAME` / `#else` / `#endif`, and with
`if NAME` at the end of a directive.

Each combination of keywords is compiled once and remembered: switching
back to a combination already used is instant. The PBR material uses the
keywords `NB_TRANSPARENT` and `NB_CUTOUT` for its opacity modes.

## Lighting

In the forward pass, `nb_surface` shaders are lit with the `//! lighting`
model:

| Model | Formula |
|---|---|
| `pbr` | Cook-Torrance with GGX distribution, energy conserving; uses `metallic`, `roughness`, `ao` |
| `lambert` | diffuse only: colour × light × cos(angle) |
| `phong` | diffuse plus a Phong highlight (`specularColor`, `shininess`) |
| `blinn` | diffuse plus a Blinn-Phong highlight |
| `toon` | light cut into `toonSteps` bands between `shadeColor` and full colour, with a hard-edged highlight |
| `unlit` | `albedo` as it is, plus `emission` |

All of them add the ambient light (`AmbientLight`), `emission` and the
camera's fog. Directional, point and spot lights are all supported, with
their range and cone.

## Fyrox built-ins

Fyrox's own uniform blocks can be read directly, without declaring them:

| Block | Members you may need |
|---|---|
| `fyrox_instanceData` | `worldMatrix`, `worldViewProjection`, `useSkeletalAnimation` |
| `fyrox_cameraData` | `viewProjectionMatrix`, `position`, `zNear`, `zFar` |
| `fyrox_lightsBlock` | the visible lights (use `nb_light` instead) |
| `fyrox_boneMatrices` | bone matrices (use `nb_skinMatrix` instead) |
| `fyrox_sceneDepth` | a texture: the depth of the solid scene, in the forward pass |

Fyrox's GLSL helper functions (`S_SRGBToLinear`, `S_LinearToSRGB`,
`S_PBR_CalculateLight`, ...) are available as well.

## Coordinates in shaders

Programs work in a left-handed space (+X to the right). Fyrox draws with X
mirrored. Inside shaders, **world positions, normals and directions are
in Fyrox space**: their X is the negative of what your program sees.
This applies to `nb_worldPos`, `nb_normal`, `fyrox_cameraData.position`,
light directions, and the values `ShaderSetCamera` and `ShaderSetLight`
give.

In practice:

- Effects that work in object space (`nb_displace`) or texture space
  (`nb_uv`) need no care.
- A world-space direction you pass from BASIC (a wind direction, say)
  should have its X negated, or the effect will move the mirror way. The
  built-in wind and water shaders do this for you.

[Contents](../SHADERS.md) · Previous: [4. Writing shaders](04-writing-shaders.md) · Next: [6. Post-processing](06-post-processing.md)
