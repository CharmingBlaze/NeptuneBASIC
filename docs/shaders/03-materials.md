# 3. Ready-made materials

[Contents](../SHADERS.md) · Previous: [2. First steps](02-first-steps.md) · Next: [4. Writing shaders](04-writing-shaders.md)

Ready-made materials give you good-looking surfaces without writing any
shader code. Each `Create...Material` command returns a material; `Set...`
commands change it; `EntityMaterial` draws an entity with it.

- [How materials work](#how-materials-work)
- [PBR materials](#pbr-materials)
- [Toon and stylized materials](#toon-and-stylized-materials)
- [Water](#water)
- [Foliage and wind](#foliage-and-wind)
- [Procedural materials](#procedural-materials)
- [Built-in shaders](#built-in-shaders)
- [One entity, different settings](#one-entity-different-settings)
- [Which material should I use?](#which-material-should-i-use)

## How materials work

```basic
paint = CreatePBRMaterial()       ; 1. make a material
SetAlbedo paint, 200, 30, 30      ; 2. set it up
car = CreateCube()
EntityMaterial car, paint         ; 3. draw an entity with it
door = CreateCube()
EntityMaterial door, paint        ; any number of entities can share it
SetAlbedo paint, 30, 30, 200      ; changing it changes car and door
EntityMaterial door, 0            ; 0: back to the normal material
```

Things worth knowing:

- **A material is a shader.** Every material is a shader object with its
  own parameters, so all the `Shader...` commands work on materials too.
  `SetRoughness m, 0.3` and `ShaderSetFloat m, "roughness", 0.3` do the
  same thing.
- **Materials are cheap.** A thousand `CreatePBRMaterial` calls still
  compile the PBR shader only once; each material only stores its
  settings.
- **Sharing is fastest.** Entities that share a material are drawn
  together by the engine. Make a new material only when the settings
  differ.
- **A copy** with its own settings: `ShaderClone(material)`.
- **Freeing**: `FreeShader material` frees it; its entities go back to
  their normal materials.

## PBR materials

PBR ("physically based rendering") materials behave like real surfaces:
light bounces off them the way it does off real metal, plastic, wood or
stone. They are lit by every light in the scene, reflect the sky, and
cast and receive shadows.

```basic
; a brick wall with a full set of texture maps
wall = CreateCube()
ScaleEntity wall, 4, 2, 0.2

brick = CreatePBRMaterial()
SetAlbedoMap brick, LoadTexture("brick_color.png")
SetNormalMap brick, LoadTexture("brick_normal.png"), 1.0
SetRoughnessMap brick, LoadTexture("brick_rough.png")
SetAOMap brick, LoadTexture("brick_ao.png")
SetHeightMap brick, LoadTexture("brick_height.png"), 0.04
EntityMaterial wall, brick
```

### The three main settings

Almost every real material is described by three numbers:

| Setting | Command | Range | Default |
|---|---|---|---|
| colour | `SetAlbedo m, red, green, blue [, alpha]` | 0-255 | 255, 255, 255 |
| metal or not | `SetMetallic m, value` | 0 (not metal) to 1 (metal) | 0 |
| roughness | `SetRoughness m, value` | 0 (mirror) to 1 (matte) | 0.5 |

Some starting points:

| Material | Colour | Metallic | Roughness |
|---|---|---|---|
| gold | 255, 200, 90 | 1 | 0.2-0.35 |
| silver, chrome | 235, 235, 240 | 1 | 0.05-0.2 |
| iron, steel | 160, 160, 165 | 1 | 0.4-0.6 |
| copper | 240, 150, 110 | 1 | 0.3 |
| shiny plastic | any | 0 | 0.2-0.3 |
| rubber | any, dark | 0 | 0.8-0.9 |
| wood | 140, 90, 50 | 0 | 0.6-0.8 |
| stone, concrete | 150, 150, 145 | 0 | 0.8-1 |
| skin | 230, 180, 150 | 0 | 0.5 |
| car paint | any | 0 | 0.1-0.2 |

Metals get their colour from their reflections, so a metal with a dark
colour looks dark; non-metals have a white highlight whatever their
colour.

### Texture maps

Each setting can also vary across the surface, from a texture:

| Command | The texture holds | Notes |
|---|---|---|
| `SetAlbedoMap m, tex` | the colour | multiplied by `SetAlbedo`'s colour |
| `SetMetallicMap m, tex` | metal (white) or not (black), in the red channel | multiplied by `SetMetallic` |
| `SetRoughnessMap m, tex` | rough (white) or smooth (black), in the red channel | multiplied by `SetRoughness` |
| `SetNormalMap m, tex [, strength]` | small bumps, as a normal map | strength 0 flat, 1 normal, 2 deeper |
| `SetAOMap m, tex` | creases (black) that ambient light misses | |
| `SetEmissiveMap m, tex` | the colour of glowing parts | needs `SetEmissive` to glow |
| `SetOpacityMap m, tex [, mode]` | see-through (black) or solid (white), red channel | see below |
| `SetHeightMap m, tex [, scale]` | height, white high | parallax depth, default 0.05 |

Pass texture 0 to any of them to remove the map.

When `SetMetallicMap` or `SetRoughnessMap` is used, set the matching value
to 1 so the texture is used as it is: `SetMetallic m, 1`.

### Glowing

```basic
lamp = CreatePBRMaterial()
SetAlbedo lamp, 40, 40, 40
SetEmissive lamp, 255, 220, 150, 4     ; colour, then strength
```

`SetEmissive m, red, green, blue [, strength]` makes a surface give off
light of its own. It is not lit or shadowed, and with a strength above
about 1 it is bright enough to make **bloom** (a glow around it). It does
not light up other objects; add a light for that.

### See-through materials

```basic
glassy = CreatePBRMaterial()
SetAlbedo glassy, 180, 220, 255
SetOpacity glassy, 0.3                       ; 30% opaque, blended

leaves = CreatePBRMaterial()
SetAlbedoMap leaves, LoadTexture("leaf.png")
SetOpacityMap leaves, LoadTexture("leaf_mask.png"), OPACITY_CUTOUT
```

There are two kinds of transparency:

| Mode | Looks | Shadows | Use for |
|---|---|---|---|
| `OPACITY_BLEND` | smooth: things behind show through | not received | glass, ghosts, water, smoke |
| `OPACITY_CUTOUT` | parts are either there or not, cut at 50% | cast and received | leaves, fences, grilles, hair cards |
| `OPACITY_OPAQUE` | solid; ignores opacity | cast and received | turning it off again |

`SetOpacity m, value [, mode]` uses `OPACITY_BLEND` unless you say
otherwise; `SetOpacityMap m, tex [, mode]` uses `OPACITY_CUTOUT`.

### Reflections

```basic
sky = LoadTexture("sky_cube.png", 128)          ; flag 128: a cube map
chrome = CreatePBRMaterial()
SetAlbedo chrome, 235, 235, 240
SetMetallic chrome, 1
SetRoughness chrome, 0.05
SetEnvironmentMap chrome, sky, 1.0              ; reflect the sky
SetFresnel chrome, 1                            ; stronger at grazing angles
```

| Command | Effect |
|---|---|
| `SetEnvironmentMap m, cubemap [, intensity]` | reflect a cube map; intensity 0-1 or more |
| `SetReflectionIntensity m, value` | change the intensity later |
| `SetFresnel m, amount` | 0: reflections the same from every angle; 1: much stronger at glancing angles, as in reality |

Without an environment map, what PBR materials reflect depends on
`EnvironmentLighting`. In mode 0 (the default) they get
only the `AmbientLight` colour and reflect nothing. In mode 1 they are lit
by, and reflect, the sky box, or the **reflection probe** the camera is
in:

```basic
; a room whose shiny floor reflects the room
room = CreateCube()
ScaleEntity room, 10, 4, 10
FlipMesh room
probe = CreateReflectionProbe(256)      ; switches EnvironmentLighting to 1
ScaleEntity probe, 10, 4, 10            ; its box: the camera must be inside
floor = CreatePBRMaterial()
SetRoughness floor, 0.1
```

A probe takes a picture of the scene in every direction from where it
is. `UpdateProbe probe` retakes it after the scene changes, and
`ProbeMode probe, 1` retakes it every frame (costly). `ProbeTexture(probe)`
returns the picture as a cube map, for `SetEnvironmentMap` or your own
shaders. The probe commands are in the [command reference](../COMMANDS.md).

### Depth with parallax

`SetHeightMap m, tex, scale` shifts the texture by a height map, so flat
brick, cobbles or tiles look as if they have real depth when you move
past them. Keep the scale small (0.02-0.08). `SetParallaxScale m, scale`
changes it later; 0 turns it off.

### The specular workflow

Some texture sets come with "specular" maps instead of metallic maps. Use
them with:

```basic
old = CreatePBRMaterial()
SetMaterialWorkflow old, WORKFLOW_SPECULAR, 40, 40, 40   ; specular colour
```

`WORKFLOW_METALLIC` (the default) goes back to metallic/roughness.

## Toon and stylized materials

Toon materials light surfaces in a few flat bands, like a drawing, and
can draw outlines and glowing rims. The same command makes a dozen other
non-realistic looks.

```basic
; a cartoon character
hero = CreateToonMaterial(SHADING_TOON)
SetAlbedo hero, 255, 140, 60
SetToonSteps hero, 3
SetToonOutline hero, True, 0.03
SetOutlineColor hero, 20, 20, 30
SetRimLighting hero, True, 0.5
SetRimColor hero, 255, 240, 200
player = LoadAnimMesh("examples/media/Fox.glb")
EntityMaterial player, hero
```

### Shading modes

`CreateToonMaterial(mode)` takes one of these (`SHADING_TOON` if you leave
it out). `SetShadingMode m, mode` switches an existing material to another
look, keeping the settings both looks have (colour, texture, ...).

| Mode | Look | Good for |
|---|---|---|
| `SHADING_TOON` | flat light bands, a highlight and a rim | cartoon games |
| `SHADING_CEL` | two tones, no highlight | anime and comic styles |
| `SHADING_FLAT` | one shade per triangle | low-poly games |
| `SHADING_UNLIT` | colour and texture only, no lighting | UI in 3D, skies, glowing signs |
| `SHADING_VERTEXLIT` | lighting worked out at the corners and blended, like 1990s hardware | retro games |
| `SHADING_PIXEL` | crisp, unfiltered texture pixels and banded light | pixel-art 3D |
| `SHADING_PS1` | wobbly snapped vertices, swimming textures, few colours | PlayStation 1 style |
| `SHADING_MATCAP` | shading from a "lit sphere" picture | sculpture, clay, quick previews |
| `SHADING_POSTERIZE` | colours cut down to a few levels | poster and print styles |
| `SHADING_HANDPAINTED` | soft warm-to-cool shading with brush strokes | painterly games |
| `SHADING_LAMBERT` | plain matte lighting | simple objects |
| `SHADING_PHONG` | classic shiny highlight | retro-shiny objects |
| `SHADING_PBR` | realistic lighting, drawn forward | mixing with toon objects |
| `SHADING_RIM` | dark with glowing edges | ghosts, sci-fi |

`SetVertexLighting m, True`, `SetFlatShading m, True` and
`SetPixelShading m, True` are short ways to switch to those three looks;
`False` goes back to toon shading.

A matcap material needs its picture: `ShaderSetTexture m, "matcap", tex`.

### Light bands

| Command | Effect | Default |
|---|---|---|
| `SetToonSteps m, steps` | number of lit bands; 1 gives two tones | 3 (toon), 2 (cel) |
| `SetToonRamp m, tex` | colour the light from a ramp picture instead: dark on the left, lit on the right; 0 goes back to steps | none |
| `SetLightingRamp m, tex` | the same, under a name for any ramp-lit material | |
| `ShaderSetColor m, "shadeColor", r, g, b` | colour of the unlit side | 115, 115, 153 (cool blue-grey) |

A ramp picture is a small strip, say 256 × 1 pixels. Hard steps in the
picture give hard bands; a smooth fade gives soft shading.

### Outlines

```basic
SetToonOutline hero, True, 0.03      ; on, 0.03 units wide
SetOutlineColor hero, 0, 0, 0        ; black
SetOutlineWidth hero, 0.05           ; change the width later
SetToonOutline hero, False           ; off
```

The outline is a copy of the mesh, pushed out a little along its
normals and drawn inside out, so it shows only around the edges. Widths
are in world units, so a big object needs a wider outline. For outlines
drawn over the whole screen instead, use the `outline` screen effect
([chapter 6](06-post-processing.md)).

### Rim light

A rim light is a glow along the edges that face away from the camera,
which makes characters stand out from the background.

| Command | Effect | Default |
|---|---|---|
| `SetRimLighting m, enable [, strength]` | on or off, and how bright | strength 0.6 |
| `SetRimColor m, r, g, b` | its colour | white |
| `SetRimPower m, power` | how thin it is: higher is thinner | 3 |

## Water

```basic
; a sea
sea = CreateWaterMaterial()
SetWaterColor sea, 25, 140, 150, 5, 30, 64     ; shallow, then deep colour
SetWaterWaves sea, 0.3, 8, 1, 0.4              ; height, length, direction x, z
SetWaterSpeed sea, 1.2
SetWaterDepth sea, 5                            ; deep colour 5 units down
SetWaterFoam sea, 0.6
SetWaterReflection sea, 0.7
SetWaterRefraction sea, 0.5
EntityMaterial watermesh, sea
```

Water moves its vertices to make waves, so the water mesh needs many
vertices. A plane from `CreatePlane` or a single quad has only a few and
will not show waves; build a grid instead:

```basic
; a flat grid of size x size units with cells x cells squares
Function CreateWaterGrid(size#, cells)
    Local mesh = CreateMesh()
    Local surf = CreateSurface(mesh)
    For z = 0 To cells
        For x = 0 To cells
            AddVertex surf, (x / Float(cells) - 0.5) * size, 0, (z / Float(cells) - 0.5) * size, x / Float(cells), z / Float(cells)
            If x > 0 And z > 0
                Local v = z * (cells + 1) + x
                AddTriangle surf, v - cells - 2, v - 1, v
                AddTriangle surf, v - cells - 2, v, v - cells - 1
            EndIf
        Next
    Next
    UpdateNormals mesh
    Return mesh
End Function

watermesh = CreateWaterGrid(100, 128)
```

| Command | Effect | Default |
|---|---|---|
| `SetWaterWaves m, height [, length, dir_x, dir_z]` | wave height, distance between crests, and the direction they travel | 0.25, 6, 1, 0.3 |
| `SetWaterSpeed m, speed` | how fast the waves move | 1 |
| `SetWaterColor m, sr, sg, sb, dr, dg, db` | shallow and deep colours, 0-255 | 26, 140, 153 and 5, 31, 64 |
| `SetWaterDepth m, depth` | how deep (in units) before the water is fully the deep colour | 4 |
| `SetWaterFoam m, amount [, r, g, b]` | foam on crests and where the water touches things, and its colour | 0.5, white |
| `SetWaterReflection m, amount [, cubemap]` | how much it reflects (0-1), optionally a cube map of the sky | 0.6 |
| `SetWaterRefraction m, amount` | how much it bends the view of things under it | 0.5 |

Depth colour and foam come from the scene behind the water, so they show
wherever the water meets the ground or other objects. For being *under*
the water, see `SetUnderwaterEffect` in [chapter 6](06-post-processing.md).

## Foliage and wind

```basic
; grass and bushes that sway in the wind
SetWindDirection 1, 0.3          ; the wind blows towards +x, a little +z
SetWindStrength 0.3

grass = CreateFoliageMaterial()
SetAlbedoMap grass, LoadTexture("grass.png")
SetOpacityMap grass, LoadTexture("grass_mask.png")
EntityMaterial bush, grass
```

Foliage materials move their vertices with the wind. Higher vertices move
more than lower ones, so the tops of plants sway while their roots stay in
the ground: model plants standing upright, with their base at y = 0.

| Command | Effect | Default |
|---|---|---|
| `SetWindDirection x, z` | direction of the wind for every wind material | 1, 0 |
| `SetWindStrength strength` | how strong it is | 0.2 |
| `SetFoliageWind m, strength [, speed, stiffness]` | give one material its own wind instead | 1.5 speed, 1 stiffness |

Stiffness above 1 makes plants bend less; below 1, more. Gusts and
direction changes are just these commands called again over time.

## Procedural materials

Procedural materials make patterns from formulas instead of pictures:
gradients, noise (clouds, marble, rock, cells) and repeating patterns
(checkers, stripes, dots). They have no seams, never get blurry up close,
and can move.

```basic
; marble
marble = CreateProceduralShader(1)          ; 1 = noise
SetGradientColor marble, 0, 90, 90, 100     ; dark veins, where the noise is low
SetPatternColor marble, 240, 240, 235       ; light stone, where it is high
SetNoiseType marble, NOISE_PERLIN
SetNoiseScale marble, 3
SetNoiseOctaves marble, 6
SetDistortion marble, 0.4
EntityMaterial statue, marble
```

`CreateProceduralShader(kind)` starts from a gradient (0), noise (1) or a
pattern (2). All three can be combined in one material:

**1. The gradient** blends between two colours. Noise and patterns are
drawn on top of it.

| Command | Effect | Default |
|---|---|---|
| `SetGradient m, type` | `GRADIENT_LINEAR` (across), `GRADIENT_RADIAL` (from the middle out), `GRADIENT_ANGULAR` (around the middle), `GRADIENT_NONE` (only the first colour) | linear |
| `SetGradientColor m, index, r, g, b [, a]` | colour 0 (start) or 1 (end) | dark blue, orange |
| `SetGradientDirection m, angle` | the angle of a linear gradient in degrees: 0 runs across the texture (along u), 90 down it (along v) | 90 |

**2. Noise** is a smooth random pattern. A noise material (kind 1) draws
the noise as its pattern: it blends from gradient colour 0 where the
noise is low (black to start with) to the pattern colour where it is
high (white to start with). In gradient and pattern materials, noise can
also disturb the gradient (`SetNoiseStrength`) or warp everything
(`SetDistortion`).

| Command | Effect | Default |
|---|---|---|
| `SetNoiseType m, type` | the kind of noise, below | `NOISE_PERLIN` |
| `SetNoiseScale m, scale` | size of the features: higher means smaller and more of them | 4 |
| `SetNoiseOctaves m, octaves` | layers of finer detail, 1-12 | 4 |
| `SetNoisePersistence m, value` | how strong each finer layer is (0.5: half as strong) | 0.5 |
| `SetNoiseLacunarity m, value` | how much finer each layer is (2: twice as fine) | 2 |
| `SetNoiseSpeed m, speed` | how fast it moves; 0 is still | 0 |
| `SetNoiseStrength m, strength` | in gradient and pattern materials, how much the noise disturbs the gradient | 0 |
| `SetDistortion m, amount` | warp the gradient and pattern with the noise | 0 |
| `SetProceduralSeed m, seed` | a different variation of the same noise | 0 |

| Noise type | Looks like |
|---|---|
| `NOISE_VALUE` | soft, blocky blobs |
| `NOISE_PERLIN` | smooth hills: clouds, smoke, marble |
| `NOISE_SIMPLEX` | like Perlin, with fewer straight-line artefacts |
| `NOISE_VORONOI` | cells with soft centres: scales, stones, caustics |
| `NOISE_CELLULAR` | cells with sharp edges: cracked mud, giraffe skin |

Octaves are the secret of natural-looking noise: one octave is a soft
blur; six layers of finer and fainter noise on top of each other look like
rock or clouds.

**3. A pattern** is drawn over the gradient in its own colour.

| Command | Effect | Default |
|---|---|---|
| `SetPattern m, type` | `PATTERN_NONE`, `PATTERN_CHECKER`, `PATTERN_GRID`, `PATTERN_STRIPES`, `PATTERN_DOTS`, `PATTERN_CELLS`, `PATTERN_NOISE` (the noise itself) | none; checker for kind 2, noise for kind 1 |
| `SetPatternScale m, scale` | how many times it repeats across the surface | 8 |
| `SetPatternColor m, r, g, b [, a]` | its colour; alpha sets how strongly it covers the gradient | white |
| `SetPatternRotation m, angle` | rotate it, in degrees | 0 |

```basic
; floor tiles: a checker pattern over a subtle noise
tiles = CreateProceduralShader(2)
SetPattern tiles, PATTERN_CHECKER
SetPatternScale tiles, 8
SetPatternColor tiles, 30, 30, 30, 200
SetGradient tiles, GRADIENT_NONE
SetGradientColor tiles, 0, 220, 215, 200
SetNoiseStrength tiles, 0.15

; a sunset sky band
sunset = CreateProceduralShader(0)
SetGradientColor sunset, 0, 255, 120, 40
SetGradientColor sunset, 1, 40, 30, 120
SetGradientDirection sunset, 90

; lava that flows
lava = CreateProceduralShader(1)
SetNoiseType lava, NOISE_CELLULAR
SetGradientColor lava, 0, 120, 10, 0       ; dark red along the cracks between cells
SetPatternColor lava, 255, 220, 60         ; glowing yellow cell centres
SetNoiseSpeed lava, 0.3
```

### Turning a procedural material into a texture

`BakeProceduralTexture(m [, width, height, normal_map])` draws a
procedural material into an ordinary texture. Use it to give any
material, model or sprite a procedural pattern, or to make a normal map
from noise:

```basic
rocky = CreateProceduralShader(1)
SetNoiseType rocky, NOISE_VORONOI
SetNoiseScale rocky, 8
colour = BakeProceduralTexture(rocky, 512, 512)
bumps = BakeProceduralTexture(rocky, 512, 512, True)    ; a normal map

boulder = CreatePBRMaterial()
SetAlbedoMap boulder, colour
SetNormalMap boulder, bumps, 1.2
SetRoughness boulder, 0.9
```

Baking runs on the CPU, so do it while loading, not every frame. A baked
texture does not move with `SetNoiseSpeed`.

## Built-in shaders

Every ready-made material is built from one of these shaders, and there
are more for special effects. Load one with `LoadShader("builtin:name")`,
set its parameters with the `ShaderSet...` commands, and apply it with
`ShaderApply shader, entity` (or `EntityMaterial entity, shader`).

```basic
shield = LoadShader("builtin:forcefield")
ShaderSetColor shield, "color", 80, 255, 120      ; green instead of blue
ShaderSetFloat shield, "hexScale", 20             ; smaller cells
bubble = CreateSphere(32)
ScaleEntity bubble, 3, 3, 3
ShaderApply shield, bubble
```

Colour parameters (marked "colour") are set with `ShaderSetColor` in
0-255; their defaults are shown here in 0-1, as the shader sees them.
Every shader also has the automatic values described in
[chapter 5](05-shader-language.md#values-filled-in-for-you).

**Basic lighting**

| Name | What it is | Parameters (default) |
|---|---|---|
| `unlit` | colour × texture, no lighting | `albedo` colour (1,1,1,1), `albedoMap`, `uvScale` (1,1) |
| `textured` | a texture, with realistic lighting | `albedo`, `albedoMap`, `uvScale`, `roughness` (0.6), `specularColor` (0.6), `shininess` (32) |
| `vertexcolor` | vertex colours × texture, matte | as `textured` |
| `diffuse` | matte (Lambert) lighting | as `textured` |
| `phong`, `blinnphong` | classic shiny highlights | as `textured`; `shininess` sets the size of the highlight |

**Realistic (PBR)**

| Name | What it is | Parameters (default) |
|---|---|---|
| `pbr` | the shader of `CreatePBRMaterial` | `albedo`, `albedoMap`, `normalMap`, `normalStrength` (1), `metallic` (0), `roughness` (0.5), `metallicMap`, `roughnessMap`, `aoMap`, `emissive` (0,0,0), `emissiveStrength` (1), `emissiveMap`, `opacity` (1), `opacityMap`, `heightMap`, `parallaxScale` (0), `envMap`, `reflection` (0), `fresnel` (1), `specularColor` (0.04), `workflow` (0), `uvScale` (1,1), `alphaCutoff` (0.5) |
| `normalmap` | the same, for normal-mapped objects | as `pbr` |
| `metal` | the same, set up as polished metal | as `pbr`, `metallic` 1, `roughness` 0.25 |
| `glass` | see-through and reflective | `albedo` (0.85,0.95,1,0.25), `envMap`, `reflection` (0.8), `fresnel` (3), `shininess` (96), `refraction` (0.03) |
| `water` | the shader of `CreateWaterMaterial` | `shallowColor`, `deepColor`, `waveHeight` (0.25), `waveLength` (6), `waveSpeed` (1), `waveDirection` (1,0.3), `depthFade` (4), `reflection` (0.6), `refraction` (0.5), `foamAmount` (0.5), `foamColor`, `envMap`, `opacity` (0.85) |
| `terrain` | grass, rock, sand and snow for terrains | see [Terrain](#terrain) |

**Stylized**

| Name | What it is | Parameters (default) |
|---|---|---|
| `toon` | the shader of `CreateToonMaterial(SHADING_TOON)` | `albedo`, `albedoMap`, `toonRamp`, `useRamp` (0), `toonSteps` (3), `shadeColor` (0.45,0.45,0.6), `specularColor` (0.8), `shininess` (48), `rimColor` (1,1,1), `rimPower` (3), `rimStrength` (0.35) |
| `cel` | two-tone toon | as `toon`, 2 steps, no highlight or rim |
| `outline` | just an outline (inverted hull) | `outlineColor` (0,0,0,1), `outlineWidth` (0.03) |
| `rim` | dark with glowing edges | `albedo` (0.2,0.2,0.25,1), `rimColor` (0.3,0.8,1), `rimPower` (2.5), `rimStrength` (2) |
| `flat` | one shade per triangle | `albedo`, `albedoMap` |
| `vertexlit` | lighting per vertex | `albedo`, `albedoMap` |
| `ps1` | 1990s console look | `albedo`, `albedoMap`, `snap` (160, vertex wobble: lower is wobblier), `colorDepth` (32 levels) |
| `pixelart` | crisp pixels, banded light | `albedo`, `albedoMap`, `pixelsPerUnit` (16), `toonSteps` (4) |
| `matcap` | lit-sphere shading | `matcap` texture, `albedo` |
| `posterize` | few colour levels | `albedo`, `albedoMap`, `levels` (4) |
| `handpainted` | painterly | `albedo`, `albedoMap`, `shadeColor` (0.55,0.5,0.75), `strokes` (0.12), `strokeScale` (18) |

**Effects**

| Name | What it is | Parameters (default) |
|---|---|---|
| `hologram` | glowing see-through scan lines that flicker | `color` (0.2,0.9,1), `lineDensity` (60), `speed` (1.5), `flicker` (0.15) |
| `dissolve` | burns away along a noise pattern | `albedo`, `albedoMap`, `amount` (0.5: 0 whole, 1 gone), `edgeWidth` (0.05), `edgeColor` (1,0.45,0.05), `noiseScale` (6) |
| `fire` | animated flames, for sprites | `innerColor` (1,0.85,0.3), `outerColor` (1,0.25,0.02), `speed` (1.6), `intensity` (3) |
| `smoke` | animated smoke, for sprites | `color` (0.55,0.55,0.58,0.8), `speed` (0.3), `density` (1.2) |
| `forcefield` | a shield bubble: cells, glowing rim, and a glow where it touches things | `color` (0.3,0.6,1), `intersection` (0.6), `hexScale` (12), `pulse` (1) |
| `wind` | the shader of `CreateFoliageMaterial` | `albedo`, `albedoMap`, `windDirection` (1,0), `windStrength` (0.2), `windSpeed` (1.5), `stiffness` (1) |
| `displace` | moves vertices by a height map and waves | `albedo`, `albedoMap`, `heightMap`, `amount` (0.2), `speed` (1), `frequency` (6) |

**Procedural**

| Name | What it is |
|---|---|
| `gradient`, `noise`, `procedural` | the shader of `CreateProceduralShader` (kinds 0, 1 and 2). Parameters: `gradientType`, `gradientColorA`, `gradientColorB`, `gradientDirection`, `noiseType`, `noiseScale`, `noiseSpeed`, `noiseOctaves`, `noisePersistence`, `noiseLacunarity`, `noiseStrength`, `patternType`, `patternScale`, `patternColor`, `patternRotation`, `distortion`, `seed`, `roughness` (0.6), `metallic` (0), `bumpStrength` (0) |

A dissolving enemy, using the `amount` parameter:

```basic
; an enemy that burns away over two seconds
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 0, -4
light = CreateLight()
enemy = CreateCube()
burn = LoadShader("builtin:dissolve")
ShaderApply burn, enemy
start = MilliSecs()
While Not KeyHit(KEY_ESCAPE)
    t# = (MilliSecs() - start) / 2000.0
    If t > 1 Then t = 1
    ShaderSetFloat burn, "amount", t
    RenderWorld
    Flip
Wend
```

### Terrain

`builtin:terrain` draws a terrain (from `CreateTerrain`, or any mesh)
with grass, rock, sand and snow, worked out per pixel:

- rock on steep slopes (`rockSlope`),
- sand up to `sandHeight` above `seaLevel`, darker and glossy just above
  the water line,
- snow above `snowHeight`,
- grass everywhere else, with dry patches.

With `painted` set to 1 it uses the weights painted with `PaintTerrain` or
`ModifyTerrainLayer` instead (layer 1 rock, 2 sand, 3 snow). Fine detail
(rock layers and cracks, sand ripples) fades out with distance.

| Parameter | Meaning | Default |
|---|---|---|
| `grassColor`, `dryGrassColor`, `rockColor`, `rockColor2`, `sandColor`, `snowColor` | colours | greens, greys, sand, white |
| `seaLevel` | height of the water line | none |
| `sandHeight` | how far above `seaLevel` sand reaches | 2.5 |
| `snowHeight` | height where snow starts | none |
| `rockSlope` | slope where rock starts and where it is full (as 1 − how upright the surface is) | 0.3, 0.5 |
| `painted` | 1: use painted layers | 0 |
| `detail`, `bumpStrength` | amount of fine detail and bumpiness | 1, 1 |
| `grassMap`, `rockMap`, `sandMap`, `snowMap`, `mapScale` | optional textures for each layer (rock is mapped from three sides so cliffs do not stretch), and their scale | none, 0.25 |

```basic
ground = CreateTerrain(256)
ScaleEntity ground, 2, 120, 2
land = LoadShader("builtin:terrain")
ShaderApply land, ground
ShaderSetFloat land, "seaLevel", 30
ShaderSetFloat land, "snowHeight", 100
```

The terrain shader draws in the deferred pass, so terrains are lit by
every light and receive shadows. `examples/island.nb` uses it.

### Reading a built-in shader

`ShaderGetSource(shader)` returns the source of any built-in shader.
They are written in exactly the language chapter 4 teaches, so they are
good starting points for your own:

```basic
toon = LoadShader("builtin:toon")
file = WriteFile("my_toon.glsl")
WriteLine file, ShaderGetSource(toon)
CloseFile file
```

## One entity, different settings

All entities with the same material share its settings. To change a
setting for just one of them, use the per-entity commands:

```basic
crate1 = CreateCube()
crate2 = CreateCube()
wood = LoadShader("builtin:diffuse")
EntityShader crate1, wood
EntityShader crate2, wood
ShaderFloat crate2, "shininess", 8            ; only crate2
ShaderVector crate2, "albedo", 1, 0.8, 0.6, 1 ; only crate2 (0-1 here)
ShaderTexture crate2, "albedoMap", LoadTexture("examples/media/checker.png")
```

| Command | Sets for one entity |
|---|---|
| `ShaderFloat entity, name$, value` | a number |
| `ShaderVector entity, name$, x, y [, z, w]` | a vector or colour (0-1) |
| `ShaderTexture entity, name$, texture` | a texture |

An entity with its own settings gets its own copy of the material, so
use them for a few special entities. For many variations, make a few
materials (`ShaderClone`) and share each between many entities.

## Which material should I use?

| You want | Use |
|---|---|
| realistic objects: metal, stone, wood, plastic | `CreatePBRMaterial` |
| a cartoon or anime look | `CreateToonMaterial(SHADING_TOON)` or `SHADING_CEL` |
| low poly | `CreateToonMaterial(SHADING_FLAT)` |
| a retro console look | `SHADING_PS1`, `SHADING_VERTEXLIT` or `SHADING_PIXEL` |
| glowing signs, skies, UI in the world | `CreateToonMaterial(SHADING_UNLIT)` or `SetEmissive` |
| a lake or sea | `CreateWaterMaterial` on a grid mesh |
| grass, bushes, trees | `CreateFoliageMaterial` |
| marble, wood grain, clouds, lava, tiles | `CreateProceduralShader` |
| glass | `LoadShader("builtin:glass")` or a PBR material with `SetOpacity` |
| a shield, hologram, fire or dissolve effect | the built-in shader of that name |
| a landscape | `LoadShader("builtin:terrain")` |
| something else | write a shader: [chapter 4](04-writing-shaders.md) |

[Contents](../SHADERS.md) · Previous: [2. First steps](02-first-steps.md) · Next: [4. Writing shaders](04-writing-shaders.md)
