# Neptune Level Editor

Neptune Level Editor is a program for building 3D levels: place objects,
lights and cameras, say which objects are solid, and load the result with one
command:

```basic
level = LoadLevelFile("levels/castle.rgs")
```

It is [Fyrox](https://fyrox.rs)'s scene editor in Neptune IDE's colours. This
is an early version: see [what is missing](#what-is-missing) before you rely
on it. Levels made in Blender load the same way and are further along; see
[BLENDER.md](BLENDER.md).

A finished example comes with NeptuneBASIC:

```bash
neptune run examples/editorlevel.nb
```

## Starting it

From Neptune IDE, choose *Program > Open Level Editor*: it opens on the
folder you have open (or the folder of the program you are editing). From a
command line:

```bash
neptune-editor mygame
```

Give it your game's folder (or start it inside that folder). Everything a
level uses must be inside the game folder, as with every file a program
loads. The editor keeps `settings.ron`, `neptune-editor.log` and a `data`
folder there.

It opens on a starter level: a solid ground, a sun and a place to start.
Change it, press Play to walk about in it, and save it with *File > Save
Scene* as a `.rgs` file. To open a level you saved, name it too:
`neptune-editor mygame levels/castle.rgs`.

The panels are arranged as in Blender. The 3D view (**Level**) fills the left
of the window. The list of objects (**Objects**) is at the top right, with
the **Neptune** panel and the **Inspector** (the selected object's settings
and properties) under it, and the asset browser, the log and the undo
history are tabs under the view.

The Inspector shows what a level needs: an object's name, where it is, how
it is turned and how big it is, whether it is seen and casts shadows, and
its properties. The engine's own settings are left out; start the editor
with `--full-inspector` to see them all. Drag a panel's title to
move it; *View > Reset Layout* puts everything back.

## The Neptune panel

The Neptune panel sets what `LoadLevelFile` reads from the selected objects,
so you do not have to type names and property names:

| Row | What it sets |
|---|---|
| Solid | **Solid** (seen and collided with), **Invisible** (collided with, never seen) or **Not Solid**. It puts `col_` or `hit_` in front of the name, or takes it off |
| Type | The `type` property, which your program lists with `CountLevelObjects` and `LevelObject` |
| Physics | None, Static, Dynamic or Kinematic (the `physics` property). None also takes out Shape and Mass |
| Shape | The body's shape: Auto, Box, Sphere, Capsule, Cylinder, Mesh or Hull |
| Mass | The body's mass; empty is 1 |
| Prefab | A model to place at the object, as with Blender levels |

A change goes to every selected object, and Ctrl+Z undoes it. Type a value
and press Enter, or click somewhere else.

## Quad view

*View > Quad View* (Ctrl+Alt+Q) splits the 3D view in four, as Hammer and
UnrealEd do: the 3D view at the top left, the level seen from the top at the
top right, from the front at the bottom left and from the side at the
bottom right. The three flat views have no perspective, so lining things up
along an axis is easy, and each has a grid in its own plane.

Every tool works in the view under the mouse: select, move, turn and scale
there, scroll to zoom, and drag with the right mouse button to move a flat
view (or look around in the 3D view).

| Key | What it does |
|---|---|
| Ctrl+Alt+Q | Quad view on or off; turning it off keeps the view the mouse is over |
| Ctrl+Space | In quad view, the view under the mouse fills the window, and back |
| Numpad 7, 1, 3 | The top, front or side view on its own |
| Numpad 5 | The 3D view on its own |

These are Blender's keys; Ctrl+Space is also TrenchBroom's. Start the editor
with `--quad` to open in quad view.

## Blocking out

Rough out a level with plain boxes first, as in Hammer or TrenchBroom, and
replace them with models later.

* **The block tool** (Shift+B, or the cube button on the toolbar): drag in
  any view to draw a box. It snaps to the grid, and when you let go it
  becomes a grey block named `col_Block1`, `col_Block2` and so on, so your
  game treats it as solid (the Neptune panel changes that). In the 3D view
  the box lies on the ground and is as high as the last block; in the flat
  views of the quad view you draw two sides and the third comes from the
  last block, so walls drawn from the top all have the same height. The
  first block is 2 high. Ctrl+Z takes a block away again.
* **The grid:** `[` makes it twice as fine and `]` twice as coarse (from
  1/16 to 64). The size shows in the corner of the 3D view. The magnet button
  above the view turns snapping on and off; a new game folder starts with a
  grid of 0.5, snapped.
* **Nudging:** the arrow keys move the selected objects one grid step
  across the view under the mouse (left, right, up and down as you see
  them), and Page Up and Page Down one step towards or away from you (up and
  down in the 3D view). Ctrl+Z undoes it.

## Keys

The tools are on Blender's keys:

| Key | What it does |
|---|---|
| G, R, S | Move, rotate and scale the selected object |
| W | The select tool |
| X | Delete the selection |
| F | Bring the selection into view |
| Ctrl+Z, Ctrl+Shift+Z | Undo and redo |
| Ctrl+S | Save the scene |
| F5 | Play |
| Right mouse button | Look around |
| Arrow keys, Page Up, Page Down | Nudge the selection one grid step; with the right mouse button held, move the camera |
| Shift+B | The block tool |
| `[`, `]` | Grid finer, coarser |

The camera moves on the arrow keys while you hold the right mouse button,
not on W A S D, so that S can be scale.
Every key can be changed in the editor's settings, and keys you change are
kept. Orbiting with the middle mouse button, as Blender does, is not there.

## Play

The green Play button saves the level and runs it with `neptune`; the red
button stops it. The list beside the button chooses what runs:

| Choice | What it does |
|---|---|
| **Run main.nb** | Runs your game. It is offered when the game folder has a `main.nb`, or only one program. Your program is given `--override-scene` and the level's file name, which it can read with `CommandLine()` to start in that level |
| **Look Around** | Needs no program: flies through the level with W A S D and the mouse (Space and Ctrl go up and down, Shift is faster), starting at an object with `type` = `spawn` if there is one |

The program is checked first, so a mistake in it shows in the editor's log
and nothing runs. `neptune` must be beside `neptune-editor` or on your path.

`neptune-editor mygame levels/castle.rgs --play` opens a level and presses
Play at once.

## What comes across

`LoadLevelFile` treats a scene from the editor like a level from Blender.
The rules are the same ones [BLENDER.md](BLENDER.md) explains in full.

| In the editor | In your game |
|---|---|
| Meshes and their materials | Drawn as they are in the editor |
| Parent and child objects | The same parents and children |
| Directional, point and spot lights | Real lights, with every setting the editor gave them: `LightColor`, `LightRange` and the other light commands work on them |
| Cameras | Real cameras, switched off until you ask for one with `CameraProjMode camera, 1` |
| Properties (the *Properties* list in the Inspector) | Values your program reads with `EntityProperty`, `EntityPropertyInt` and `EntityPropertyFloat` |
| A name starting with `col_` | Solid and visible |
| A name starting with `hit_` | Solid and never drawn |

Find objects with `FindChild(level, "Name")`, or give them a `type` property
and list them:

```basic
For i = 1 To CountLevelObjects(level, "coin")
    coin = LevelObject(level, i, "coin")
    value = EntityPropertyInt(coin, "value", 1)
Next
```

The properties the engine understands are the same as for Blender levels:
`type`, `physics`, `shape`, `mass`, `friction`, `bounce`, `collision_type`,
`pick`, `hidden`, `prefab`, and on lights `intensity`, `range` and `shadows`.

A model placed in the scene counts as one object. Properties and `col_`
names inside the model's own file are not read.

## Where things end up

A level looks the same in your game as in the editor. The numbers differ in
one way: an object at `(x, y, z)` in the editor is at `(-x, y, z)` in your
program, because the editor's X axis points the other way.

## What is missing

* **Models and textures from files are not tested.** The example level is
  made of shapes stored in the scene itself. A scene that uses model or
  texture files depends on the editor's `data/resources.registry` file, and
  `neptune build` does not pack that file yet.
* **Physics bodies made in the editor** (its Rigid Body and Collider objects)
  are not connected to `EntityPhysics`. Use `col_` names and the `physics`
  property instead.
* The editor still shows a few things that mean nothing for BASIC programs,
  such as the script editor setting. (The export window and the engine's
  built-in fonts and shapes in the asset browser are hidden;
  `--full-inspector` lists the built-in ones again.)
* **Quad view:** the flat views show the sky behind the level, and a ground
  whose top is exactly at height 0 hides the grid in the 3D view.
* **Blocking out:** blocks are boxes only, with no cutting (CSG), editing of
  faces or edges, or textures that keep their size as a block grows.
