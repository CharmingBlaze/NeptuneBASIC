# Making 2D levels and sprites in other tools

You can draw a 2D level in a map editor and animate characters in
[Aseprite](https://www.aseprite.org) or TexturePacker, then load the results
with one command each. These map editors can be loaded:

| Editor | What to load |
|---|---|
| [Tiled](https://www.mapeditor.org) | `.tmx`, `.tmj` or `.world` |
| [LDtk](https://ldtk.io) | `.ldtk` |
| [Ogmo Editor 3](https://ogmo-editor-3.github.io) | a level's `.json`, with the `.ogmo` project in that folder or above |
| [Sprite Fusion](https://www.spritefusion.com) | `map.json` from the JSON export, next to `spritesheet.png` |
| [Pyxel Edit](https://pyxeledit.com) | the tilemap exported as JSON or XML, next to the tileset exported as a PNG of the same name |
| [Aseprite](https://www.aseprite.org) | a `.aseprite` file with tilemap layers |

Any other editor that can export Tiled's `.tmx` format works too.

Complete examples come with NeptuneBASIC:

```bash
neptune run examples/tiledmap.nb
```

```bash
neptune run examples/mapeditors.nb
```

The maps are in `examples/media/tiled`. Open `meadow.tmx` in Tiled, change
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

`LoadTiledMap` reads Tiled's own files, `.tmx` and `.tmj`, and any tilesets,
templates and pictures they use. `DrawTiledMap` draws every visible layer in
order, with its opacity, tint and parallax, and follows `Camera2D` like other
2D drawing, so a large map can scroll. Animated tiles play by themselves.
`LoadTiledMap` returns 0 if the map cannot be loaded, and the console says why.

Every orientation, tile layer format and map size in Tiled can be used, except
the Zstandard layer format. Tilesets can be one image or separate images, and
tiles can be any size. Very large tiles use a lot of memory, because every tile
in a map is stored at the size of its largest.

### Isometric and hexagonal maps

These draw and load like any other map. Cells overlap on these grids, so use
`TileMapPickX` and `TileMapPickY`, which take both coordinates, to find the
cell under a point, and `TileMapCellCenterX` and `TileMapCellCenterY` to go
the other way:

```basic
ground = TiledLayer(map, "Ground")
cx = TileMapPickX(ground, mouseWorldX#, mouseWorldY#)
cy = TileMapPickY(ground, mouseWorldX#, mouseWorldY#)
```

On an isometric map, `TiledObjectX` and `TiledObjectY` are converted to
positions on the screen, and a rectangle's four corners are given as its
points, because it is a diamond once drawn.

### Levels in one file

A Tiled `.world` file, like an LDtk project, holds several levels. Pass a
level's name as a second value, or leave it out for the first:

```basic
map = LoadTiledMap("world.world", "cave")
For i = 1 To CountTiledLevels(map)
    Print TiledLevelName(map, i) + " is at " + TiledLevelX(map, i) + ", " + TiledLevelY(map, i)
Next
```

In a `.world` file, the name is the map's file name without its extension.

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
in one, and keep the group's offset, visibility, opacity, tint and parallax.

Picture layers (*Image Layer* in Tiled) are drawn in their place among the
others, and repeat if *Repeat X* or *Repeat Y* is on. `TiledImageLayer(map,
"Sky")` gives the picture as an image. Parallax follows Tiled: a layer's speed
is measured from the middle of the view, so use `Camera2D` to scroll.

`TiledLayerAlpha map, "Fog", 0.5` changes how solid a layer is, and
`TiledLayerProperty` reads a layer's custom properties.

### Tiles with their own data

Give a tile a *Type* or *Custom Properties* in the tileset, and read them from
the frame number that `GetTile` returns:

```basic
t = GetTile(ground, cx, cy)
If t >= 0 And TiledTileType(map, t) = "lava" Then hurt = TiledTileProperty(map, t, "damage", "1")
```

Shapes drawn in Tiled's *Tile Collision Editor* can become physics:

```basic
walls = TiledMapBody2D(map, "Ground")
```

Only tiles with shapes are solid. Leave out the layer's name to use every
layer. `TileMapBody2D` is the other way, and makes every tile of a layer solid.

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
| `TiledObjectShape` | 0 rectangle, 1 ellipse, 2 point, 3 polygon, 4 polyline, 5 tile, 6 text |
| `TiledObjectTile` | For a tile object, its frame in `TiledMapImage(map)`; otherwise -1 |
| `TiledObjectText` | The words of a text object. `TiledObjectProperty(obj, "text.color")` and the like give its look |
| `CountTiledObjectPoints`, `TiledObjectPointX`, `TiledObjectPointY` | Corners of a polygon or polyline, in map pixels: enemy paths, for example |
| `TiledObjectProperty`, `TiledObjectPropertyInt`, `TiledObjectPropertyFloat` | A custom property as text or a number, or a value you give if it is missing |
| `TiledObjectHasProperty` | 1 if it has that property |

Objects are data for your program, and `DrawTiledMap` does not draw them.
Objects made from a template work like any other. To show a tile object at its
place, size and rotation:

```basic
DrawTiledObject coin
```

`TiledMapProperty(map, "title")` reads a custom property of the map itself, and
`TiledMapBackground` gives its background colour. `TiledMapWidth`,
`TiledMapHeight`, `TiledMapTileWidth`, `TiledMapTileHeight`,
`TiledMapPixelWidth`, `TiledMapPixelHeight` and `TiledMapOrientation` describe
it. `FreeTiledMap` frees the map with its layers, objects and pictures.

### What is not read

Tiles drawn flipped or rotated in Tiled appear correctly. These are not
supported or are ignored:

* Layers saved with Zstandard compression: use CSV, or Base64 with zlib or gzip
  (an error says so).
* `.world` files that only have patterns: list the maps (an error says so).
* Property types defined in a Tiled project, whose defaults are not filled in.
* Hexagonal tiles rotated by 60 degrees, which draw unrotated.

## Maps from other editors

Everything above applies to these too: the same commands load, draw and read
them. Here is how each editor's parts arrive.

### LDtk

Save the project and load its `.ldtk` file. *Save levels separately* is fine.
Pass a level's name to choose it, as with a `.world` file above.

```basic
map = LoadTiledMap("game.ldtk", "Level_0")
```

| In LDtk | In NeptuneBASIC |
|---|---|
| Tiles or auto layer | A tile layer of the same name. Cells holding several tiles add layers called `Name 2`, `Name 3`, and `ShowTiledLayer` with `Name` reaches them all |
| IntGrid layer | A layer of its values, not drawn: `GetTile` gives the value and `TileMapBody2D` makes the cells solid. Its auto tiles are the layer called `Name tiles` |
| Entity | An object: `TiledObjectType` is the entity's name, and `TiledObjectName` is its `name` field if it has one |
| Field | A custom property. A Point becomes `x,y` in pixels, at the middle of its cell |
| First Array of Points on an entity | `CountTiledObjectPoints` and `TiledObjectPointX/Y` |
| Level fields | `TiledMapProperty` |
| Tile custom data and enums | `TiledTileProperty(map, t, "data")` and `TiledTileType` |

An entity showing a tile larger than one cell shows only its first cell.
Level background pictures are not read.

### Ogmo Editor 3

Load the level's `.json`. The `.ogmo` project must be in the same folder or one
above it, because it says where the tilesets are.

| In Ogmo | In NeptuneBASIC |
|---|---|
| Tile layer | A tile layer of the same name |
| Grid layer | A layer of values, not drawn, as for IntGrid above. Letters give their character code |
| Entity | An object. Its values are custom properties and its nodes are its points |
| Decal | An object of type `decal` named after its picture. The `texture`, `originX`, `originY`, `scaleX` and `scaleY` properties describe it, and you draw it yourself |

### Sprite Fusion

Choose *JSON* when exporting, unzip it, and load `map.json` with
`spritesheet.png` beside it. A layer marked as a collider has the property
`collider` set to 1, so `TiledLayerProperty(map, "Walls", "collider")` finds it.

### Pyxel Edit

Export the tilemap as JSON or XML, and export the tileset as a PNG with the
same name next to it, such as `level.json` and `level.png`. The names
`level_tileset.png` and `tileset.png` are also found.

### Aseprite tilemaps

Load the `.aseprite` file itself. Each tilemap layer of the first frame becomes
a tile layer, and other layers are skipped. To use the same file's sprites, also
export a sheet as described below.

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
and packs what they use along with them: tilesets, templates, level files,
projects and pictures. Decal pictures named by Ogmo levels are not packed
unless your program names them too.

## When something looks wrong

| What you see | Why, and what to do |
|---|---|
| `LoadTiledMap` or `LoadSheetImage` returns 0 | The console says why, usually a file that isn't where the map or sheet says |
| "Zstandard layers are not supported" | In Tiled: *Map > Map Properties > Tile Layer Format*, then pick CSV or Base64 with zlib or gzip |
| "no .ogmo project found" | Keep the level in the project's folder or below it |
| "no level called" | Check the spelling with `TiledLevelName`, or leave the name out |
| "export the tileset from Pyxel Edit" | The map needs a PNG of the tileset next to it, with the same name |
| LDtk layers appear twice in `CountTiledLayers` | A cell holds several tiles there, and each extra tile needs another layer. This is normal |
| Black squares around sprites | The picture has no transparency. Export it with an alpha channel |
| The map is cut off when scrolling | Use `Camera2D` to scroll. If you pass your own position to `DrawTiledMap`, it must place the map on screen |
| A character sinks into the ground or floats | Check its `MidHandle` and the size of its physics shape |
| "has no tag" | Tags are missing from the export (*Meta: Tags*), or the name is different |
