# 24. User interfaces

[Contents](../MANUAL.md) · Previous: [23. Networking](23-networking.md) · Next: [25. Sharing your game](25-sharing.md)

Menus, settings screens, inventories, level editors and tools need
windows, buttons, lists and text boxes. The `Gui...` commands make
them. (For a quick button or slider drawn each frame, the `UI...`
commands in [chapter 14](14-2d-games.md#simple-buttons-and-sliders) are
simpler.)

- [How the GUI works](#how-the-gui-works)
- [Windows and layout](#windows-and-layout)
- [Widgets](#widgets)
- [Reading what the user did](#reading-what-the-user-did)
- [Lists, menus and tabs](#lists-menus-and-tabs)
- [Dialogs](#dialogs)
- [Style](#style)
- [Tools for editors](#tools-for-editors)

## How the GUI works

GUI widgets are made **once**, not every frame. The engine draws them
and handles the mouse and keyboard; each frame, the program only asks
what the user did:

```basic
Graphics3D 1280, 720
win = GuiWindow("Options", 20, 20, 300, 260)
name = GuiTextBox("Player", win)
volume = GuiSlider(0, 100, 80, win)
fullscreen = GuiCheckBox("Fullscreen", 0, win)
ok = GuiButton("OK", win)

While Not KeyHit(KEY_ESCAPE)
    If GuiClicked(ok)
        Print "Name: " + GuiText(name)
        Print "Volume: " + GuiValue(volume)
        SetFullscreen GuiValue(fullscreen)
    EndIf
    RenderWorld
    Flip
Wend
```

Every widget command returns a handle and takes a **parent** as its last
argument: the window, panel or tab it goes in. Parent 0 (or leaving it
out) puts it on the screen.

## Windows and layout

| Command | Makes |
|---|---|
| `GuiWindow(title$, x, y, w, h [, parent])` | a window that can be dragged and closed |
| `GuiPanel([layout, parent])` | a panel that lays out what is put in it: 0 a column, 1 a row, 2 wrapping rows, 3 free |
| `GuiGrid(columns, rows [, parent])`, `GuiCell w, column, row` | a grid of equal cells |
| `GuiScroll([parent])` | an area that scrolls when its contents don't fit |
| `GuiBorder([thickness, parent])` | a box with a border |
| `GuiTabs([parent])`, `GuiTab(tabs, title$)` | pages with tabs |

Widgets inside a window or panel are placed for you, one under the other.
To adjust them: `SetGuiSize w, width, height`, `GuiMargin` (space round
it), `GuiAlign` (where in its space), `SetGuiPosition` (in a free panel),
`GuiCenter` (in the middle of its parent), `GuiVisible` and `GuiEnable`.

Sizes are in **logical pixels**: on a high-resolution screen, the GUI is
scaled up so it stays readable.

## Widgets

| Command | Makes |
|---|---|
| `GuiButton(text$)` | a button |
| `GuiLabel(text$)` | a line of text |
| `GuiTextBox([text$, parent, multiline])` | a box to type in |
| `GuiCheckBox(text$ [, checked, parent])` | a tick box |
| `GuiSlider(min, max, value)` | a slider |
| `GuiProgress([value])` | a progress bar, 0 to 1 |
| `GuiNumber(value, min, max, step)` | a number box with arrows |
| `GuiImage(image)` | a picture |
| `GuiDropDown(items$)`, `GuiList(items$)` | a drop-down and a list, items separated by `|` |
| `GuiTree()`, `GuiTreeItem(parent, text$)` | a tree of items |
| `GuiMenu()`, `GuiMenuItem(parent, text$)` | a menu bar and its menus |
| `GuiColorPicker(r, g, b)`, `GuiColorField(r, g, b)` | colour choosers |

## Reading what the user did

| Command | Gives |
|---|---|
| `GuiClicked(w)` | how many times it was clicked since you last asked (like `KeyHit`) |
| `GuiChanged(w)` | how many times the user changed it: typed, ticked, moved, chose |
| `GuiValue(w)` | its value: a slider's position, 1 for a ticked box, a number box's number |
| `GuiText(w)` | its text: what was typed, or the chosen item |
| `GuiSelected(w)` | the chosen item's number (from 0; -1 for none) |

and to change them from the program: `SetGuiText`, `SetGuiValue`,
`SetGuiSelected`.

`GuiUnderMouse()` is true when the mouse is over the GUI, so the game
can ignore clicks meant for it:

```basic
If MouseHit(1) And Not GuiUnderMouse() Then Print "fire!"
```

## Lists, menus and tabs

```basic
Graphics3D 1280, 720
menu = GuiMenu()
fileMenu = GuiMenuItem(menu, "File")
newItem = GuiMenuItem(fileMenu, "New")
quitItem = GuiMenuItem(fileMenu, "Quit")

win = GuiWindow("Character", 20, 60, 320, 400)
tabs = GuiTabs(win)
looks = GuiTab(tabs, "Looks")
GuiLabel "Class", looks
class = GuiDropDown("Ranger|Knight|Wizard", looks)
items = GuiTab(tabs, "Items")
bag = GuiList("Sword|Shield|Potion", items)
GuiAddItem bag, "Rope"

While Not KeyHit(KEY_ESCAPE)
    If GuiClicked(quitItem) Then End
    If GuiChanged(class) Then Print "Class: " + GuiText(class)
    If GuiChanged(bag) Then Print "Picked item " + GuiSelected(bag)
    RenderWorld
    Flip
Wend
```

`GuiAddItem`, `GuiRemoveItem`, `GuiClearItems`, `GuiCountItems` and
`GuiItemText` work on drop-downs and lists.

## Dialogs

```basic
Graphics3D 1280, 720
ask = GuiMessageBox("Quit", "Do you really want to quit?", 1)
While Not KeyHit(KEY_ESCAPE)
    If GuiChanged(ask)
        If GuiValue(ask) = 1 Then End       ; the first button was pressed
    EndIf
    RenderWorld
    Flip
Wend
```

`GuiMessageBox(title$, text$ [, buttons])` shows a message at once; the
program learns which button was pressed through `GuiChanged` and
`GuiValue`. `GuiFileDialog(title$, path$ [, save])` asks for a file to
open or save. `GuiModal window` makes a window that must be answered
before anything else; `GuiPopup` and `GuiShowPopup` make small pop-up
panels.

## Style

`GuiTheme 0` is the dark theme and `GuiTheme 1` the light one.
`GuiThemeColor` and `GuiThemeFontSize` change them for every widget;
`GuiColor`, `GuiTextColor` and `GuiFontSize` change one widget;
`GuiTooltip w, text$` adds a tip that shows when the mouse rests on it.

## Tools for editors

Several widgets are made for level editors and tools: `GuiVector3` (an X,
Y, Z editor), `GuiCurve` (a curve the user can shape, read with
`GuiCurveValue`), `GuiFileBrowser`, and `GuiDock` with `GuiDockWindow`
(windows that can be docked side by side). `examples/gui.nb` shows them
all over a 3D scene.

[Contents](../MANUAL.md) · Previous: [23. Networking](23-networking.md) · Next: [25. Sharing your game](25-sharing.md)
