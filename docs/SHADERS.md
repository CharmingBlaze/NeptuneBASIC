# Shaders, materials and post-processing

NeptuneBASIC lets programs write, load, compile, change and apply GPU
shaders with BASIC commands, and comes with a library of ready-made
materials (PBR, toon, water, procedural, ...) and screen effects (bloom,
CRT, depth of field, ...) that need no shader code at all.

Everything here is an extension (ext): Blitz3D had no shaders.

- [Quick start](#quick-start)
- [What the renderer supports](#what-the-renderer-supports)
- [Ready-made materials](#ready-made-materials)
- [Writing a shader](#writing-a-shader)
- [Surface shaders: `nb_surface`](#surface-shaders-nb_surface)
- [Vertex shaders](#vertex-shaders)
- [Parameters](#parameters)
- [Built-in values, attributes and functions](#built-in-values-attributes-and-functions)
- [Directives](#directives)
- [Keywords and variants](#keywords-and-variants)
- [Lighting and shadows](#lighting-and-shadows)
- [WGSL shaders](#wgsl-shaders)
- [Fyrox shader files](#fyrox-shader-files)
- [Post-processing](#post-processing)
- [Hot reloading](#hot-reloading)
- [Errors and debugging](#errors-and-debugging)
- [Performance](#performance)
- [Coordinates](#coordinates)
- [Limits](#limits)
- [Examples](#examples)
- [How it works](#how-it-works)

## Quick start

A material without writing any shader code:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 1, -4
light = CreateLight()
TurnEntity light, 45, 30, 0

ball = CreateSphere(32)
gold = CreatePBRMaterial()
SetAlbedo gold, 255, 200, 90
SetMetallic gold, 1
SetRoughness gold, 0.25
EntityMaterial ball, gold

While Not KeyHit(KEY_ESCAPE)
    TurnEntity ball, 0, 1, 0
    RenderWorld
    Flip
Wend
```

A shader of your own, in a few lines of GLSL:

```basic
src$ = "uniform vec4 tint = vec4(1.0, 0.5, 0.2, 1.0);" + Chr(10)
src$ = src$ + "//! fragment" + Chr(10)
src$ = src$ + "void nb_surface(inout NbSurface s) {" + Chr(10)
src$ = src$ + "    s.albedo = tint.rgb * (0.5 + 0.5 * sin(nb_time * 3.0));" + Chr(10)
src$ = src$ + "}"
pulse = CreateShader(src$)
ShaderApply pulse, ball
ShaderSetColor pulse, "tint", 80, 160, 255     ; change it any time
```

Shaders usually live in files; `LoadShader("shaders/pulse.glsl")` loads
one, and `ShaderHotReload` recompiles it every time you save the file.

A screen effect:

```basic
crt = LoadPostProcess("builtin:crt")
AddPostProcess crt, camera
```

## What the renderer supports

NeptuneBASIC renders with Fyrox 1.0. These facts shape the shader system:

| | |
|---|---|
| Graphics API | OpenGL 3.3 core (OpenGL ES 3.0 where that is all there is), on Windows, Linux and macOS |
| Shader language the GPU runs | GLSL 3.30 |
| Shader languages you can write | GLSL (NeptuneBASIC's form, below), WGSL (checked and translated by [naga](https://github.com/gfx-rs/wgpu/tree/trunk/naga)), Fyrox `.shader` files |
| Stages | vertex and fragment |
| Geometry stage | none: `ShaderGeometrySource` fails with an explanation. Build geometry with `CreateMesh`/`AddVertex`, or move vertices in a vertex shader |
| Compute stage | none: `ShaderComputeSource` and `@compute` fail with an explanation. Compute into a texture with a post-processing effect and `PostProcessOutput`, or on the CPU (`BakeProceduralTexture` does this for procedural materials) |
| Lighting | deferred PBR (G-buffer) with shadows, plus a forward pass for blending and custom lighting |
| Shadows | directional (3 cascades), spot and point (cube) shadow maps, hard or PCF-filtered |
| Post-processing | HDR with tone mapping, bloom, SSAO, FXAA and colour grading built in; any number of extra screen passes from NeptuneBASIC |
| Texture arrays | no array sampler in Fyrox: `ShaderSetTextureArray` stacks layers into a 3D texture |

`ShaderGetBackend()` returns this in one line.

## Ready-made materials

These need no shader code. Each `Create...` command returns a material (a
shader object with its own parameters); draw an entity with it using
`EntityMaterial entity, material`. Materials made from the same built-in
share one compiled shader, so a thousand `CreatePBRMaterial`s still compile
once.

### PBR

```basic
brick = CreatePBRMaterial()
SetAlbedoMap brick, LoadTexture("brick.png")
SetNormalMap brick, LoadTexture("brick_n.png"), 1.0
SetRoughnessMap brick, LoadTexture("brick_r.png")
SetAOMap brick, LoadTexture("brick_ao.png")
SetHeightMap brick, LoadTexture("brick_h.png"), 0.04   ; parallax depth
EntityMaterial wall, brick
```

| Command | Effect |
|---|---|
| `SetAlbedo m, r, g, b [, a]` / `SetAlbedoMap m, tex` | base colour |
| `SetMetallic m, 0..1` / `SetMetallicMap` | metal or not |
| `SetRoughness m, 0..1` / `SetRoughnessMap` | mirror to matte |
| `SetNormalMap m, tex [, strength]` | bumps |
| `SetAOMap m, tex` | ambient occlusion |
| `SetEmissive m, r, g, b [, strength]` / `SetEmissiveMap` | glow (blooms) |
| `SetOpacity m, 0..1 [, mode]` / `SetOpacityMap m, tex [, mode]` | `OPACITY_CUTOUT` (holes, keeps shadows) or `OPACITY_BLEND` |
| `SetHeightMap m, tex [, scale]` / `SetParallaxScale` | parallax mapping |
| `SetEnvironmentMap m, cube [, intensity]` / `SetReflectionIntensity` / `SetFresnel` | reflections |
| `SetMaterialWorkflow m, WORKFLOW_SPECULAR, r, g, b` | specular workflow |

PBR materials draw in Fyrox's deferred pass: Cook-Torrance (GGX)
lighting from every light, image-based lighting from the sky, and real
shadows cast and received. Blended opacity moves them to the forward pass.

### Toon and stylized

```basic
hero = CreateToonMaterial(SHADING_TOON)
SetAlbedo hero, 255, 140, 60
SetToonSteps hero, 3
SetToonOutline hero, True, 0.04
SetOutlineColor hero, 20, 20, 30
SetRimLighting hero, True
EntityMaterial player, hero
```

`SetShadingMode m, mode` switches a material between looks and keeps the
settings both looks have:

| Mode | Look |
|---|---|
| `SHADING_TOON` | banded light, highlight, rim |
| `SHADING_CEL` | two-tone, no highlight |
| `SHADING_FLAT` | one shade per triangle (low poly) |
| `SHADING_UNLIT` | colour and texture only |
| `SHADING_VERTEXLIT` | light worked out per vertex (early 3D hardware) |
| `SHADING_PIXEL` | crisp texels and banded light (pixel art) |
| `SHADING_PS1` | wobbly snapped vertices, affine textures, few colours |
| `SHADING_MATCAP` | lit-sphere texture (`ShaderSetTexture m, "matcap", tex`) |
| `SHADING_POSTERIZE` | few colour levels |
| `SHADING_HANDPAINTED` | soft warm/cool shading with brush noise |
| `SHADING_LAMBERT`, `SHADING_PHONG`, `SHADING_PBR`, `SHADING_RIM` | classic lighting models |

`SetToonRamp m, tex` lights through a ramp image instead of steps (light
level picks the colour across the image). Outlines are inverted hulls: a
copy of the mesh, pushed out along its normals and drawn inside out.
`SetVertexLighting`, `SetFlatShading` and `SetPixelShading` switch to and
from those looks.

### Water and foliage

```basic
sea = CreateWaterMaterial()
SetWaterWaves sea, 0.3, 8, 1, 0.4     ; height, length, direction x, z
SetWaterSpeed sea, 1.2
SetWaterDepth sea, 5                 ; units to reach the deep colour
SetWaterFoam sea, 0.6
SetWaterReflection sea, 0.7, skycube
EntityMaterial watermesh, sea

grass = CreateFoliageMaterial()
SetWindDirection 1, 0.3
SetWindStrength 0.3
EntityMaterial bush, grass
```

Waves move vertices, so the water mesh needs vertices to move (a grid, not
a single quad). Depth colour and foam read the scene's depth buffer; they
show where the water meets the ground. Wind materials follow the global
wind unless `SetFoliageWind` gives them their own.

### Procedural materials

```basic
marble = CreateProceduralShader(1)         ; 0 gradient, 1 noise, 2 pattern
SetNoiseType marble, NOISE_PERLIN
SetNoiseScale marble, 3
SetNoiseOctaves marble, 6
SetDistortion marble, 0.4
EntityMaterial statue, marble

tiles = CreateProceduralShader(2)
SetPattern tiles, PATTERN_CHECKER
SetPatternScale tiles, 8
SetPatternColor tiles, 30, 30, 30
```

Gradients (`GRADIENT_LINEAR`, `_RADIAL`, `_ANGULAR`), noise (`NOISE_VALUE`,
`_PERLIN`, `_SIMPLEX`, `_VORONOI`, `_CELLULAR`, mixed in octaves of fractal
Brownian motion) and patterns (`PATTERN_CHECKER`, `_GRID`, `_STRIPES`,
`_DOTS`, `_CELLS`, `_NOISE`) combine in one material. Nothing is stored in
textures; the GPU works them out per pixel, and `SetNoiseSpeed` animates
them.

`BakeProceduralTexture(m, width, height [, normal_map])` evaluates the
material on the CPU into an ordinary texture, so a procedural pattern can
be the albedo, roughness or normal map of any other material:

```basic
bumps = CreateProceduralShader(1)
SetNoiseScale bumps, 8
SetNormalMap rock, BakeProceduralTexture(bumps, 256, 256, True)
```

### Built-in shaders

Every ready-made material is also a built-in shader you can load and
change with the `ShaderSet*` commands: `LoadShader("builtin:<name>")`.

| Name | What it is | Main parameters |
|---|---|---|
| `unlit` | colour × texture, no lighting | `albedo`, `albedoMap`, `uvScale` |
| `textured` | textured, PBR-lit (forward) | `albedo`, `albedoMap`, `roughness` |
| `vertexcolor` | vertex colours × texture, Lambert | `albedo`, `albedoMap` |
| `diffuse` | Lambert | `albedo`, `albedoMap` |
| `phong`, `blinnphong` | classic highlights | `albedo`, `specularColor`, `shininess` |
| `pbr`, `normalmap`, `metal` | deferred PBR (see above) | see `CreatePBRMaterial` |
| `glass` | blended, reflective | `albedo`, `envMap`, `reflection`, `fresnel` |
| `water` | see `CreateWaterMaterial` | `waveHeight`, `waveLength`, `shallowColor`, ... |
| `toon`, `cel` | see `CreateToonMaterial` | `toonSteps`, `shadeColor`, `rimColor`, ... |
| `outline` | inverted hull | `outlineColor`, `outlineWidth` |
| `rim` | dark with glowing edges | `rimColor`, `rimPower`, `rimStrength` |
| `hologram` | additive scan lines and flicker | `color`, `lineDensity`, `speed` |
| `dissolve` | burns away along noise | `amount` (0-1), `edgeColor`, `edgeWidth` |
| `fire`, `smoke` | animated billboards (use on sprites) | `innerColor`, `speed`, `density` |
| `forcefield` | cells, rim and glow where it touches geometry | `color`, `hexScale`, `pulse` |
| `gradient`, `noise`, `procedural` | see `CreateProceduralShader` | |
| `matcap`, `flat`, `ps1`, `pixelart`, `vertexlit`, `posterize`, `handpainted` | stylized looks | |
| `wind` | swaying foliage | `windDirection`, `windStrength`, `windSpeed` |
| `displace` | vertex displacement by height map and waves | `heightMap`, `amount`, `frequency` |
| `terrain` | per-pixel grass, rock, sand and snow for terrains (deferred: lit with shadows) | see below |

`builtin:terrain` draws a terrain made with `CreateTerrain` (any mesh
works). It chooses rock by slope, sand near `seaLevel` and snow above
`snowHeight`, or with `painted` set to 1 it uses the weights painted with
`PaintTerrain` / `ModifyTerrainLayer` (layer 1 rock, 2 sand, 3 snow). Grass
gets dry patches, rock gets layers and cracks, sand gets ripples and turns
darker and glossy just above `seaLevel`, and small detail fades out with
distance. Colours (sRGB 0-1): `grassColor`, `dryGrassColor`, `rockColor`,
`rockColor2`, `sandColor`, `snowColor`; also `sandHeight`, `rockSlope`
(start and full, as 1 - normal height), `detail`, `bumpStrength`, and
optional textures `grassMap`, `rockMap` (mapped from three sides),
`sandMap`, `snowMap` with `mapScale`. Camera fog (`CameraFogMode`) is
applied; post-process fog works too and keeps PBR lighting on everything
else.

```basic
land = LoadShader("builtin:terrain")
EntityShader ground, land
ShaderFloat ground, "seaLevel", 48
ShaderFloat ground, "snowHeight", 130
```

`ShaderGetSource(shader, 0)` shows the source of any of them: they are
written in exactly the language described below, so they make good
starting points.

## Writing a shader

A NeptuneBASIC shader is GLSL 3.30 with a few additions:

```glsl
// pulse.glsl
//! lighting blinn                 // directives start with //!
//! shadows on
#include <neptune/noise.glsl>      // built-in library (or "my_file.glsl")

uniform vec4 tint = vec4(1.0, 0.5, 0.2, 1.0);   // parameters, with defaults
uniform float speed = 2.0;
uniform sampler2D pattern;

//! fragment                       // the fragment stage starts here
void nb_surface(inout NbSurface s) {
    vec3 p = texture(pattern, nb_uv).rgb;
    s.albedo = tint.rgb * p * (0.7 + 0.3 * sin(nb_time * speed));
    s.shininess = 64.0;
}
```

- Text before any stage marker is **common**: it goes into both stages
  (uniforms, includes, shared functions).
- `//! vertex` and `//! fragment` start the stages. Leave the vertex stage
  out and the standard one is used (it handles skinned meshes too).
- A fragment stage either has a `main()` that writes `FragColor` (it is
  declared for you), or a `nb_surface` function (next section).

The same file works anywhere: `LoadShader("pulse.glsl")`,
`CreateShader(text$)`, or the stages one by one:

```basic
s = CreateShader()
ShaderSource s, "uniform float speed = 2.0;"
ShaderFragmentSource s, "void main() { FragColor = vec4(vec3(fract(nb_time * speed)), 1.0); }"
If CompileShader(s) = 0 Then RuntimeError ShaderError(s)
```

A fragment `main` writes linear (not sRGB) colour: Fyrox renders in HDR and
tone maps afterwards. `nb_linear(c)` converts an sRGB colour.

## Surface shaders: `nb_surface`

Most shaders only need to describe a surface; NeptuneBASIC writes the rest
for the chosen pass and lighting model. Define

```glsl
void nb_surface(inout NbSurface s) { ... }
```

and change any of these fields (the defaults are shown):

| Field | Default | Meaning |
|---|---|---|
| `vec3 albedo` | `1, 1, 1` | base colour, sRGB 0-1 (like textures and program colours) |
| `float alpha` | `1` | opacity (needs `//! blend alpha` or `//! alpha_clip on`) |
| `vec3 normal` | interpolated normal | world-space normal (`nb_normalFromMap` for normal maps) |
| `float metallic` | `0` | PBR |
| `float roughness` | `0.6` | PBR |
| `float ao` | `1` | ambient occlusion |
| `vec3 emission` | `0` | linear light added on top; above 1 glows with bloom |
| `float alphaCutoff` | `0.5` | for `alpha_clip` |
| `vec3 specularColor` | `0.5` | Phong, Blinn, toon highlights |
| `float shininess` | `32` | highlight size |
| `float toonSteps` | `3` | toon bands |
| `vec3 shadeColor` | `0.45, 0.45, 0.6` | toon: colour of the unlit side |

In a surface shader you can read the interpolated vertex values `nb_uv`,
`nb_uv2`, `nb_worldPos`, `nb_normal`, `nb_tangent`, `nb_binormal`,
`nb_color` (vertex colour) and `nb_data0`-`nb_data3` (`VertexData`).

The same `nb_surface` works in every pass: deferred (`//! pass gbuffer`,
written to Fyrox's G-buffer, lit by Fyrox with shadows), forward with any
lighting model, and even shadow passes (cut-out alpha casts cut-out
shadows).

## Vertex shaders

To move vertices but keep everything else standard, define a hook in the
vertex stage instead of a `main`:

```glsl
//! vertex
// local space, before the object's transform
void nb_displace(inout vec3 position, inout vec3 normal, vec2 uv) {
    position += normal * sin(position.y * 4.0 + nb_time * 3.0) * 0.1;
}
```

or, for effects that must line up between objects (waves, wind), in world
space:

```glsl
//! vertex
void nb_displaceWorld(inout vec3 position, inout vec3 normal, vec2 uv) {
    position.y += sin(position.x * 0.5 + nb_time) * 0.3;
}
```

Shadow passes use the same hooks, so displaced objects cast displaced
shadows. The standard vertex program applies skeletal animation first, so
hooks work on animated characters too.

For full control, write the vertex `main` yourself:

```glsl
//! vertex
#include <neptune/attributes.glsl>     // vertexPosition, vertexNormal, ...
#include <neptune/skinning.glsl>       // nb_skinMatrix()
out vec2 uv;
void main() {
    vec4 p = nb_skinMatrix() * vec4(vertexPosition, 1.0);
    uv = vertexTexCoord;
    gl_Position = fyrox_instanceData.worldViewProjection * p;
}
//! fragment
in vec2 uv;
void main() { FragColor = vec4(uv, 0.0, 1.0); }
```

Instanced rendering needs nothing special: Fyrox fills
`fyrox_instanceData` per instance.

## Parameters

Each `uniform` line becomes a material parameter:

```glsl
uniform float speed = 2.0;                 // float, int, uint, bool
uniform vec4 tint = vec4(1, 0.5, 0.2, 1);  // vec2, vec3, vec4
uniform mat4 projector;                    // mat3, mat4 (identity by default)
uniform float weights[16];                 // arrays of float, int, vec2-4, mat4
uniform sampler2D detail;                  // textures: sampler2D, sampler3D,
uniform samplerCube sky;                   //   samplerCube, sampler2DArray
uniform sampler2D bumpNormal; //! fallback normal
```

Use them by name in the code; NeptuneBASIC puts the values in a uniform
block for you. An unbound texture reads white (black or a flat normal with
`//! fallback`; names containing "normal" fall back to a flat normal).

Set them from BASIC:

```basic
ShaderSetFloat s, "speed", 3.5
ShaderSetVector3 s, "offset", 0, 1, 0
ShaderSetColor s, "tint", 255, 128, 0           ; 0-255 -> 0-1
ShaderSetMatrix4 s, "projector", "1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1"
ShaderSetBuffer s, "weights", "0.1, 0.2, 0.4, 0.2, 0.1"
ShaderSetTexture s, "detail", tex
ShaderSetCubeMap s, "sky", cubetex
ShaderSetTextureArray s, "layers", Str(t1) + "," + Str(t2) + "," + Str(t3)
ShaderSetParameter s, "tint", "1, 0.5, 0, 1"    ; any type, from text
v$ = ShaderGetParameter(s, "speed")
```

Parameters are checked against the declaration: setting a texture on a
float, or a name the shader does not have, is an error that says so.

Every entity using a shader shares its parameters. For one entity only,
use `ShaderFloat entity, name$, value`, `ShaderVector` or `ShaderTexture`;
or make a copy with `ShaderClone`.

`ShaderSetCamera s, camera` and `ShaderSetLight s, light [, index]` fill
the parameters `nb_cameraPosition`, `nb_viewMatrix`,
`nb_projectionMatrix`, `nb_viewProjection` and `nb_lightPosition`,
`nb_lightDirection`, `nb_lightColor`, `nb_lightRange` (declared as arrays,
`index` picks the element). `ShaderSetMaterial s, other` copies all
parameters two shaders share.

Reflection: `ShaderParameterCount`, `ShaderParameterName(s, i)`,
`ShaderParameterType(s, name$)` (`"float"`, `"vec3"`, `"float[16]"`,
`"sampler2D"`, ...), `ShaderParameterExists(s, name$)`.

## Built-in values, attributes and functions

Values the runtime fills in when a shader uses them:

| Name | Type | Value |
|---|---|---|
| `nb_time` | float | seconds since the program started (`ShaderSetTime` fixes it) |
| `nb_deltaTime` | float | length of the last frame |
| `nb_resolution` | vec2 | window size in pixels (`ShaderSetResolution` fixes it) |
| `nb_ambient` | vec3 | `AmbientLight` colour, sRGB 0-1 |
| `nb_fogColor`, `nb_fogRange` | vec3, vec2 | camera fog (range 0, 0 = no fog) |

Fyrox's built-in blocks are available without declaring them:
`fyrox_instanceData` (`worldMatrix`, `worldViewProjection`,
`useSkeletalAnimation`), `fyrox_cameraData` (`viewProjectionMatrix`,
`position`, `zNear`, `zFar`, ...), `fyrox_lightsBlock` (up to 16 visible
lights), `fyrox_lightData` (shadow passes), `fyrox_boneMatrices`, and the
texture `fyrox_sceneDepth` (scene depth, in the forward pass). Fyrox's
helper functions are there too (`S_SRGBToLinear`, `S_PBR_CalculateLight`,
...).

Vertex attributes (standard program, or `#include <neptune/attributes.glsl>`):

| Name | Location | |
|---|---|---|
| `vertexPosition` | 0 | vec3 |
| `vertexTexCoord` | 1 | vec2, first UV set |
| `vertexNormal` | 2 | vec3 |
| `vertexTangent` | 3 | vec4 (w = handedness) |
| `boneWeights`, `boneIndices` | 4, 5 | skinned meshes |
| `vertexSecondTexCoord` | 6 | vec2, second UV set |
| `vertexColor` | 7 | vec4 0-1 |
| `vertexData0`-`3` | 8-11 | `VertexData` values |

Helpers always available in fragment stages: `nb_linear`, `nb_srgb`,
`nb_viewDir()`, `nb_fresnel(normal, power)`, `nb_normalFromMap(sample,
strength)`, `nb_screenUV()`, `nb_textureArray(tex, uv, layer)`.

The library (`#include <neptune/name.glsl>`):

| File | Functions |
|---|---|
| `lighting` | `nb_lightCount()`, `nb_light(i, worldPos)` (colour, direction, attenuation), `nb_lambertTerm`, `nb_phongTerm`, `nb_blinnTerm`, `nb_toonTerm`, `nb_pbrTerm`, `nb_shade(surface)`, `nb_applyFog` |
| `noise` | `nb_hash12/22/33`, `nb_valueNoise`, `nb_perlin`, `nb_perlin3`, `nb_simplex`, `nb_voronoi`, `nb_noise(type, p)`, `nb_fbm`, `nb_fbmEx(type, p, octaves, persistence, lacunarity)`, `NB_HEIGHT_NORMAL` |
| `pattern` | `nb_rotate`, `nb_checker`, `nb_grid`, `nb_stripes`, `nb_dots`, `nb_linearGradient`, `nb_radialGradient`, `nb_angularGradient` |
| `color` | `nb_hsv2rgb`, `nb_rgb2hsv`, `nb_posterize`, `nb_luminance`, `nb_saturate` |
| `procedural` | noise + pattern + color |
| `attributes` | the vertex attribute declarations |
| `skinning` | `nb_skinMatrix()` |

`#include "file.glsl"` includes your own files, relative to the including
file; each file is included once.

## Directives

| Directive | Values | Default |
|---|---|---|
| `//! pass` | `forward`, `gbuffer`, `post` | `forward` |
| `//! lighting` | `pbr`, `lambert`, `phong`, `blinn`, `toon`, `unlit` | `pbr` |
| `//! blend` | `none`, `alpha`, `add`, `multiply`, `premultiplied` | `none` |
| `//! cull` | `back`, `front`, `none` | `back` |
| `//! depth_write` | `on`, `off` | on unless blending |
| `//! depth_test` | `on`, `off` | `on` |
| `//! shadows` | `on`, `off`: cast shadows | `off` |
| `//! alpha_clip` | `on`, `off`: discard below `s.alphaCutoff` | `off` |
| `//! default name values` | default of a parameter (for WGSL) | |
| `//! fallback texture white/black/normal` | texture read when none is set | white |
| `//! include "file"` | same as `#include` | |
| `//! vertex`, `//! fragment`, `//! common` | start a stage | |

- **forward**: drawn after the opaque scene, lit by the lighting model
  from the 16 most important visible lights; can blend; reads
  `fyrox_sceneDepth`. Does not receive shadow maps.
- **gbuffer**: writes Fyrox's G-buffer, so Fyrox lights it (PBR, every
  light, image-based lighting) and it receives shadows. No blending
  (blending moves it to forward, with a warning in `ShaderLog`).
- **post**: a full-screen effect (see Post-processing).

A directive can end with `if KEYWORD` or `if !KEYWORD` (next section).

## Keywords and variants

`ShaderSetKeyword s, "NAME", True` adds `#define NAME 1` and recompiles.
Each combination of keywords is compiled once and cached, so switching
back and forth costs nothing after the first time.

```glsl
//! blend alpha if GHOST
//! fragment
void nb_surface(inout NbSurface s) {
#ifdef GHOST
    s.alpha = 0.4;
#endif
}
```

The PBR material uses `NB_TRANSPARENT` and `NB_CUTOUT` this way.

## Lighting and shadows

Forward shaders get Fyrox's visible lights through `nb_light(i, P)`:

```glsl
#include <neptune/lighting.glsl>
//! fragment
void main() {
    vec3 N = normalize(nb_normal);
    vec3 c = vec3(0.0);
    for (int i = 0; i < nb_lightCount(); ++i) {
        NbLight l = nb_light(i, nb_worldPos);   // l.L points to the light
        c += l.color * l.atten * smoothstep(0.0, 0.05, dot(N, l.L));
    }
    FragColor = vec4(c, 1.0);
}
```

Directional, point and spot lights are all handled (`l.atten` includes
distance and cone falloff). `//! lighting` picks a ready model for
`nb_surface` shaders: Lambert, Phong, Blinn-Phong, toon (bands + highlight
+ shade colour), PBR (Cook-Torrance GGX, energy conserving) or unlit.

Shadows:

- `//! shadows on` makes a shader cast shadows (it gets directional, spot
  and point shadow passes, using its vertex hooks).
- Receiving shadows needs the deferred pass (`//! pass gbuffer`; the PBR
  material uses it).

Shadow settings, for the whole renderer:

```basic
SetShadowMode SHADOW_SOFT          ; SHADOW_OFF, SHADOW_HARD, SHADOW_SOFT (PCF)
SetShadowQuality 3                 ; 0-3: map size 512-4096, precision
SetShadowResolution 2048
SetShadowDistance 80               ; how far shadows reach
SetShadowCascadeCount 3            ; 1-3 cascades for directional lights
SetShadowBias 0.0005               ; fixes "shadow acne"
EnableShadows : DisableShadows
LightShadows light, False          ; per light
```

## WGSL shaders

WGSL files (`.wgsl`, or `CreateShader(src$, "wgsl")`) are parsed and
validated by naga and translated to GLSL. Conventions:

```wgsl
//! cull none
struct Params { tint: vec4<f32>, nb_time: f32 };
struct Instance { worldMatrix: mat4x4<f32>, worldViewProjection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> properties: Params;          // parameters
@group(0) @binding(1) var<uniform> fyrox_instanceData: Instance; // Fyrox built-in
@group(1) @binding(0) var albedo: texture_2d<f32>;
@group(1) @binding(1) var albedoSampler: sampler;
//! default tint 1 0.5 0.2 1

struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex fn vs(@location(0) p: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var o: VOut;
    o.pos = fyrox_instanceData.worldViewProjection * vec4<f32>(p, 1.0);
    o.uv = uv;
    return o;
}

@fragment fn fs(i: VOut) -> @location(0) vec4<f32> {
    return textureSample(albedo, albedoSampler, i.uv) * properties.tint;
}
```

- One `@vertex` and one `@fragment` entry point.
- Each `var<uniform>` struct is a block of parameters; its members are the
  parameter names (`ShaderSetColor s, "tint", ...`). Members named
  `nb_time`, `nb_resolution`, ... are filled in by the runtime.
- Fyrox built-ins are uniform structs with Fyrox's names and member names;
  read their members directly.
- Vertex inputs use the mesh locations in the table above.
- `#ifdef KEYWORD` / `#ifndef` / `#else` / `#endif` lines select code by
  keyword; `//!` directives work as in GLSL.
- Clip space is OpenGL's (the matrices come from Fyrox).
- Errors point at WGSL lines (`ripple.wgsl:12: no definition in scope
  for identifier: 'x'`).

## Fyrox shader files

A `.shader` file is Fyrox's own format (GLSL inside a RON description of
resources and passes; see `examples/media/wave.shader`). It is used as it
is, with its properties as parameters. `ShaderSave s, "out.shader"` writes
the Fyrox shader any NeptuneBASIC shader turns into, and
`ShaderGetSource(s, 3)` returns it.

## Post-processing

A post-processing effect is a full-screen pass over the finished image.
Effects form a chain per camera:

```basic
bloom = LoadPostProcess("builtin:bloom")
SetPostProcessParameter bloom, "threshold", "0.6"
grain = LoadPostProcess("builtin:grain")
AddPostProcess bloom, camera       ; camera 0 = every camera
AddPostProcess grain, camera
SetPostProcessOrder grain, 10      ; lower orders run first
SetPostProcessEnabled bloom, False ; switch off without removing
```

Built-in effects and their parameters (`SetPostProcessParameter`):

| Effect | Parameters |
|---|---|
| `bloom` | `threshold`, `intensity`, `radius` |
| `exposure` (HDR) | `exposure` (stops) |
| `tonemap` (HDR) | `mode` (0 ACES, 1 Reinhard, 2 Uncharted 2, 3 none), `exposure` |
| `vignette` | `intensity`, `smoothness`, `color` |
| `chromatic` | `amount` |
| `grain` | `amount`, `size` |
| `colorgrade` | `lift`, `gammaRGB`, `gain`, `temperature`, `tint`, `contrast`, `saturation`, `brightness` |
| `gamma`, `contrast`, `saturation`, `brightness` | one value each |
| `fxaa` | `subpix`, `edgeThreshold` |
| `outline` | `thickness`, `color`, `depthThreshold`, `normalThreshold` |
| `pixelate` | `pixelSize` |
| `crt` | `curvature`, `scanlines`, `mask`, `vignette` |
| `distortion` | `amount`, `scale`, `speed` |
| `heathaze` | `amount`, `speed`, `scale`, `startDistance` |
| `blur`, `gaussian` | `radius` |
| `radialblur` | `amount`, `center` |
| `sharpen` | `amount` |
| `dof` | `focusDistance`, `focusRange`, `maxBlur` |
| `motionblur` | `amount` (camera motion) |
| `ssr` | `intensity`, `maxDistance`, `thickness` |
| `ssao` | `radius`, `intensity` |
| `fog` | `density`, `color`, `heightFalloff`, `start`, `skyFog` (0 leaves the sky alone), `skyHorizon` (with `skyFog` 0: how high above the horizon the sky still fades into the fog) |
| `underwater` | `waterLevel`, `color`, `density`, `distortion` |
| `rain`, `snow` | `amount`, `speed` (`size` for snow) |
| `invert` | |

Fyrox's own HDR, tone mapping, bloom, SSAO, FXAA and colour grading are
switched with `PostEffect`, `GraphicsQuality`, `CameraExposure` and
`CameraColorGrading`. `SetUnderwaterEffect`, `SetRainEffect`,
`SetSnowEffect` and `SetAtmosphericFog` set up the matching effects on a
camera in one call.

### Writing an effect

```glsl
// scanlines.glsl -> LoadPostProcess("scanlines.glsl")
uniform float strength = 0.25;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    c *= 1.0 - strength * (0.5 + 0.5 * sin(uv.y * nb_resolution.y * 1.5));
    return vec4(c, 1.0);
}
```

or with `CreatePostProcess(source$)`. An effect reads:

| Name | |
|---|---|
| `nb_screenTexture` | the image so far |
| `nb_originalTexture` | the scene before any effect |
| `nb_depthTexture` | depth of everything drawn, forward-drawn surfaces (water, blended and classic materials) included |
| `nb_normalTexture` | world normals (G-buffer: deferred surfaces only) |
| `nb_resolution`, `nb_time` | |
| `nb_cameraPos`, `nb_clip` (near, far), `nb_invViewProj`, `nb_prevViewProj` | camera, for depth effects and motion blur |
| `nb_texel()`, `nb_luma(c)`, `nb_linearDepth(uv)`, `nb_worldPosAt(uv)`, `nb_normalAt(uv)` | helpers |

Effects normally work on the final (sRGB) image. `PostProcessPriority e,
-1` runs one on the HDR image before tone mapping instead (linear colour,
values above 1). Other routing:

- `PostProcessInput e, POST_ORIGINAL` reads the untouched scene;
  `PostProcessInput e, other` reads another effect's output.
- `PostProcessOutput e, texture` draws into a texture instead of the
  screen: the result can texture objects, be read by other effects, or be
  used as a GPU-computed texture.
- `PostProcessResolution e, 0.5` draws at half size (cheap glows).
- `PostProcessCamera e, cam` / `SetPostProcess cam, e` limit effects to a
  camera.

## Hot reloading

```basic
s = LoadShader("shaders/water.glsl")
ShaderHotReload s
```

Every quarter of a second the runtime checks the file and everything it
includes. When one changes, the shader is compiled again; entities update
at once. If the new version does not compile, the old one stays on screen
and the error goes to `ShaderError`, `ShaderLog` and the console, once per
change. Hot reloading reads files, so it needs `ShaderHotReload` on a
shader loaded from a file.

## Errors and debugging

Compile errors name the file and line of your source, even inside
includes, along with the pass and stage:

```
water.glsl:42 (Forward fragment): 'nb_foam' : undeclared identifier
```

Errors are found in three places, and all report the same way:

1. NeptuneBASIC's own checks (directives, uniform declarations, includes,
   missing stages): `pulse.glsl:3: blend must be none, alpha, add, ...`;
2. naga, for WGSL;
3. the GPU driver, which compiles every pass when `CompileShader` runs (or
   on the first frame, for shaders compiled before `Graphics3D`).

| Command | Gives |
|---|---|
| `CompileShader(s)`, `ValidateShader(s)` | 1 or 0 |
| `ShaderError$(s)` / `ShaderCompileError$(s)` | the error |
| `ShaderLog$(s)` | every compile, warning and hot reload |
| `ShaderGetCompilationStatus(s)` | `SHADER_COMPILED`, `SHADER_FAILED`, ... |
| `ShaderDebug$(s)` | a full report |
| `ShaderGetUniforms$`, `ShaderGetAttributes$`, `ShaderGetBindings$` | reflection |
| `ShaderGetResourceInfo$(s)` | users, materials, compiles, cache use |
| `ShaderGetSource$(s, 1 / 2 / 3)` | the generated vertex and fragment programs, the Fyrox shader |

`ValidateShader` checks without replacing the shader in use: compile
before you apply. A shader that failed is not used, and its entities draw
with their normal materials (or the last version that worked).

## Performance

- **Compile once.** Compiled shaders are cached by their final text; every
  object with the same source and keywords shares one Fyrox shader, and
  Fyrox builds one GPU pipeline for it.
- **Share materials.** Entities that use a shader without per-entity
  overrides share one material: setting a parameter is one write, and
  Fyrox batches those entities together.
- **No per-frame rebuilds.** Parameters change the existing material in
  place; automatic values (`nb_time`, ...) are written once per frame per
  material, only for shaders that use them.
- **Post effects** reuse their render targets and programs between frames.
- `ShaderGetResourceInfo` shows materials, compiles and cache hits, and
  `TrisRendered()` the triangles drawn.

`examples/shadertests.nb` checks that 200 cubes with one shader material
cost no more than 200 with the standard material.

## Coordinates

Programs work in Blitz space; Fyrox draws with X mirrored. Inside shaders,
world positions, normals and directions (`nb_worldPos`,
`fyrox_cameraData.position`, light directions, `ShaderSetCamera` and
`ShaderSetLight` values) are in **Fyrox space**: X is negated compared with
the program. Directions a program passes to a shader for world-space work
should negate X (the built-in wind and water shaders do this for
`windDirection` and `waveDirection`). Local space (`nb_displace`) and UVs
are the same in both.

## Limits

- Vertex and fragment stages only (no geometry, tessellation or compute;
  see [What the renderer supports](#what-the-renderer-supports) for
  alternatives).
- Forward shaders see the 16 most important visible lights and do not
  sample shadow maps; deferred (`gbuffer`) shaders receive shadows.
- Soft shadows are on or off (Fyrox's PCF), with up to 3 cascades; there
  are no contact shadows.
- Reflection probes are Fyrox's; `ssr` is a screen-space approximation.
- At most 16 textures and 64 parameters per post-processing effect.
- WGSL: one vertex and one fragment entry point, no array textures, no
  storage buffers.

## Examples

| File | Shows |
|---|---|
| `examples/shaders.nb` | every ready-made material and the custom shaders side by side, with keys for bloom, pixelation, CRT, heat haze, a custom post shader, vignette and grain, and outlines. Unlit, textured, diffuse, Phong, Blinn-Phong, PBR, normal mapping, metal, glass, water, toon, cel, outline, hologram, dissolve, fire, smoke, force field, gradient, procedural noise, vertex displacement, custom GLSL with hot reloading, WGSL, wind, PS1, pixel art and flat shading |
| `examples/shadertests.nb` | the acceptance tests (below) |
| `examples/media/shaders/rimtoon.glsl` | a surface shader: toon lighting, noise, rim glow |
| `examples/media/shaders/ripple.wgsl` | a WGSL shader with animated parameters |
| `examples/media/shaders/scanlines_post.glsl` | a custom post-processing effect |
| `examples/media/wave.shader` | a Fyrox `.shader` file with `VertexData` (`examples/rendering.nb`) |

`neptune run --uncapped examples/shadertests.nb` runs 20 checks on the
GPU, compares pixels from screenshots, and prints PASS or FAIL for each:
compiling a vertex and fragment shader, applying it to a cube, changing
parameters, binding several textures, normal maps, PBR, directional and
point lighting, cast and received shadows, a post-processing chain, hot
reloading (and keeping the old shader when the edit breaks), error lines,
procedural gradients and noise, toon banding, transparency, freeing
resources, the backend, WGSL, and frame time. The source processing,
WGSL translation and every built-in shader and effect are also unit
tested (`cargo test -p neptune-runtime`), which runs in CI on Windows,
Linux and macOS.

## Command index

Every command of the shader system, by area. All of them are
extensions. Arguments, defaults and a description of each are in the
[command reference](COMMANDS.md); `LoadShader` is listed there with the
3D commands.

| Area | Commands |
|---|---|
| Shaders | `CreateShader`, `CompileShader`, `ReloadShader`, `FreeShader`, `ValidateShader`, `ShaderValidate`, `ShaderExists`, `ShaderCompiled`, `ShaderError`, `ShaderCompileError`, `ShaderLog`, `ShaderSource`, `ShaderVertexSource`, `ShaderFragmentSource`, `ShaderGeometrySource`, `ShaderComputeSource`, `ShaderGetSource`, `ShaderApply`, `EntityMaterial`, `ShaderEnable`, `ShaderDisable`, `ShaderClone`, `ShaderSave`, `ShaderHotReload`, `ShaderSetKeyword` |
| Shader parameters | `ShaderSetParameter`, `ShaderGetParameter`, `ShaderSetFloat`, `ShaderSetInt`, `ShaderSetBool`, `ShaderSetVector2`, `ShaderSetVector3`, `ShaderSetVector4`, `ShaderSetMatrix3`, `ShaderSetMatrix4`, `ShaderSetColor`, `ShaderSetTexture`, `ShaderSetTextureArray`, `ShaderSetCubeMap`, `ShaderSetTime`, `ShaderSetResolution`, `ShaderSetCamera`, `ShaderSetLight`, `ShaderSetMaterial`, `ShaderSetBuffer`, `ShaderParameterExists`, `ShaderParameterCount`, `ShaderParameterName`, `ShaderParameterType` |
| Shader debugging | `ShaderDebug`, `ShaderGetUniforms`, `ShaderGetAttributes`, `ShaderGetBindings`, `ShaderGetResourceInfo`, `ShaderGetCompilationStatus`, `ShaderGetBackend` |
| PBR materials | `CreatePBRMaterial`, `SetAlbedo`, `SetAlbedoMap`, `SetMetallic`, `SetRoughness`, `SetEmissive`, `SetNormalMap`, `SetMetallicMap`, `SetRoughnessMap`, `SetAOMap`, `SetEmissiveMap`, `SetOpacityMap`, `SetOpacity`, `SetHeightMap`, `SetEnvironmentMap`, `SetReflectionIntensity`, `SetFresnel`, `SetParallaxScale`, `SetMaterialWorkflow` |
| Toon and stylized materials | `CreateToonMaterial`, `SetToonRamp`, `SetLightingRamp`, `SetToonSteps`, `SetToonOutline`, `SetOutlineColor`, `SetOutlineWidth`, `SetRimLighting`, `SetRimColor`, `SetRimPower`, `SetShadingMode`, `SetVertexLighting`, `SetFlatShading`, `SetPixelShading` |
| Water, wind and foliage | `CreateWaterMaterial`, `SetWaterWaves`, `SetWaterSpeed`, `SetWaterDepth`, `SetWaterReflection`, `SetWaterRefraction`, `SetWaterFoam`, `SetWaterColor`, `SetWindDirection`, `SetWindStrength`, `CreateFoliageMaterial`, `SetFoliageWind` |
| Procedural materials | `CreateProceduralShader`, `SetGradient`, `SetGradientColor`, `SetGradientDirection`, `SetNoiseType`, `SetNoiseScale`, `SetNoiseSpeed`, `SetNoiseOctaves`, `SetNoisePersistence`, `SetNoiseLacunarity`, `SetNoiseStrength`, `SetPattern`, `SetPatternScale`, `SetPatternColor`, `SetPatternRotation`, `SetDistortion`, `SetProceduralSeed`, `BakeProceduralTexture` |
| Post-processing | `CreatePostProcess`, `LoadPostProcess`, `AddPostProcess`, `RemovePostProcess`, `FreePostProcess`, `SetPostProcess`, `SetPostProcessParameter`, `SetPostProcessEnabled`, `SetPostProcessOrder`, `PostProcessInput`, `PostProcessOutput`, `PostProcessResolution`, `PostProcessCamera`, `PostProcessPriority`, `PostProcessShader`, `SetUnderwaterEffect`, `SetRainEffect`, `SetSnowEffect`, `SetAtmosphericFog` |
| Shadows | `SetShadowMode`, `SetShadowQuality`, `SetShadowResolution`, `SetShadowBias`, `SetShadowSoftness`, `SetShadowDistance`, `SetShadowCascadeCount`, `EnableShadows`, `DisableShadows` |

## How it works

| Piece | File |
|---|---|
| Source processing: includes, directives, sections, uniforms, generated passes, error line maps | `crates/neptune-runtime/src/shadersrc.rs` |
| The GLSL library, standard vertex program, presets | `crates/neptune-runtime/src/shaderlib.rs` |
| WGSL through naga | `crates/neptune-runtime/src/wgsl.rs` |
| Shader objects, compile cache, GPU compile, parameters, materials, hot reload, reflection | `crates/neptune-runtime/src/shader.rs` |
| Ready-made materials, CPU procedural baking | `crates/neptune-runtime/src/matcmds.rs` |
| Post-processing chains and the Fyrox render pass | `crates/neptune-runtime/src/postfx.rs` |
| Shadow settings | `crates/neptune-runtime/src/quality.rs` |

Compiling a shader: the source is preprocessed (includes expanded, keyword
defines added, directives read, `uniform` lines turned into parameters),
each pass is assembled (shadow passes reuse the vertex code), parameters
are qualified with their uniform block, and the result is written as a
Fyrox shader. Fyrox parses it, then `RenderPassContainer::new` builds every
pass's GPU program on the spot; driver messages are mapped back through
per-line source maps. The compiled shader goes into a cache keyed by its
text. Materials are Fyrox materials made from it, shared between entities.

Post-processing is a Fyrox `SceneRenderPass`. Each frame the program's
chains are turned into a list of passes; the render pass copies the frame,
runs each effect's program on a full-screen quad (ping-ponging between two
reused targets, or into effect targets and textures), and writes the
result back, in the HDR stage for negative priorities and the LDR stage
otherwise.
