# Making 2D levels and sprites in other tools

You can draw a 2D level in [Tiled](https://www.mapeditor.org) and animate
characters in [Aseprite](https://www.aseprite.org) or TexturePacker, then load
the results with one command each. Tiled is free, and all three run on
Windows, Linux and macOS.

A complete example comes with NeptuneBASIC:

```bash
neptune run examples/tiledmap.nb
```

Its map is `examples/media/tiled/meadow.tmx`. Open it in Tiled, change
something, save and run the example again.

## Maps from Tiled

```basic
Graphics 960, 480, 0, 2
map = LoadTiledMap("levels/meadow.tmx")

While Not KeyHit(1)
    Cls
    DrawTiledMap map
    Flip
Wend
```

`LoadTiledMap` reads Tiled's own files, `.tmx` and `.tmj`, and any tilesets and
pictures they use. `DrawTiledMap` draws every visible layer in order, and
follows `Camera2D` like other 2D drawing, so a large map can scroll.
`LoadTiledMap` returns 0 if the map cannot be loaded, and the console says why.

### Setting up a map in Tiled

When you create a map, choose these:

| Setting | Value |
|---|---|
| Orientation | Orthogonal |
| Tile layer format | CSV |
| Map size | Fixed, not Infinite |

Tilesets must be *Based on Tileset Image*, with tiles the same size as the
map's tiles. If a map uses something NeptuneBASIC can't read, the console
says what to change.

### Layers

Every tile layer is an ordinary tile map, so all the tile map commands work
on it. Get one by its name:

```basic
ground = TiledLayer(map, "Ground")
TileMapBody2D ground              ; make its tiles solid for 2D physics
t = GetTile(ground, 4, 10)        ; which tile is at column 4, row 10
SetTile ground, 4, 10, -1         ; dig it out
```

To draw layers at different speeds for a parallax background, call
`DrawTileMap` on each layer yourself with its own position.

`ShowTiledLayer map, "Secret", False` hides a layer from `DrawTiledMap`, and
`True` shows it again. Layers hidden in Tiled start hidden. `CountTiledLayers`
and `TiledLayerName` list them. Layers in a group work as if they were not
in one, and keep the group's offset and visibility.

### Objects

Add an *Object Layer* in Tiled to mark places and areas: the start, enemies,
coins, doors, danger zones. Give objects a **Name** and a **Type** (called
*Class* in some versions of Tiled), and add *Custom Properties* for anything
else. Then find them:

```basic
For i = 1 To CountTiledObjects(map, "coin")
    coin = TiledObject(map, i, "coin")
    x# = TiledObjectX(coin)
    y# = TiledObjectY(coin)
    value = TiledObjectPropertyInt(coin, "value", 10)
Next

door = FindTiledObject(map, "CastleDoor")
```

Numbering starts at 1. Leave out the type to go through every object.
`TiledObjectX` and `TiledObjectY` give an object's top-left corner in pixels
from the top-left of the map. That is true for tile objects too, even
though Tiled itself places those by their bottom-left corner.

| Command | Gives |
|---|---|
| `TiledObjectName`, `TiledObjectType`, `TiledObjectLayer` | Its name, type and the layer it is on |
| `TiledObjectX`, `TiledObjectY`, `TiledObjectWidth`, `TiledObjectHeight` | Where it is and its size |
| `TiledObjectRotation` | Its rotation, in degrees clockwise |
| `TiledObjectShape` | 0 rectangle, 1 ellipse, 2 point, 3 polygon, 4 polyline, 5 tile |
| `TiledObjectTile` | For a tile object, its frame in `TiledMapImage(map)`; otherwise -1 |
| `CountTiledObjectPoints`, `TiledObjectPointX`, `TiledObjectPointY` | Corners of a polygon or polyline, in map pixels: enemy paths, for example |
| `TiledObjectProperty`, `TiledObjectPropertyInt`, `TiledObjectPropertyFloat` | A custom property as text or a number, or a value you give if it is missing |
| `TiledObjectHasProperty` | 1 if it has that property |

Objects are data for your program, and `DrawTiledMap` does not draw them. To
show a tile object:

```basic
DrawImage TiledMapImage(map), TiledObjectX(coin), TiledObjectY(coin), TiledObjectTile(coin)
```

`TiledMapProperty(map, "title")` reads a custom property of the map itself.
`FreeTiledMap` frees the map with its layers, objects and picture.

### What is not read

Tiles drawn flipped or rotated in Tiled appear correctly. These are not
supported or are ignored:

* Isometric, staggered and hexagonal maps, and infinite maps (an error says so).
* Compressed or Base64 layers: use CSV (an error says so).
* Tilesets made of separate images, and tile sizes different from the map's (an error says so).
* Layer opacity, tint and parallax values, image layers, animated tiles and
  collision shapes drawn on tiles. These are ignored. Use a solid layer with
  `TileMapBody2D`, or objects, for collisions.

## Sprite sheets from Aseprite and TexturePacker

```basic
hero = LoadSheetImage("art/hero.json")
MidHandle hero
anim = CreateAnim2D(hero)
Anim2DPlayTag anim, "run"
```

You load the `.json` file, and it names its picture, which should be kept
next to it. The result is an ordinary animation image, so `DrawImage`,
`DrawImageEx`, `ImagesCollide` and the others all work on it, and every frame
is the same size even if the tool trimmed or packed them tightly.

**From Aseprite:** use *File > Export Sprite Sheet*. On the *Output* tab turn
on *JSON Data*, and also *Meta: Tags* to include your tags.

**From TexturePacker:** choose the *JSON (Hash)* or *JSON (Array)* format and
turn off *Allow rotation*. TexturePacker has no tags, so play frame ranges
with `CreateAnim2D` and `Anim2DPlay`.

### Tags

Tags are named stretches of the timeline in Aseprite, such as `idle` or
`run`. `Anim2DPlayTag` plays one with Aseprite's own timing for each frame and
its direction (forward, reverse or ping-pong). If that tag is already playing
it does nothing, so you can simply ask for the right one every frame:

```basic
If Not onground
    Anim2DPlayTag anim, "jump"
ElseIf moving
    Anim2DPlayTag anim, "run"
Else
    Anim2DPlayTag anim, "idle"
End If
DrawImage hero, x, y, Anim2DFrame(anim)
```

Pass a third value to choose the mode yourself: 1 loops, 2 ping-pongs and 3
plays once, which suits an attack or a death. `Anim2DTag` tells you which tag
is playing, and `Anim2DPlaying` tells you when a one-shot has finished.

`CountSheetTags`, `SheetTagName`, `SheetTagFirst` and `SheetTagLast` read the
tags, and `SheetFrameTime` gives a frame's time in milliseconds.

## Sharing your game

`neptune build` finds your maps and sheets from the file names in your program
and packs the tilesets and pictures they use along with them.

## When something looks wrong

| What you see | Why, and what to do |
|---|---|
| `LoadTiledMap` or `LoadSheetImage` returns 0 | The console says why, usually a file that isn't where the map or sheet says |
| "set Tile Layer Format to CSV" | In Tiled: *Map > Map Properties > Tile Layer Format* |
| Black squares around sprites | The picture has no transparency. Export it with an alpha channel |
| The map is cut off when scrolling | Use `Camera2D` to scroll. If you pass your own position to `DrawTiledMap`, it must place the map on screen |
| A character sinks into the ground or floats | Check its `MidHandle` and the size of its physics shape |
| "has no tag" | Tags are missing from the export (*Meta: Tags*), or the name is different |
