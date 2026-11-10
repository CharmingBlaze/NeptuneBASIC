# Making levels in Blender

Build your level in [Blender](https://www.blender.org) and let
NeptuneBASIC bring all of it into your game: the shapes, materials, lights,
cameras, which objects are solid, which ones fall, where the player starts
and anything else you want to mark. One command loads it:

```basic
level = LoadLevelFile("levels/castle.glb")
```

Blender is free and runs on Windows, Linux and macOS. Any other tool that
writes glTF works too, but this guide uses Blender.

A complete example comes with NeptuneBASIC. Run it first to see what this
guide describes:

```bash
neptune run examples/level.nb
```

Its level is `examples/media/courtyard.blend`. Open that file in Blender,
change something, export it and run the example again.

## Your first level, step by step

1. **Install the add-on (once).** In Blender, open *Edit > Preferences >
   Add-ons*, click the small arrow at the top right, choose *Install from
   Disk* and pick `tools/blender/neptune_levels.py`. Press N in the 3D view
   and you will see a **Neptune** tab.
2. **Build something.** Add a large cube for the ground.
3. **Make it solid.** Select it and press **Solid** in the Neptune tab. Its
   name now starts with `col_`, and that is all it takes.
4. **Add a light.** *Add > Light > Sun*.
5. **Mark the start.** *Add > Empty > Plain Axes*, move it to where the
   player should begin. Select it, press **Set Properties...** and type
   `spawn` as the Type.
6. **Export.** Save your `.blend` file in your game's folder, then press
   **Export Level** in the Neptune tab. You get a `.glb` file beside it.
7. **Load it:**

```basic
Graphics3D 1280, 720, 32, 2
AmbientLight 40, 40, 50

level = LoadLevelFile("level.glb")
start = LevelObject(level, 1, "spawn")

camera = CreateCamera()
CameraClsColor camera, 120, 160, 210
PositionEntity camera, EntityX(start, True), EntityY(start, True) + 1.7, EntityZ(start, True)

While Not KeyHit(1)
    UpdateWorld
    RenderWorld
    Flip
Wend
```

Each time you change the level, press **Export Level** and run the game.

## What comes across

| In Blender | In your game |
|---|---|
| Meshes and their materials | Drawn, with colour, metal, roughness, normal and glow maps |
| Parent and child objects | The same parents and children |
| Armatures and actions | Sequences for `Animate` |
| Empties | Invisible entities that mark a place |
| Sun, point and spot lights | Real lights: colour, strength, reach and cone |
| Cameras | Real cameras, switched off until you ask for one |
| Custom properties | Values your program can read, and settings for the engine |

Two things in a Blender file are not part of what it exports, so set them in
your program. The **World** background becomes `CameraClsColor` or `SkyBox`,
and its soft all-round light becomes `AmbientLight`.

## Finding objects from your program

There are two ways. Use `FindChild` when you know an object's name:

```basic
door = FindChild(level, "CastleDoor")
```

Use **custom properties** when there are many of the same thing. In Blender,
select an object, open the *Object* properties panel and add values under
*Custom Properties* (or use **Set Properties...** in the Neptune tab). By
custom, the property called `type` says what an object is. `CountLevelObjects`
and `LevelObject` can then give you every object of one type:

```basic
For i = 1 To CountLevelObjects(level, "coin")
    coin = LevelObject(level, i, "coin")
    value = EntityPropertyInt(coin, "value", 1)
Next
```

Numbering starts at 1. Without a type, they list every object in the level
that has at least one custom property.

Read a property with the command for the kind of value you want. Each takes
an optional value to return when the object does not have that property:

| Command | Gives |
|---|---|
| `EntityProperty(e, "name", "none")` | Text |
| `EntityPropertyInt(e, "health", 100)` | A whole number |
| `EntityPropertyFloat(e, "speed", 2.5)` | A decimal number |
| `EntityHasProperty(e, "name")` | 1 if the object has it, otherwise 0 |

Property names ignore capitals. A tick box in Blender reads as 1 or 0. Any
entity can have properties: `SetEntityProperty` adds or changes one from your
program, and `CountEntityProperties` and `EntityPropertyName` list them.

## Solid objects

Start an object's name with one of these. Blender keeps the start of a name
when you duplicate an object (`col_Wall.001`, `col_Wall.002`), so copies stay
solid.

| Name starts with | The object is |
|---|---|
| `col_` | Solid and visible: floors, walls, ramps |
| `hit_` | Solid and never drawn: invisible walls, or a simple shape standing in for a detailed one |

What "solid" means depends on which of NeptuneBASIC's two collision systems
your game uses, and you choose by how you call `LoadLevelFile`:

**Physics** (`EntityPhysics`, used by default):

```basic
level = LoadLevelFile("level.glb")
```

Solid objects become static physics bodies, using their exact shape.

**Blitz-style collisions** (`Collisions`): give `LoadLevelFile` a collision type.

```basic
Const TYPE_PLAYER = 1, TYPE_LEVEL = 2
level = LoadLevelFile("level.glb", 0, TYPE_LEVEL)
EntityType player, TYPE_PLAYER
EntityRadius player, 0.5
Collisions TYPE_PLAYER, TYPE_LEVEL, 2, 2
```

To use both at once, keep the collision type and also give solid objects the
custom property `physics` = `static`.

## Properties the engine understands

These custom properties do something by themselves. Everything else is
yours to use.

| Property | Value | What it does |
|---|---|---|
| `type` | text | What the object is, for `CountLevelObjects` and `LevelObject` |
| `physics` | `static`, `dynamic` or `kinematic` | Makes a rigid body. Dynamic objects fall and can be pushed |
| `shape` | `auto`, `box`, `sphere`, `capsule`, `cylinder`, `mesh` or `hull` | The body's collision shape (default `auto`) |
| `mass` | number | The body's mass (default 1) |
| `friction` | number | How much the body grips |
| `bounce` | number | How much the body bounces |
| `collision_type` | number | Its `EntityType` for `Collisions` |
| `pick` | 1 sphere, 2 polygon, 3 box | Its `EntityPickMode` |
| `hidden` | 1 | Starts hidden, as with `HideEntity` |
| `prefab` | file name | Places that model here (see below) |
| `intensity` | number | On a light: replaces its brightness (`LightIntensity`) |
| `range` | number | On a point or spot light: replaces its reach (`LightRange`) |
| `shadows` | 0 | On a light: casts no shadows |

## Prefabs: placing other model files

A level often uses the same tree or barrel many times. Add an Empty where it
should stand, and give the Empty a `prefab` property naming the file:

```
prefab = Tree.glb
```

The file is looked for beside the level first and then from the game folder.
The Empty's position, rotation and scale place the model. Every copy shares
the same file in memory, and `neptune build` packs the files with your game.
The Empty becomes the model, so `Animate` and `FindChild` work on it. Custom
properties inside the prefab's own file are not read.

## Lights

Lights keep their colour, cone and direction. A sun's strength carries over.
Point and spot lights fade over a set distance in NeptuneBASIC, where in
Blender they fade forever, so their power in watts sets how far they reach: a
100 W lamp reaches about 7 units, and a 1000 W lamp about 22. Use the
`range` and `intensity` properties if you want a lamp to differ from that.

`FindChild` and `LevelObject` give you a real light entity, so `LightColor`,
`LightRange` and the other light commands work on it.

## Cameras

Every camera that is on draws a picture, so cameras from a level start off.
Switch one on when you want it, such as for a cut-scene:

```basic
overview = FindChild(level, "Overview")
CameraProjMode overview, 1
```

Use 2 for an orthographic camera. A level camera keeps its lens and its
range. `CameraProjMode camera, 0` switches it off again.

## Where things end up

Blender and NeptuneBASIC use different axes. An object at `(x, y, z)` in
Blender is at `(-x, z, -y)` in your program, so Blender's up (Z) is up in
your game. An object that faces Blender's front (its -Y direction) faces its
entity's forward direction.

## Moving, copying and freeing a level

* `FreeEntity level` frees all of it, physics bodies included.
* Physics bodies live in world space, as all physics bodies do, so they do
  not follow if you move the level with `PositionEntity`. To place a level
  somewhere else, make a pivot, position it and pass it as `parent` to
  `LoadLevelFile`, or move the objects in Blender.
* `FindChild(level, name)` finds objects that have become physics bodies.
  `GetChild` does not, because they have left the level's tree.
* Use `LoadLevelFile` once for each level. `CopyEntity` on a level copies the
  model but not its lights, cameras or properties.

## Exporting without the add-on

The add-on only saves clicks. With *File > Export > glTF 2.0*, use these:

| Setting | Value |
|---|---|
| Format | glTF Binary (`.glb`) |
| Include > Data > Custom Properties | on |
| Include > Data > Cameras | on |
| Include > Data > Punctual Lights | on |
| Transform > +Y Up | on |
| Mesh > Apply Modifiers | on |
| Lighting > Lighting Mode | Standard |

`LoadLevelFile` also opens FBX files, as `LoadMesh` does, but they carry no
properties, lights or cameras into your game. Scenes saved by Neptune Level
Editor (`.rgs`) do: see [EDITOR.md](EDITOR.md).

## When something looks wrong

| What you see | Why, and what to do |
|---|---|
| `LoadLevelFile` returns 0 | The console says why. The file must be inside the game's folder |
| Black screen | Nothing is looking: create a camera, or switch on one from the level |
| Metal looks black | Metal shows only what it reflects. Add a `SkyBox` with `EnvironmentLighting`, or a `CreateReflectionProbe` |
| The sky is black | Blender's World is not exported. Use `CameraClsColor` or `SkyBox` |
| Shadows are too dark | Raise `AmbientLight` |
| An object is not solid | Check that its name begins with `col_` or `hit_`, and export again |
| A property is empty | Custom properties were off in the export, or the property is on the mesh data rather than the object |
| A crate falls through the floor | The floor is not a physics body. Don't pass a collision type to `LoadLevelFile`, or give the floor `physics` = `static` |
| A prefab is missing | The console names the file. Paths are relative to the level, or to the game folder |
