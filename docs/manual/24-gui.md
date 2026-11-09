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
- [Try it yourself](#try-it-yourself)

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

## Try it yourself

These are all made from `Gui...` widgets, created once before the loop.

1. **Greeter.** Make a window with a text box, a button called `Greet` and a
   label. When the button is clicked, the label says `Hello, ` and then
   whatever was typed in the box, with an exclamation mark.
   *Hint: `GuiClicked` tells you the button was pressed, `GuiText` reads the box and `SetGuiText` changes the label.*
2. **Options screen.** Make an options window with a volume slider from 0 to
   100, a `Fullscreen` tick box and a drop-down to choose the difficulty
   (Easy, Normal or Hard). Under them show a progress bar that follows the
   slider, and a label that always describes all three settings.
   *Hint: `GuiChanged` is true when the user has changed a widget, and `SetGuiValue` moves a progress bar (0 to 1).*
3. **Backpack.** Make a window with a list of items (Sword, Shield, Potion). A
   `Use` button removes the chosen item from the list, a `Find rope` button adds
   a Rope, and a label always says how many items there are, or
   `Your backpack is empty!`
   *Hint: `GuiSelected` is -1 when nothing is chosen, so check for that before removing.*

### Answers

1.

```basic
Graphics3D 1280, 720
camera = CreateCamera()

win = GuiWindow("Greeter", 40, 40, 300, 200)
box = GuiTextBox("", win)
greet = GuiButton("Greet", win)
message = GuiLabel("Type your name above", win)

While Not KeyHit(KEY_ESCAPE)
    If GuiClicked(greet)
        SetGuiText message, "Hello, " + GuiText(box) + "!"
    EndIf
    RenderWorld
    Flip
Wend
```

The widgets are made once, before the loop, and each one is given the window
as its parent so it sits inside it. In the loop, the program only asks what
the user did. `GuiClicked` counts clicks since it was last asked, so one click
changes the label once.

2.

```basic
Graphics3D 1280, 720
camera = CreateCamera()

win = GuiWindow("Options", 40, 40, 320, 300)
GuiLabel "Volume", win
volume = GuiSlider(0, 100, 80, win)
bar = GuiProgress(0.8, win)
fullscreen = GuiCheckBox("Fullscreen", 0, win)
GuiLabel "Difficulty", win
difficulty = GuiDropDown("Easy|Normal|Hard", win)
SetGuiSelected difficulty, 1
summary = GuiLabel("", win)

changed = True                      ; describe the settings once at the start
While Not KeyHit(KEY_ESCAPE)
    If GuiChanged(volume) Then changed = True
    If GuiChanged(fullscreen) Then changed = True
    If GuiChanged(difficulty) Then changed = True

    If changed
        SetGuiValue bar, GuiValue(volume) / 100
        text$ = "Volume " + Int(GuiValue(volume)) + ", "
        text = text + GuiText(difficulty) + ", "
        text = text + IIf(GuiValue(fullscreen) > 0, "fullscreen", "windowed")
        SetGuiText summary, text
        changed = False
    EndIf

    RenderWorld
    Flip
Wend
```

The program keeps a flag, `changed`, which is set whenever any of the three
widgets is changed. The label and the bar are only rebuilt when it is set, and
once at the start so they begin correct. A tick box gives 1 from `GuiValue`
when ticked, a drop-down gives the chosen item from `GuiText`, and the slider
(0 to 100) is divided by 100 for the progress bar, which wants 0 to 1.

3.

```basic
Graphics3D 1280, 720
camera = CreateCamera()

win = GuiWindow("Backpack", 40, 40, 300, 340)
bag = GuiList("Sword|Shield|Potion", win)
use = GuiButton("Use", win)
find = GuiButton("Find rope", win)
status = GuiLabel("", win)
refresh = True

While Not KeyHit(KEY_ESCAPE)
    If GuiClicked(use)
        chosen = GuiSelected(bag)
        If chosen >= 0
            GuiRemoveItem bag, chosen
            refresh = True
        EndIf
    EndIf
    If GuiClicked(find)
        GuiAddItem bag, "Rope"
        refresh = True
    EndIf

    If refresh
        count = GuiCountItems(bag)
        If count = 0
            SetGuiText status, "Your backpack is empty!"
        Else
            SetGuiText status, "Items: " + count
        EndIf
        refresh = False
    EndIf

    RenderWorld
    Flip
Wend
```

Nothing is drawn by the program: it only reacts. `Use` removes the item that is
chosen in the list (items count from 0, so the number from `GuiSelected` goes
straight to `GuiRemoveItem`), but only if something is chosen. `Find rope`
adds an item. In both cases the status label is recounted from the list
itself, so it can never disagree with what the list shows.

[Contents](../MANUAL.md) · Previous: [23. Networking](23-networking.md) · Next: [25. Sharing your game](25-sharing.md)
