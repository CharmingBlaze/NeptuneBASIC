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

```bash
neptune-editor mygame
```

Give it your game's folder (or start it inside that folder). Everything a
level uses must be inside the game folder, as with every file a program
loads. The editor keeps `settings.ron`, `neptune-editor.log` and a `data`
folder there.

Choose *File > New Scene*, build the level, and save it with *File > Save
Scene* as a `.rgs` file.

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

* **No Neptune panel yet.** Solid objects and properties are set by typing
  names and property values, not with buttons as in the Blender add-on.
* **Models and textures from files are not tested.** The example level is
  made of shapes stored in the scene itself. A scene that uses model or
  texture files depends on the editor's `data/resources.registry` file, and
  `neptune build` does not pack that file yet.
* **Physics bodies made in the editor** (its Rigid Body and Collider objects)
  are not connected to `EntityPhysics`. Use `col_` names and the `physics`
  property instead.
* The editor still shows tools that mean nothing for BASIC programs, such as
  its script editors and export window.
