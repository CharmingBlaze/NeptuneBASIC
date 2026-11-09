# 25. Sharing your game

[Contents](../MANUAL.md) · Previous: [24. User interfaces](24-gui.md)

Your game is finished, or at least ready for friends to try. This chapter
covers making a version people can run without installing NeptuneBASIC,
making it run well on all kinds of computers, and where to go next.

- [Making a stand-alone game](#making-a-stand-alone-game)
- [Graphics quality](#graphics-quality)
- [Windows, Linux and macOS](#windows-linux-and-macos)
- [Plugins](#plugins)
- [Where to go next](#where-to-go-next)
- [Try it yourself](#try-it-yourself)

## Making a stand-alone game

```
neptune build mygame.nb
```

makes a folder, `dist/mygame/`, with everything the game needs: an
executable (`mygame.exe` on Windows) with your program inside it, and the
media files (pictures, models, sounds) your program uses, in the same
folders as before. Zip the folder and share it; players unzip it and run
the executable.

| Option | Does |
|---|---|
| `-o folder` | puts the game in another folder |
| `--console` | keeps a console window open next to the game on Windows (for `Print` and `DebugLog` output while testing) |

Some things to know:

- Build NeptuneBASIC itself with `cargo build --release` first, so the
  game runs at full speed.
- Media files are found from the file names written in quotes in your
  program (`LoadImage("gfx/ship.png")`). A file name put together while
  the program runs (`LoadImage("level" + n + ".png")`) can't be found
  that way: copy those files into the game folder yourself.
- In the built game, file names are relative to the game's folder, as
  they were to the folder you ran `neptune` from.
- If the game stops with an error, the message is shown in a message box.

## Graphics quality

Not everyone has a fast graphics card. NeptuneBASIC runs on any card with
OpenGL 3.3 (roughly 2010 onwards), and quality presets turn the expensive
effects off on older machines:

```basic
Graphics3D 1280, 720
GraphicsQuality QUALITY_LOW          ; QUALITY_LOW, QUALITY_MEDIUM, QUALITY_HIGH, QUALITY_ULTRA
Print GetGraphicsQuality()
```

A settings menu can offer the levels to the player. The quality can also
be chosen when starting the game, which is handy for testing:

```
neptune run --quality low mygame.nb
```

| Level | For |
|---|---|
| low | old laptops: the expensive effects off |
| medium | most computers |
| high | good graphics cards |
| ultra | everything on |

`PostEffect` turns single effects (bloom, ambient occlusion,
anti-aliasing, shadows, light shafts) on and off, and the
[shader manual](../shaders/07-shadows.md) has the shadow settings.
`neptune run --uncapped` runs a game as fast as the machine allows,
without waiting for the screen, to measure its speed.

## Windows, Linux and macOS

The same program runs on all three, with the same commands. To make
sure yours does too:

- Use `/` in file paths (`"gfx/ship.png"`), which works everywhere.
- Linux and macOS care about capital letters in file names: `Ship.png`
  and `ship.png` are different files there. Write names exactly as they
  are.
- Build the game on each system you want to share it for:
  `neptune build` makes a game for the system it runs on.

## Plugins

Plugins add new commands to NeptuneBASIC, written in Rust, C or C++, for
when a game needs something the built-in commands don't have (a special
file format, a device, a library). A plugin is a shared library placed
next to `neptune`; its commands then work like any other. `neptune
plugins` lists the plugins found and their commands. [PLUGINS.md](../PLUGINS.md)
explains how to write one.

## Where to go next

- **Read the example games.** `examples/games/starcourier.nb` (2D) and
  `examples/games/orbrun.nb` (3D) are complete, commented games.
- **Make something small and finish it.** A finished small game teaches
  more than an unfinished big one: a Pong, a Breakout, a maze.
- **Look things up.** The [command reference](../COMMANDS.md) has every
  command with an example, and the [language summary](../LANGUAGE.md) the
  whole language on one page.
- **Make it look good.** [The shader manual](../SHADERS.md) covers
  realistic and stylized materials, screen effects and shadows.

## Try it yourself

You cannot share a game that does not run well, so these exercises are about
preparing one. (`neptune build` itself is a command you type, not a program.)

1. **Quality menu.** Show a spinning cube on a floor, and let the player choose
   the graphics quality with the keys 1 (low), 2 (medium), 3 (high) and 4
   (ultra). The screen says which quality is in use.
   *Hint: `GraphicsQuality` takes `QUALITY_LOW` to `QUALITY_ULTRA`, and `GetGraphicsQuality()` tells you which one is set.*
2. **Asset finder.** A game's pictures might be in the game's folder, in `gfx`,
   or in `media`, and you want to write the program so that it looks in all
   three places and uses `/` in every path, so it works on Windows, Linux and
   macOS. Write a function `FindAsset$(root$, name$)` that returns the path of
   the first folder where the file exists, or `"(missing)"`. Test it with a
   pretend `ship.png` in a `gfx` folder, and with a file that is nowhere.
   *Hint: `FileType(path)` is 1 for a file, and `CreateDir` and `WriteFile` make the test files.*
3. **Keeping up.** Draw a grid of 100 spinning cubes and show the frame rate.
   If the game runs slower than 30 frames a second for a whole second, lower the
   quality by one level, so that a slow computer gets a smooth game after a few
   seconds, without the player doing anything.
   *Hint: count the frames, and look at the count every time `MilliSecs()` has moved on by 1000.*

### Answers

1.

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 3, -8
CameraLookAt camera, 0, 0, 0
light = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity light, 50, 30, 0

floor = CreatePlane()
EntityColor floor, 70, 130, 90
cube = CreateCube()
PositionEntity cube, 0, 1, 0
EntityColor cube, 240, 160, 60

Dim names$(3)
names(0) = "Low" : names(1) = "Medium" : names(2) = "High" : names(3) = "Ultra"

While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_1) Then GraphicsQuality QUALITY_LOW
    If KeyHit(KEY_2) Then GraphicsQuality QUALITY_MEDIUM
    If KeyHit(KEY_3) Then GraphicsQuality QUALITY_HIGH
    If KeyHit(KEY_4) Then GraphicsQuality QUALITY_ULTRA

    TurnEntity cube, 0.5, 1, 0
    RenderWorld
    Text 10, 10, "Quality: " + names(GetGraphicsQuality())
    Text 10, 30, "Keys 1 to 4 change it, Escape quits"
    Flip
Wend
```

The levels are numbered from 0, so `GetGraphicsQuality()` can be used straight
as the position in the `names` array. Each key sets one level, which takes
effect at once. A real game would put the same choice on its settings screen.

2.

```basic
Function FindAsset$(root$, name$)
    Local folders$[2]
    folders[0] = ""
    folders[1] = "gfx/"
    folders[2] = "media/"
    For i = 0 To 2
        If FileType(root + folders[i] + name) = 1 Then Return folders[i] + name
    Next
    Return "(missing)"
End Function

root$ = SystemProperty("tempdir") + "assetdemo/"
CreateDir root
CreateDir root + "gfx"
out = WriteFile(root + "gfx/ship.png")
WriteLine out, "pretend picture"
CloseFile out

Print "ship.png: " + FindAsset(root, "ship.png")
Print "boss.png: " + FindAsset(root, "boss.png")

DeleteFile root + "gfx/ship.png"
DeleteDir root + "gfx"
DeleteDir root
```

```output
ship.png: gfx/ship.png
boss.png: (missing)
```

The function tries each folder in turn with `FileType`, which is 1 only for a
real file, and returns as soon as it finds one. All the paths use `/`, which
every system understands. The test program builds its own `gfx` folder inside
the temporary folder with a stand-in file, finds that, fails to find the other,
and then removes everything it made.

3.

```basic
Graphics3D 1280, 720
camera = CreateCamera()
PositionEntity camera, 0, 14, -22
CameraLookAt camera, 0, 0, 0
light = CreateLight(LIGHT_DIRECTIONAL)
TurnEntity light, 50, 30, 0

Dim cubes(99)
For i = 0 To 99
    cubes(i) = CreateCube()
    ScaleEntity cubes(i), 0.6, 0.6, 0.6
    PositionEntity cubes(i), (i Mod 10) * 2.4 - 11, 0, (i / 10) * 2.4 - 11
    EntityColor cubes(i), 60 + (i Mod 10) * 18, 120, 255 - (i / 10) * 20
Next

GraphicsQuality QUALITY_ULTRA
frames = 0
fps = 60
mark = MilliSecs()

While Not KeyHit(KEY_ESCAPE)
    For i = 0 To 99
        TurnEntity cubes(i), 0.5, 1, 0
    Next

    frames = frames + 1
    If MilliSecs() - mark >= 1000
        fps = frames
        frames = 0
        mark = MilliSecs()
        If fps < 30 And GetGraphicsQuality() > QUALITY_LOW
            GraphicsQuality GetGraphicsQuality() - 1
        EndIf
    EndIf

    RenderWorld
    Text 10, 10, "FPS: " + fps + "   Quality level: " + GetGraphicsQuality()
    Flip
Wend
```

Every frame adds one to `frames`. Once a second has passed, that count is the
frame rate, so it is shown, reset, and checked. If the game is too slow and
there is still a lower level to go to, the quality drops by one; a second
later the test happens again, so a very slow computer steps down more than
once. On a fast computer nothing changes. The grid of cubes comes from
`i Mod 10` (the column) and `i / 10` (the row, since whole-number division
drops the fraction).

[Contents](../MANUAL.md) · Previous: [24. User interfaces](24-gui.md)
