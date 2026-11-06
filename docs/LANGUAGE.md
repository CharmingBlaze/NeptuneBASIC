# NeptuneBASIC language reference

NeptuneBASIC accepts classic Blitz3D BASIC and extends it. Keywords and
identifiers are case-insensitive.

## Programs and comments

```basic
; Blitz comment
' BASIC comment
x = 1 : y = 2            ; ':' separates statements
total = 1 + 2 + _
        3                ; '_' continues a line
Include "util.nb"        ; textual include (each file once)
Import "math.nb"         ; same as Include
```

## Types and variables

| type | tag | `As` name | notes |
|---|---|---|---|
| Int | `%` | `Int`, `Integer`, `Bool`, `Boolean` | 32-bit |
| Float | `#` | `Float`, `Single` | 32-bit |
| String | `$` | `String`, `Str` | |
| object | `.Type` | `TypeName` | reference to a `Type` instance |
| Variant | – | `Variant`, `Any` | dynamic, holds any value |

```basic
score% = 10          ; tag
Local speed As Float = 2.5
Global name$ = "Neo"
Const MAX = 100, TITLE$ = "Game"
```

### Smart variables (type inference)

Untagged variables take the type of what they are assigned, across the
whole function, so you rarely need tags:

```basic
x = 1.5              ; Float (Blitz3D would truncate to Int 2)
name = "Bob"         ; String
speed = 1
speed = speed + 0.5  ; Int and Float assignments widen to Float
p = New Player       ; object of type Player
Function Half(v#)    ; return type inferred as Float
    Return v / 2
End Function
```

* `Classic` (a statement at the top of the file) restores Blitz3D rules:
  untagged variables are Int.
* `Strict` requires every variable to be declared with `Local`/`Global`.
* Reading a variable that is never assigned anywhere produces a warning.

### Variants

```basic
Local v As Variant = 10
v = "ten"             ; type can change at run time
Print TypeOf(v)       ; "String"
```

Conversions between Int, Float and String are implicit (Blitz rules:
Float→Int rounds to nearest even, String→number parses the leading number).

## Operators (lowest to highest precedence)

`AndAlso` `OrElse` (short-circuit) · `Not` · `And` `Or` `Xor` (bitwise) ·
`=` `<>` `<` `>` `<=` `>=` · `+` `-` · `Shl` `Shr` `Sar` · `*` `/` `Mod` ·
`^` · unary `-` `+` `~` `Abs` `Sgn`, casts `Int()` `Float()` `Str()`.

Compound assignment: `+=`, `-=`, `*=`, `/=`.

`IIf(condition, a, b)` evaluates only the selected branch.

## Strings

```basic
s$ = "Score: " + score               ; numbers convert automatically
Print $"Player {name} has {hp * 2} HP {{literal braces}}"
```

## Control flow

```basic
If x > 1 Then Print "big" Else Print "small"
If a
    ...
ElseIf b          ; also "Else If"
    ...
Else
    ...
EndIf             ; also "End If"

While cond : ... : Wend
Repeat : ... : Until cond      ; or Forever
For i = 1 To 10 Step 2 : ... : Next
For Each e In Enemy : ... : Next        ; all objects of a Type
For Each v In values : ... : Next       ; elements of a Blitz array
For e.Enemy = Each Enemy : ... : Next   ; classic form
Exit / Continue

Select value
    Case 1, 2 : ...
    Case 3    : ...
    Default   : ...
End Select

Goto label / Gosub label / Return       ; labels: .label
```

## Functions

```basic
Function Greet$(name$, greeting$ = "Hello")
    Return greeting + ", " + name
End Function
Function Area(w, h) As Float
    Return w * h
End Function
```

## Enums

```basic
Enum State
    Idle             ; 0
    Running          ; 1
    Jumping = 10
    Falling          ; 11
End Enum
Local s As State = State.Running
```

## Types (objects), methods

```basic
Type Ship
    Field name$
    Field x#, speed# = 2.5      ; field defaults
    Field cargo[9]              ; fixed array field
    Method Move(dt#)
        Self\x = Self\x + Self\speed * dt
    End Method
End Type

s.Ship = New Ship
s\Move(0.016)
For Each s In Ship : Print s\name : Next
Delete s                      ; references to s become Null
Delete Each Ship
first.Ship = First Ship : nxt.Ship = After first
Insert nxt Before first
h = Handle(s) : s = Object.Ship(h)
```

## Arrays

```basic
Dim grid(10, 10)        ; global, indices 0..10, re-Dim clears
grid(3, 4) = 1
Local v[4]              ; Blitz array, indices 0..4
v[0] = 5
```

## Data

```basic
Restore levels
Read w, h, name$
.levels
Data 10, 20, "first"
```

## Modern command names

Every 3D command keeps its Blitz3D name. Many also have a modern name
(marked "(ext)" in the docs); both spellings take the same arguments:

```basic
SetWindowTitle "Game"            ; AppTitle
player = LoadModel("hero.glb")   ; LoadMesh
SetEntityPosition player, 0, 1, 0 ; PositionEntity
SetLightDirection sun, 0, -1, 0  ; points a light along a vector
UpdatePhysics                    ; UpdateWorld
```

Named constants cover the common flags: `DIRECTIONAL`, `POINT`, `SPOT`,
`BLEND_ALPHA/MULTIPLY/ADD`, `FX_FULLBRIGHT/VERTEXCOLOR/FLAT/NOFOG/TWOSIDED/FORCEALPHA`,
`PHYS_DYNAMIC/STATIC/KINEMATIC`, `SHAPE_BOX/SPHERE/CAPSULE/CYLINDER/MESH/HULL`,
`QUALITY_LOW/MEDIUM/HIGH/ULTRA`, `EFFECT_BLOOM/SSAO/FXAA/SHADOWS`.

## Physics (ext)

```basic
EntityPhysics ground, PHYS_STATIC              ; mesh collider for static bodies
EntityPhysics crate, PHYS_DYNAMIC, SHAPE_BOX, 5 ; mass 5
ApplyImpulse crate, 0, 10, 0
If EntityTouching(crate, ground) Then ...
door = CreateHingeJoint(post, panel, 5, 2, 0, 0, 1, 0) ; anchor and axis
```

The simulation steps every frame by itself; position, rotation and the
other entity commands keep working on physics entities.

## Game loops and timing

A fixed-rate loop keeps a game the same speed on every PC:

```basic
timer = CreateTimer(60)              ; 60 ticks a second
While Not KeyHit(KEY_ESCAPE)
    ticks = WaitTimer(timer)         ; waits, returns ticks since last time
    For t = 1 To Min(ticks, 4)       ; catch up if a frame was slow
        UpdateGame()
    Next
    RenderWorld
    Flip
Wend
```

In graphics mode `Print` and `Write` draw on the screen like Blitz3D:
`Locate x, y` moves the text cursor, and `Cls`/`RenderWorld` reset it.

## Banks and networking

```basic
b = CreateBank(8)
PokeInt b, 0, 1234 : PokeFloat b, 4, 2.5
Print PeekInt(b, 0)

server = CreateTCPServer(8000)
client = OpenTCPStream("127.0.0.1", 8000)
conn = AcceptTCPStream(server)
WriteLine client, "hello"
Print ReadLine(conn)                 ; TCP and UDP streams use the file commands
```

Multiplayer games use Blitz3D's commands over TCP. `JoinNetGame` with an
empty address finds the game on the local network:

```basic
If HostNetGame("Arena") Then me = CreateNetPlayer("Host")
; on another PC: JoinNetGame "Arena", ""
While RecvNetMsg()
    If NetMsgType() = 100 Then Print NetPlayerName(NetMsgFrom()) + " joined"
Wend
SendNetMsg 1, "hello", me
```

## Background loading and saved scenes (ext)

`LoadMeshAsync` and `LoadTextureAsync` return at once, so a loading screen
can show progress. The entity can be placed straight away; its model appears
when it has loaded.

```basic
fox = LoadMeshAsync("media/Fox.glb")
total = PendingLoads()
While PendingLoads() > 0
    Cls : Rect 100, 100, 300 * (total - PendingLoads()) / total, 20 : Flip
Wend
If EntityLoaded(fox) = -1 Then RuntimeError "Could not load the fox"
```

`SaveScene` writes every top-level entity (except cameras) to a scene file,
and `SaveEntity` writes one entity with its children. `LoadScene` (the same
as `LoadMesh`) loads the file as one entity; `FindChild` finds the named
entities in it.

```basic
NameEntity door, "door"
SaveScene "levels/level1.rgs"
; later
level = LoadScene("levels/level1.rgs")
door = FindChild(level, "door")
```

## Modern rendering (ext)

```basic
SkyBox LoadTexture("sky.png", 128)       ; six faces side by side
EntityMetallic ball, 1 : EntityRoughness ball, 0.2
EntityEmissive lamp, 255, 200, 120, 4   ; glows with PostEffect EFFECT_BLOOM
EntityHeightMap wall, LoadTexture("bricks_height.png"), 0.05

blood = CreateDecal(LoadTexture("splat.png", 3))
PositionEntity blood, x, 0, z : ScaleEntity blood, 2, 1, 2

smoke = CreateEmitter(LoadTexture("puff.png", 3))
EmitterVelocity smoke, 0, 1, 0, 0.3
EmitterColor smoke, 200, 200, 200, 180 : EmitterFadeColor smoke, 80, 80, 80, 0

EntityLOD tree, detailed, 0, 40          ; children shown by distance
EntityLOD tree, billboard, 40

CameraToTexture securitycam, screentexture
CameraExposure cam, 1                    ; automatic exposure
CameraColorGrading cam, "warm_lut.png"   ; 256x16 lookup table

water = LoadShader("water.shader")       ; Fyrox shader file
EntityShader mesh, water
ShaderFloat mesh, "time", MilliSecs() / 1000.0
VertexData surface, v, 0, phase, height  ; read in the shader at location 8
```

## GUI widgets (ext)

`Gui*` widgets are made once and kept; Fyrox draws them and handles the
mouse and keyboard, and the program reads what the user did. Parent 0 puts
a widget on the screen; a window, panel, tab or scroll area holds the
widgets created inside it.

```basic
win = GuiWindow("Options", 20, 20, 300, 260)
name = GuiTextBox("Player", win)
level = GuiDropDown("Easy|Normal|Hard", win)
volume = GuiSlider(0, 100, 80, win)
ok = GuiButton("OK", win)
menu = GuiMenu() : quit = GuiMenuItem(GuiMenuItem(menu, "File"), "Quit")

While Not KeyHit(1)
    If GuiClicked(ok) Then Print GuiText(name) + " / " + GuiText(level) + " / " + GuiValue(volume)
    If GuiClicked(quit) Then box = GuiMessageBox("Quit", "Really quit?", 1)
    If box And GuiChanged(box) Then If GuiValue(box) = 1 Then End
    Flip
Wend
```

## Animation states, blend spaces and layers (ext)

```basic
moves = CreateBlendSpace()                ; idle, walk and run along "speed"
BlendSpacePoint moves, 0, 0 : BlendSpacePoint moves, 1, 1 : BlendSpacePoint moves, 2, 3
AddBlendState hero, "move", moves, "speed"
AddAnimState hero, "jump", 3, 3           ; sequence 3, played once
AddAnimTransition hero, "move", "jump", "jump = 1", 6
AddAnimTransition hero, "jump", "move", "end", 10
StartAnimStates hero, "move"
; each frame
SetAnimParam hero, "speed", speed
SetAnimParam hero, "jump", KeyHit(57)
AnimLayer hero, 1, 4, 1, 1, 1, "Spine"    ; aim with the upper body only
UpdateWorld

glow = CreateTrack(lamp, "lightcolor")    ; animate any property
TrackKey glow, 0, 255, 100, 0 : TrackKey glow, 60, 0, 100, 255
PlayTrack glow, 2
```

## Joint motors and ragdolls (ext)

```basic
door = CreateHingeJoint(post, panel, x, y, z, 0, 1, 0)
JointLimits door, -100, 100
JointTarget door, 90                     ; swing open like a spring
wheel = CreateHingeJoint(car, tyre, x, y, z, 1, 0, 0)
JointMotor wheel, 360, 5000              ; one turn a second
Print JointAngle(wheel)

fox = LoadAnimMesh("fox.glb") : Animate fox
CreateRagdoll fox
RagdollEnable fox                        ; goes limp and falls
RagdollImpulse fox, 0, 3, 5
RagdollEnable fox, False : Animate fox   ; back to animation
```

The same joint commands with `2D` at the end work on 2D joints.

## 2D camera, physics, animation and UI (ext)

```basic
player = CreateBody2D(0, 100, 300)       ; dynamic body at (100, 300) pixels
Body2DCapsule player, 20, 30
Body2DFixedRotation player, True
level = TileMapBody2D(map)               ; solid tiles become static boxes
walk = CreateAnim2D(heroimage, 0, 3, 10) ; frames 0-3 at 10 per second

While Not KeyHit(1)
    SetVelocity2D player, (KeyDown(205) - KeyDown(203)) * 200, Body2DVY(player)
    Cls
    Camera2D Body2DX(player), Body2DY(player), 1.5   ; follow, zoomed in
    DrawTileMap map
    DrawImageEx heroimage, Body2DX(player), Body2DY(player), Anim2DFrame(walk)
    Camera2DEnable False                 ; HUD and UI in screen pixels
    If UIButton(10, 10, 100, 30, "Restart") Then PositionBody2D player, 100, 300
    volume# = UISlider(10, 50, 150, volume, 0, 1)
    Flip
Wend
```

## Tile maps (ext)

A tile map is a grid of frames of an animation image. `AutoTile` paints a
terrain and picks each cell's frame from its neighbours (see
`TileMapTerrain` for the 16- and 47-frame layouts).

```basic
tiles = LoadAnimImage("tiles.png", 32, 32, 0, 48)
map = CreateTileMap(tiles, 100, 50)
TileMapTerrain map, 1, 0, 16      ; terrain 1 uses frames 0-15
AutoTile map, 5, 5, 1
SetTile map, 0, 0, 40             ; or set frames directly
DrawTileMap map, -scrollx, -scrolly
solid = TileAt(map, px + scrollx, py + scrolly) >= 0   ; -1 is an empty cell
```

## Movies

`OpenMovie` plays animated GIFs, Motion-JPEG AVI files and folders of
numbered images; `DrawMovie` draws the current frame and returns 0 once the
movie has ended.

```basic
movie = OpenMovie("intro.avi")
While DrawMovie(movie, 0, 0, GraphicsWidth(), GraphicsHeight()) And Not KeyHit(1)
    Flip
Wend
CloseMovie movie
```

## Errors

```basic
Try
    risky()
Catch e$
    Print "failed: " + e
End Try
Throw "custom error"
```

Runtime errors (division by zero, Null object access, out-of-range
indices, ...) report `file:line:col` and can be caught with `Try`.
