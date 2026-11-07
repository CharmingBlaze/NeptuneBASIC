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

[Contents](../MANUAL.md) · Previous: [24. User interfaces](24-gui.md)
