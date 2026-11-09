# 18. 3D models and looks

[Contents](../MANUAL.md) · Previous: [17. 3D basics](17-3d-basics.md) · Next: [19. Collisions and physics](19-collisions-physics.md)

Cubes and spheres only go so far. This chapter covers loading models
made in a 3D program, playing their animations, giving surfaces
textures and materials, building meshes in code, sprites, landscapes and
skies.

- [Loading models](#loading-models)
- [Animation](#animation)
- [Animation states](#animation-states)
- [Textures](#textures)
- [How surfaces look](#how-surfaces-look)
- [Building meshes in code](#building-meshes-in-code)
- [Sprites](#sprites)
- [Terrain](#terrain)
- [Baked lighting](#baked-lighting)
- [Sky and mirrors](#sky-and-mirrors)
- [Animating anything](#animating-anything)

## Loading models

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 1, -4
light = CreateLight()

fox = LoadAnimMesh("examples/media/Fox.glb")
If fox = 0 Then RuntimeError "Fox.glb is missing"
ScaleEntity fox, 0.02, 0.02, 0.02          ; this model is large: shrink it

While Not KeyHit(KEY_ESCAPE)
    TurnEntity fox, 0, 1, 0
    RenderWorld
    Flip
Wend
```

| Command | Loads |
|---|---|
| `LoadMesh(file$ [, parent])` (or `LoadModel`) | a model as one entity |
| `LoadAnimMesh(file$ [, parent])` | a model with its bones and animations |

Models can be glTF (`.gltf`, `.glb`) or FBX files, with their textures
and materials. glTF is the best choice: free tools such as Blender export
it, and it keeps materials looking the same.

To build a whole level in Blender, with its lights, cameras, solid objects
and values for your program to read, see
[Making levels in Blender](../BLENDER.md) and the `LoadLevelFile` command.

A model's parts are child entities with the names they had in the 3D
program: `FindChild(model, "Door")` finds one, so a game can open the
door of a house model. `CopyEntity(model)` makes another copy cheaply;
load a model once and copy it for every tree in a forest.

## Animation

Models made with animations (walk, run, jump...) store them as
**sequences**, numbered from 0:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 1, -4
light = CreateLight()
fox = LoadAnimMesh("examples/media/Fox.glb")
ScaleEntity fox, 0.02, 0.02, 0.02
Animate fox, 1, 1, 0          ; mode 1 (loop), speed 1, sequence 0

While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_1) Then Animate fox, 1, 1, 1, 10   ; sequence 1, blending over 10 frames
    If KeyHit(KEY_2) Then Animate fox, 1, 1, 2, 10
    UpdateWorld                      ; animations advance here
    RenderWorld
    Flip
Wend
```

`Animate entity, mode, speed, sequence, transition`:

| Argument | Means |
|---|---|
| mode | 0 stop, 1 loop, 2 back and forth, 3 play once |
| speed | 1 normal, 2 twice as fast, 0.5 half |
| sequence | which animation |
| transition | frames to blend from the current pose, so changes are smooth |

`Animating(e)`, `AnimSeq(e)`, `AnimTime(e)` and `AnimLength(e)` say what is
playing; `SetAnimTime` jumps to a frame; `ExtractAnimSeq` cuts a new
sequence out of a long one.

## Animation states

Choosing animations by hand gets complicated: idle, walk, run, jump,
land, and the rules for going between them. An **animation state
machine** holds the rules and follows them by itself:

```basic
hero = LoadAnimMesh("examples/media/Fox.glb")
AddAnimState hero, "idle", 0, 1          ; sequence 0, looping
AddAnimState hero, "run", 2, 1
AddAnimState hero, "jump", 1, 3          ; played once
AddAnimTransition hero, "idle", "run", "speed > 0.1", 8
AddAnimTransition hero, "run", "idle", "speed <= 0.1", 8
AddAnimTransition hero, "*", "jump", "jump = 1", 4    ; from any state
AddAnimTransition hero, "jump", "idle", "end", 6      ; when the jump has finished
StartAnimStates hero, "idle"
; each frame, tell it what the character is doing:
SetAnimParam hero, "speed", 0.5
SetAnimParam hero, "jump", KeyHit(KEY_SPACE)
```

Conditions compare parameters you set with `SetAnimParam`; `"end"` waits
for a play-once animation to finish. **Blend spaces**
(`CreateBlendSpace`, `AddBlendState`) mix animations smoothly by a
parameter, such as walking into running as speed rises, and
**animation layers** (`AnimLayer`) play one animation on part of the
body (aiming with the arms while the legs run). `examples/animstates.nb`
shows them together.

## Textures

A **texture** is a picture on a surface:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
light = CreateLight()
crate = CreateCube()
PositionEntity crate, 0, 0, 5
wood = LoadTexture("examples/media/checker.png")
EntityTexture crate, wood
ScaleTexture wood, 0.5, 0.5          ; repeat it twice across each face
```

`LoadTexture(file$ [, flags])` loads one. The flags, added together,
change how it is used: 1 colour, 2 with alpha (see-through parts), 4
masked (black is see-through), 8 mipmapped (smooth in the distance), 16
and 32 clamped instead of repeating, 64 sphere-mapped (fake shine), 128 a
cube map (six faces, for skies and reflections).

| Command | Does |
|---|---|
| `EntityTexture e, tex [, frame, layer]` | puts it on an entity; layers mix several textures |
| `ScaleTexture tex, u, v` | above 1 repeats it more |
| `PositionTexture tex, u, v`, `RotateTexture tex, angle` | slides and turns it: change them each frame for flowing water or lava |
| `TextureBlend tex, mode` | how a layer mixes with those below: multiply, add, alpha... |
| `CreateTexture(w, h)` | a blank texture to draw into (`SetBuffer TextureBuffer(tex)`) |

## How surfaces look

Simple settings on the entity:

| Command | Does |
|---|---|
| `EntityColor e, r, g, b` | its colour |
| `EntityAlpha e, alpha` | see-through: 0 invisible, 1 solid |
| `EntityShininess e, value` | a highlight, 0 to 1 |
| `EntityMetallic e, value`, `EntityRoughness e, value` | metal or not, shiny or matte |
| `EntityEmissive e, r, g, b, intensity` | glows |
| `EntityBlend e, mode` | 1 normal, 2 multiply (darkens), 3 add (glows: fire, lasers) |
| `EntityFX e, flags` | 1 full-bright (ignores light), 2 vertex colours, 4 flat-shaded, 8 no fog, 16 both sides drawn |

**Brushes** hold a set of these settings to paint onto many entities at
once: `brush = CreateBrush(255, 0, 0)` then `PaintEntity e, brush`.

For realistic materials (gold, brick, wet stone), cartoon looks, water,
grass in the wind, patterns made from noise, glowing screen effects and
shaders of your own, see [the shader manual](../SHADERS.md):

```basic
gold = CreatePBRMaterial()
SetAlbedo gold, 255, 200, 90
SetMetallic gold, 1
SetRoughness gold, 0.25
ring = CreateSphere(32)
EntityMaterial ring, gold
```

## Building meshes in code

A **mesh** is made of **surfaces**; a surface is made of **vertices**
(corners) joined into **triangles**. You can build one yourself, for
procedural levels, terrain, or shapes that change:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 0, -4
light = CreateLight()

pyramid = CreateMesh()
surf = CreateSurface(pyramid)
top = AddVertex(surf, 0, 1, 0)           ; x, y, z
a = AddVertex(surf, -1, -1, -1)
b = AddVertex(surf, 1, -1, -1)
c = AddVertex(surf, 1, -1, 1)
d = AddVertex(surf, -1, -1, 1)
AddTriangle surf, top, b, a              ; corners in clockwise order, seen from outside
AddTriangle surf, top, c, b
AddTriangle surf, top, d, c
AddTriangle surf, top, a, d
UpdateNormals pyramid                    ; work out the lighting directions

While Not KeyHit(KEY_ESCAPE)
    TurnEntity pyramid, 0, 1, 0
    RenderWorld
    Flip
Wend
```

A triangle is only drawn from the side where its corners go round
clockwise; seen from the other side it is invisible. If a mesh looks
inside out, swap two corners of each triangle (or use `FlipMesh`).
`VertexColor` colours vertices (shown with `EntityFX e, 2`),
`VertexCoords` moves them, and `examples/meshes.nb` shows more.

## Sprites

A **sprite** is a flat picture that always faces the camera: smoke,
explosions, trees in the distance, health bars over heads.

```basic
Graphics3D 1280, 720
camera = CreateCamera()
puff = LoadSprite("examples/media/ring.png", 2)   ; flag 2: with alpha
PositionEntity puff, 0, 0, 5
ScaleSprite puff, 0.5, 0.5
```

`SpriteViewMode sprite, mode` chooses how it turns: 1 always faces the
camera, 3 turns only round its upright axis (good for trees). For many
particles, `CreateEmitter` makes a particle system
([chapter 21](21-worlds.md#rendering-effects)).

## Terrain

A **terrain** is a landscape: a grid of heights.

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 60, -50
TurnEntity camera, 30, 0, 0
light = CreateLight()

land = LoadTerrain("heightmap.png")       ; brightness is height
ScaleEntity land, 2, 50, 2                ; 2 units per grid square, 50 high
TerrainLayer land, LoadTexture("grass.png"), 0
TerrainLayer land, LoadTexture("rock.png"), 1
PaintTerrain land, 1, 100, 100, 12, 0.8   ; paint rock round world point 100, 100
```

| Command | Does |
|---|---|
| `CreateTerrain(size)` | a flat terrain of size × size squares (a power of 2) |
| `LoadTerrain(file$)` | heights from a grey image |
| `ModifyTerrain t, x, z, height`, `TerrainHeight(t, x, z)` | changes and reads a grid point's height (0 to 1) |
| `TerrainY(t, x, y, z)` | the ground height at a world position: keeps characters on the ground |
| `TerrainLayer t, tex, layer`, `PaintTerrain t, layer, x, z, radius, strength` | up to four textures, painted where you want them |
| `ErodeTerrain`, `ThermalErodeTerrain` | weathers mountains with simulated rain and crumbling |
| `TerrainDetail t, triangles` | level of detail: fewer triangles in the distance |

The `builtin:terrain` shader colours a terrain with grass, rock, sand and
snow by height and slope ([shader manual, chapter
3](../shaders/03-materials.md#terrain)); `examples/island.nb` uses it.

## Baked lighting

Real-time lights are bright and sharp, but soft light (the glow of the
sky in a room, dark corners, light bouncing off a red wall onto the floor)
is too slow to work out every frame. **Baking** works it out once and
stores it in textures called **lightmaps**:

```basic
Graphics3D 1280, 720
camera = CreateCamera()
room = CreateCube()
ScaleEntity room, 5, 3, 5
FlipMesh room
lamp = CreateLight(LIGHT_POINT)
AmbientLight 120, 120, 130
BakeLightmaps                    ; may take a few seconds
AmbientLight 30, 30, 35          ; turn the ambient light down afterwards
```

- `BakeLightmaps([texels_per_unit, quality, direct, entity])` bakes the
  light of the sky (the `AmbientLight` colour), darkening in corners and
  under things, and one bounce of light off nearby surfaces. The lights
  stay live. With `direct` set to 1, the lights' own light and shadows are
  baked too, and the lights can then be hidden.
- Only still scenery drawn with the standard material is baked: walls,
  floors, buildings, rocks. Moving things are lit as usual.
- `ClearLightmaps` removes it again (before baking after a change);
  `EntityLightmap e, texture` uses a lightmap made elsewhere.
- `examples/lightbake.nb` shows a baked room.

## Sky and mirrors

```basic
SkyBox LoadTexture("sky.png", 128)       ; a cube map: six faces side by side
floor = CreatePlane()
mirror = CreateMirror()                  ; reflects everything above it
```

## Animating anything

A **track** animates any setting of an entity over time, not just
bones: a door's position, a light's colour, a camera's zoom.

```basic
lamp = CreateLight(LIGHT_POINT)
glow = CreateTrack(lamp, "lightcolor")
TrackKey glow, 0, 255, 120, 0            ; at frame 0: orange
TrackKey glow, 60, 0, 120, 255           ; at frame 60: blue
PlayTrack glow, 2                        ; mode 2: back and forth
```

[Contents](../MANUAL.md) · Previous: [17. 3D basics](17-3d-basics.md) · Next: [19. Collisions and physics](19-collisions-physics.md)
