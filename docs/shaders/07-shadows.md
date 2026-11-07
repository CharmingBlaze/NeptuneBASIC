# 7. Shadows

[Contents](../SHADERS.md) · Previous: [6. Post-processing](06-post-processing.md) · Next: [8. WGSL and Fyrox shader files](08-wgsl-and-fyrox.md)

Shadows make a scene feel solid: they show where things stand and how
far apart they are. NeptuneBASIC draws real-time shadows from directional,
spot and point lights.

- [How shadows work](#how-shadows-work)
- [Turning shadows on](#turning-shadows-on)
- [Shadow settings](#shadow-settings)
- [Lights](#lights)
- [Which objects cast and receive shadows](#which-objects-cast-and-receive-shadows)
- [Fixing common problems](#fixing-common-problems)

## How shadows work

For each light, the engine first draws the scene from the light's point
of view, keeping only how far away each surface is. This picture is the
light's **shadow map**. Then, when it draws a pixel of the scene, it
checks the shadow map: if something was closer to the light than this
pixel, the pixel is in shadow.

The shadow map is a texture, so it has a resolution: a bigger shadow map
gives sharper shadows and costs more memory and time.

- **Directional lights** (the sun) light the whole world. Their shadow
  map is split into up to three **cascades**: a detailed one near the
  camera, and coarser ones further away, so near shadows stay sharp
  without a giant texture.
- **Spot lights** have one shadow map, covering their cone.
- **Point lights** shine every way, so they need six shadow maps (a cube
  map), at half the resolution of the others.

## Turning shadows on

```basic
; a scene with soft shadows
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 3, -8
TurnEntity camera, 15, 0, 0

sun = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity sun, 50, 30, 0

SetShadowMode SHADOW_SOFT       ; shadows on, with soft edges
SetShadowQuality 2              ; a 2048 shadow map

ground = CreatePlane()
For i = 0 To 4
    pillar = CreateCylinder(16)
    ScaleEntity pillar, 0.4, 2, 0.4
    PositionEntity pillar, i * 2 - 4, 2, 0
Next

While Not KeyHit(KEY_ESCAPE)
    TurnEntity sun, 0, 0.2, 0       ; the sun moves, the shadows follow
    RenderWorld
    Flip
Wend
```

## Shadow settings

These set shadows for the whole program. They can be called at any time.

| Command | Effect |
|---|---|
| `SetShadowMode mode` | `SHADOW_OFF` (none), `SHADOW_HARD` (sharp edges), `SHADOW_SOFT` (filtered, softer edges) |
| `EnableShadows`, `DisableShadows` | shadows on or off, keeping the other settings |
| `SetShadowQuality level` | 0: 512 map, half precision. 1: 1024, half. 2: 2048, full. 3: 4096, full |
| `SetShadowResolution size` | the shadow map size directly, 64-8192 (point lights use half) |
| `SetShadowDistance distance` | how far from the camera shadows reach, in units (64 to start with) |
| `SetShadowCascadeCount count` | 1-3 cascades for directional lights (3 to start with) |
| `SetShadowBias bias` | moves shadows away from surfaces slightly, against shadow acne |
| `SetShadowSoftness softness` | above 0: soft edges; 0: hard edges |

**Shadow distance and cascades go together.** The cascades share the
shadow distance: with three cascades the first covers the nearest 8%,
the second up to 40% and the third the rest. A shorter distance makes
every cascade cover less ground, so shadows get sharper; a longer one
lets shadows reach further but blurs them.

| Your game | Try |
|---|---|
| close-up, indoors | `SetShadowDistance 30`, 2 cascades |
| third person outdoors | `SetShadowDistance 80`, 3 cascades |
| big open world from above | `SetShadowDistance 200`, 3 cascades, `SetShadowQuality 3` |
| slow graphics card | `SetShadowQuality 1`, `SetShadowMode SHADOW_HARD` |

Soft shadows use Fyrox's filtering (PCF), which has a fixed softness:
`SetShadowSoftness` turns it on and off rather than setting how soft.

## Lights

```basic
lamp = CreateLight(LIGHT_POINT)
LightShadows lamp, False        ; this light casts no shadows
```

Every light casts shadows when shadows are on. `LightShadows light,
enable` turns them off (or on again) for one light. Shadows from many
lights are expensive: give shadows to the lights that matter (the sun,
the player's torch) and turn them off for small fill lights.

`SetShadowBias` applies to every light, including lights made later.

## Which objects cast and receive shadows

| Drawn with | Casts | Receives |
|---|---|---|
| the normal material | yes | yes |
| `CreatePBRMaterial` (solid or cut-out) | yes | yes |
| `CreatePBRMaterial` with `OPACITY_BLEND` | yes | no |
| `builtin:terrain`, `builtin:dissolve` | yes | yes |
| toon, cel and flat materials | yes | no |
| other stylized materials (unlit, PS1, pixel art, vertex-lit, matcap, posterize, hand-painted, rim) | no | no |
| `textured`, `diffuse`, `phong`, `blinnphong`, foliage, `displace`, procedural materials | yes | no |
| water, glass, hologram, fire, smoke, force field | no | no |
| your shader with `//! shadows on` | yes | only with `//! pass gbuffer` |
| your shader without it | no | only with `//! pass gbuffer` |

Objects that are hidden (`HideEntity`) cast no shadows. Shaders that move
vertices cast shadows of the moved shape, and cut-out shaders
(`//! alpha_clip on`) cast shadows with holes.

Receiving shadows needs the deferred pass, where Fyrox's lighting reads
the shadow maps. Forward-drawn surfaces (blended, toon, custom lighting)
are lit by the lights but are not darkened by shadows.

## Fixing common problems

| Problem | Looks like | Fix |
|---|---|---|
| **Shadow acne** | stripes or speckles of shadow on lit surfaces | raise `SetShadowBias` a little (try 0.0005, then 0.001) |
| **Peter-panning** | shadows start a little away from the object, which seems to float | lower `SetShadowBias` |
| **Blocky shadows** | jagged, pixelated shadow edges | `SetShadowQuality 3`, a shorter `SetShadowDistance`, or `SHADOW_SOFT` |
| **Shadows vanish in the distance** | far objects have no shadow | raise `SetShadowDistance` |
| **Shadows pop between sharp and blurry** | a line where the detail changes | that is a cascade edge: a shorter distance or more cascades |
| **Thin objects lose their shadows** | fences, leaves | a higher resolution; for leaves use cut-out, not blended, opacity |
| **No shadow on a custom shader** | the object casts nothing | add `//! shadows on` |
| **Custom shader is not shadowed** | other objects' shadows don't fall on it | add `//! pass gbuffer` (it will use PBR lighting) |
| **Everything is slow** | low frame rate with shadows on | fewer shadow-casting lights (`LightShadows light, False`), `SetShadowQuality 1` |

[Contents](../SHADERS.md) · Previous: [6. Post-processing](06-post-processing.md) · Next: [8. WGSL and Fyrox shader files](08-wgsl-and-fyrox.md)
