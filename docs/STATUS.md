# Implementation status

What works today. Every item marked **done** has automated tests or was
checked by running an example program. Run `neptune commands` for the full
list of available commands.

**Blitz3D coverage:** every Blitz3D command is available (language,
console, files, banks, timers, networking, multiplayer games, 2D, 3D,
input, joysticks, sound, gamma, movies). `crates/neptune-vm/tests/programs/blitz_audit.nb` checks the
language's Blitz3D behaviour line by line. The few places where a modern
engine or OS cannot match 2003 hardware exactly are listed under *Design
choices*. On top
of that come NeptuneBASIC's extensions (physics, graphics quality,
gamepad constants, modern command names, ...).

## Language and compiler: done

* Lexer, parser, semantic analysis, typed IR, bytecode, resumable VM.
* Blitz3D syntax: types, arrays, `Data`/`Read`, `Gosub`, `Select`, `Goto`,
  type tags (`%`, `#`, `$`), `Each`/`First`/`Last`/`Before`/`After`/`Insert`,
  `Handle`/`Object`.
* Modern additions: type inference ("smart variables"), `Variant`,
  `Strict` and `Classic` modes, `Enum`, type methods, `Try`/`Catch`/`Throw`,
  `$"..."` string interpolation, `For Each x In`, `IIf`, `AndAlso`/`OrElse`,
  `+=`-style operators, `Continue`.
* Standard library: maths, strings, time, random numbers (`SeedRnd`,
  `RndSeed`), files and streams (`ReadFile`/`WriteFile`/`OpenFile`,
  `FilePos`, `SeekFile`, `ReadAvail`, `CopyStream`), directories
  (`ReadDir`, `NextFile`, `CloseDir`), banks (`CreateBank`, `Peek*`/`Poke*`,
  `ReadBytes`/`WriteBytes`, `CopyBank`, `ResizeBank`), timers (`CreateTimer`,
  `WaitTimer`, `FreeTimer`), system (`SystemProperty`, `SetEnv`, `ExecFile`,
  `CallDLL` with `.dll`/`.so`/`.dylib` libraries, `CommandLine`).
* Plugins: shared libraries in a `plugins` folder add typed commands and
  constants (C interface in `sdk/neptune_plugin.h`, Rust helper crate in
  `sdk/rust`); see [PLUGINS.md](PLUGINS.md).
* Networking: TCP (`OpenTCPStream`, `CreateTCPServer`, `AcceptTCPStream`,
  `TCPStreamIP/Port`, `TCPTimeouts`) and UDP (`CreateUDPStream`,
  `SendUDPMsg`, `RecvUDPMsg`, `UDPMsgIP/Port`, `UDPTimeouts`) streams that
  work with every stream command; `DottedIP`, `CountHostIPs`, `HostIP`.
* Multiplayer games: `StartNetGame`, `HostNetGame`, `JoinNetGame`,
  `StopNetGame`, `CreateNetPlayer`, `DeleteNetPlayer`, `NetPlayerName`,
  `NetPlayerLocal`, `SendNetMsg`, `RecvNetMsg`, `NetMsgType/From/To/Data`,
  with the joined (100), left (101) and host-lost (200) messages. Blitz3D
  used DirectPlay; here the host runs a TCP server (port 47700) and games
  are found on the local network over UDP (port 47701), so an empty
  address in `JoinNetGame` searches the LAN.
* Modern command names (ext) for the classic ones, such as
  `SetEntityPosition`, `LoadModel`, `UpdatePhysics` and `SetWindowTitle`
  (see `ALIASES` in `crates/neptune-runtime/src/commands.rs`).

## 3D runtime (Fyrox)

| Area | Status | Commands |
|---|---|---|
| Window and frames | done | `Graphics3D`/`Graphics` (mode 1 = borderless fullscreen), `RenderWorld` (with `CaptureWorld` tweening), `Flip`, `UpdateWorld`, `ClearWorld`, `AppTitle`, `GraphicsWidth/Height/Depth`, `SetFullscreen`/`IsFullscreen` (ext), `TrisRendered`, `VWait`, `ScanLine` |
| Graphics modes | done | `CountGfxModes`, `GfxModeWidth/Height/Depth`, `GfxModeExists`, `GfxMode3DExists`, `GfxMode3D`, `CountGfxDrivers`, `GfxDriverName`, `SetGfxDriver`, `GfxDriver3D`, `Windowed3D`, `TotalVidMem`, `AvailVidMem`, `GfxDriverCaps3D`, `HWTexUnits`, `HWMultiTex`, `AntiAlias`, `Dither`, `WBuffer` |
| Gamma | done | `SetGamma`, `UpdateGamma`, `GammaRed/Green/Blue` (applied through a colour-grading table on every camera) |
| Graphics quality (ext) | done | `GraphicsQuality 0-3`, `GetGraphicsQuality`, `PostEffect` (bloom, SSAO, FXAA, shadows, light scattering), `neptune run --quality low` |
| Entities | done | `CreatePivot/Cube/Sphere/Cylinder/Cone/Plane`, `FreeEntity`, `CopyEntity`, parenting, names, `Hide/ShowEntity` |
| Transforms | done | `Position/Move/Translate/Rotate/Turn/Scale/PointEntity`, `AlignToVector`, `EntityX/Y/Z`, `EntityPitch/Yaw/Roll`, `EntityDistance`, `EntityScaleX/Y/Z` (ext), `GetMatElement`, `TFormPoint/Vector/Normal` + `TFormedX/Y/Z`, `VectorYaw/Pitch`, `DeltaYaw/Pitch`, `EntityClass`, `LookAt`, `SetLightDirection`, `SetEntityDirection` (ext) |
| Cameras | done | `CameraZoom`, `CameraRange`, `CameraViewport`, `CameraClsColor`, `CameraClsMode` (a camera that does not clear is drawn over the cameras before it), `CameraProjMode` 0/1/2 (orthographic), `CameraFogMode/Color/Range`, `CameraProject` |
| Lights | done | directional, point, spot; `LightColor`, `LightRange`, `LightConeAngles`, `AmbientLight`, `LightShadows` (ext) |
| Materials | done | `EntityColor`, `EntityAlpha`, `EntityAutoFade`, `EntityShininess`, `EntityBlend`, `EntityFX` (all six flags), `EntityOrder`, `WireFrame`, `EntityNormalMap` (ext) |
| Textures | done | `LoadTexture` (colour, alpha, masked, clamp, sphere map and cube map flags), `LoadAnimTexture`, `CreateTexture`, `EntityTexture` layers 0-3 and frames, `TextureBlend`, `ScaleTexture`, `PositionTexture`, `RotateTexture`, `TextureCoords`, `TextureFilter`, `ClearTextureFilters`, `SetCubeFace`, `SetCubeMode`, `TextureWidth/Height/Name`, `ActiveTextures`, `FreeTexture` |
| Mirrors | done | `CreateMirror` (the reflection is rendered from a reflected camera each frame, before the main view) |
| Custom meshes | done | `CreateMesh`, `CreateSurface`, `GetSurface`, `CountSurfaces`, `AddVertex`, `AddTriangle`, `VertexCoords/Normal/Color/TexCoords` and getters, `UpdateNormals`, `FlipMesh`, `ScaleMesh`, `PositionMesh`, `RotateMesh`, `FitMesh`, `AddMesh`, `CopyMesh`, `ClearSurface`, `FindSurface`, `MeshesIntersect`, `LightMesh`, `MeshCullBox` |
| Brushes | done | `CreateBrush`, `LoadBrush`, `BrushColor/Alpha/Shininess/Texture/Blend/FX`, `PaintSurface/Entity/Mesh`, `GetSurfaceBrush`, `GetEntityBrush`, `GetBrushTexture`, `FreeBrush` |
| Background loading (ext) | done | `LoadMeshAsync` (also `LoadSceneAsync`) returns the entity at once and the model appears in it when loaded; `LoadTextureAsync` returns a white texture that gets its image when decoded; `EntityLoaded`, `TextureLoaded` (1 loaded, 0 loading, -1 failed), `PendingLoads` for loading screens |
| Saved scenes (ext) | done | `SaveScene` (all top-level entities except cameras) and `SaveEntity` write a Fyrox scene file (`.rgs`); `LoadMesh`/`LoadScene` load it back as one entity, `FindChild` finds named entities and loaded models can be animated again. Models are saved by file name and loaded again from their files. Scenes made in the Fyrox editor load the same way |
| Models | done | `LoadMesh`/`LoadAnimMesh` for glTF (`.gltf`/`.glb`) and FBX, `LoaderMatrix`, `CopyEntity` (animated copies are independent), `FindChild`, `CountChildren`/`GetChild` into models, `MeshWidth/Height/Depth`. Old Blitz formats (`.b3d`, `.x`, `.3ds`) are deliberately not supported: export to glTF from Blender or any modern tool |
| Animation | done | `Animate` (loop, ping-pong, one-shot, with `transition` blending between sequences), `SetAnimTime`, `Animating`, `AnimTime`, `AnimSeq`, `AnimLength`, `ExtractAnimSeq`, `LoadAnimSeq` (animations from another file, retargeted), `CountAnimSeqs` (ext); keyframe animation of any entity with `AddAnimSeq`/`SetAnimKey` |
| Collisions | done | `EntityType`, `EntityRadius`, `EntityBox`, `Collisions` (methods 1–3, responses 1–3), `ResetEntity`, `EntityCollided`, `CountCollisions`, `CollisionX/Y/Z/NX/NY/NZ/Time/Entity/Surface/Triangle`, `ClearCollisions` |
| Picking | done | `EntityPickMode`, `LinePick`, `CameraPick`, `EntityPick`, `Picked*`, `CameraProject`, `Projected*`, `EntityVisible`, `EntityInView` |
| Sprites | done | `CreateSprite`, `LoadSprite`, `SpriteViewMode` 1-4, `RotateSprite`, `ScaleSprite`, `HandleSprite`. Sprites face the first active camera |
| Terrain | done | `CreateTerrain`, `LoadTerrain`, `TerrainSize`, `TerrainHeight`, `ModifyTerrain`, `TerrainY/X/Z`; `TerrainDetail`/`TerrainShading` are accepted (the GPU draws the full grid) |
| Physics (ext) | done | `EntityPhysics` (dynamic, static, kinematic; box, sphere, capsule, cylinder, mesh, hull), `PhysicsGravity`, `EntityMass/Friction/Bounce/Damping/GravityScale`, `LockRotation`, `ApplyForce/Impulse/ImpulseAt/Torque/TorqueImpulse`, `SetVelocity`, `SetAngularVelocity`, `Velocity*`, `EntityTouching`, `CountContacts`, `ContactEntity`, `PhysicsRayCast`, `CreateBallJoint/HingeJoint/FixedJoint`, `FreeJoint`. Bodies move in world space: `EntityPhysics` detaches a parented entity in place, and `CopyEntity` gives copies their own body |
| Sound | done | `LoadSound`, `FreeSound`, `LoopSound`, `SoundVolume/Pan/Pitch`, `PlaySound`, `PlayMusic`, `PlayCDTrack` (no CD audio: returns no channel), `StopChannel`, `PauseChannel`, `ResumeChannel`, `ChannelVolume/Pan/Pitch`, `ChannelPlaying`, `CreateListener`, `Load3DSound`, `EmitSound` (WAV, OGG, MP3, FLAC) |
| 2D text | done | `Text`, `Print`/`Write` on the screen in graphics mode, `Locate`, `Color`, `GetColor`, `ClsColor`, `Cls`, `LoadFont`, `SetFont`, `FreeFont`, `FontWidth/Height`, `StringWidth/Height` |
| 2D drawing | done | `Rect`, `Oval`, `Line`, `Plot`, `Origin`, `Viewport`, on the screen or into image and texture buffers |
| Movies | done | `OpenMovie`, `DrawMovie`, `MovieWidth`, `MovieHeight`, `MoviePlaying`, `CloseMovie` for animated GIFs, Motion-JPEG AVI files and folders of numbered images (30 fps); no sound track |
| Tile maps (ext) | done | `CreateTileMap` (frames of an animation image), `SetTile`, `GetTile`, `DrawTileMap` (draws only what is on screen), `TileAt`, `TileMapCellX/Y`, `TileMapWidth/Height`, `TileMapImage`, `FreeTileMap`; autotiling with `TileMapTerrain` (16-frame or 47-frame "blob" layouts), `AutoTile`, `GetTileTerrain`; `SaveTileMap`/`LoadTileMap` (also reads CSV exported from the Tiled editor) |
| 2D images | done | `LoadImage`, `LoadAnimImage`, `CreateImage`, `CopyImage`, `GrabImage`, `DrawImage`, `DrawBlock`, `DrawImageRect`, `DrawBlockRect`, `TileImage`, `TileBlock`, `MidHandle`, `HandleImage`, `AutoMidHandle`, `MaskImage`, `ScaleImage`, `ResizeImage`, `RotateImage`, `TFormImage`, `TFormFilter`, `SaveImage`, `ImagesCollide`, `ImagesOverlap`, `ImageRectCollide`, `ImageRectOverlap`, `RectsOverlap` |
| Buffers and pixels | done | `SetBuffer`, `BackBuffer`, `FrontBuffer`, `ImageBuffer`, `TextureBuffer`, `GraphicsBuffer`, `LockBuffer`, `UnlockBuffer`, `ReadPixel(Fast)`, `WritePixel(Fast)`, `CopyPixel(Fast)`, `CopyRect`, `BufferWidth/Height`, `SaveBuffer` (the screen too), `LoadBuffer`; `Text` and every 2D drawing command work in image and texture buffers. Pixel reads come from image and texture buffers, not the screen |
| Keyboard and mouse | done | `KeyDown/Hit`, `GetKey`, `WaitKey`, `FlushKeys`, `MouseX/Y/Z`, `MouseXSpeed/YSpeed/ZSpeed` (raw motion, so mouse-look works with a hidden pointer), `MouseDown/Hit`, `GetMouse`, `WaitMouse`/`MouseWait`, `MoveMouse`, `HidePointer`, `ShowPointer`, `FlushMouse`, `CaptureMouse` (ext) |
| Joysticks and gamepads | done | `JoyType`, `JoyDown`, `JoyHit`, `GetJoy`, `WaitJoy`/`JoyWait`, `EnableDirectInput`, `DirectInputEnabled` (input always comes straight from the OS), `FlushJoy`, `JoyX/Y/Z/U/V`, `JoyPitch/Yaw/Roll`, `JoyHat`, `JoyXDir/YDir/ZDir/UDir/VDir`, `CountJoys` and `JOY_A`... constants (ext); Xbox, PlayStation and generic controllers through gilrs |

Programs work in logical pixels. On a high-DPI display, `Graphics3D 1280,720`
opens a window 1280×720 logical pixels in size. `GraphicsWidth`, `MouseX`
and `Text` coordinates all use the same units.

### Rendering paths

Entities use Fyrox's deferred PBR material (with real-time shadows) until
they need a Blitz fixed-function feature: blend modes, `EntityFX`,
shininess, alpha, several texture layers, texture transforms, sphere maps or
fog. They then switch to the classic forward shader in `classic.rs`
(per-pixel lighting from up to 16 lights, fog, four texture layers). The
switch is automatic and reversible.

### Older and newer PCs

The renderer needs OpenGL 3.3, which every desktop GPU from about 2010 on
supports (all GPUs of the last ten years). `--quality low` (or
`GraphicsQuality 0`) turns off shadows, SSAO, bloom, FXAA and multisampling
for older integrated GPUs; `ultra` turns everything up.

## Tooling

| Item | Status |
|---|---|
| `neptune run/check/disasm/commands` CLI | done |
| `neptune build`: stand-alone game folder (executable + media; no console window on Windows) | done |
| `neptune plugins`: list loaded plugins and plugin folders | done |
| `neptune lsp`: language server (errors as you type, completion, hover, signature help, outline, go to definition) | done |
| VS Code extension: highlighting, language server, run (F5), build | done (`editors/vscode`) |

## Platforms

Windows, Linux and macOS. Everything is written once for all three: the
renderer is OpenGL 3.3 through Fyrox, gamepads go through gilrs, plugins and
`CallDLL` load `.dll`, `.so` or `.dylib` files, screenshots read the game's
own frame, and errors of packaged games appear in a dialog box (a message box
on Windows, zenity/kdialog on Linux, an alert on macOS). The CI workflow
(`.github/workflows/ci.yml`) builds and tests on all three, including a
plugin round trip and packaging a game. `neptune build` packages a game for
the platform it runs on; build on each platform you ship to.

On Linux, building needs the usual development packages for sound, gamepads
and windowing (see the CI workflow for the list).

## Design choices

* Models load from glTF (`.gltf`/`.glb`) and FBX. The old Blitz formats
  (`.b3d`, `.x`, `.3ds`) are deliberately not supported: export to glTF
  from Blender or any modern tool.
* Built-in primitives are generated in Blitz space with Blitz3D's texture
  layout, so textures read the right way round on every face.
* Waiting commands (`WaitKey`, `WaitMouse`, `WaitJoy`) pause the program
  without freezing the window: the VM retries them each frame.
* The renderer is OpenGL 3.3, so there is one graphics driver; the mode list
  offers common screen sizes (any window size works).
* A camera with `CameraClsMode` colour clearing off renders into its own
  image, which is drawn over the screen before any 2D drawing, so it
  overlays the cameras before it (a gun or cockpit camera, for example).
* Blitz3D played movies through DirectShow (Windows only). NeptuneBASIC
  plays animated GIFs, Motion-JPEG AVI files and folders of numbered images
  on every platform without a video library; convert other video with
  `ffmpeg -i intro.mp4 -c:v mjpeg -q:v 3 -an intro.avi`. Movies have no
  sound: play the sound track with `PlayMusic`.
* Multiplayer games use TCP and LAN discovery instead of DirectPlay; the
  commands and messages are Blitz3D's.
* `WBuffer`, `MeshCullBox`, `TerrainDetail` and `TerrainShading` are
  accepted; the GPU handles depth precision, culling and detail itself.
* Cube maps are a strip of six faces in Blitz order (left, front, right,
  back, up, down), each face being the view from the centre of the cube.
* Mirrors are rendered with a reflected camera rather than by drawing the
  world upside down. The first four mirrors use cameras made before the
  program's own, so their reflections are ready in the same frame; more
  mirrors work but lag one frame.
* Audio CDs (`PlayCDTrack`) are not supported.
* Models and scene files load through Fyrox's resource manager, which only
  reads files inside the game's folder (the working directory). A model
  outside it makes `LoadMesh` return 0 with a message.
* Saved scenes keep what is part of the scene (meshes, materials,
  textures, lights, sprites, terrain, names, models). Collision types, pick
  modes, physics settings and sounds belong to the program and are set
  again after loading; so are changes made to a model's own parts.

## Examples

| File | Shows |
|---|---|
| `examples/cube.nb` | basic scene, keyboard movement |
| `examples/modern3d.nb` | the same idea with modern command names |
| `examples/modern.nb` | modern language features (console) |
| `examples/hud.nb` | 2D text and fonts over 3D |
| `examples/textures.nb` | textures, tiling, masking |
| `examples/materials.nb` | fog, blend modes, shininess, texture layers |
| `examples/meshes.nb` | custom meshes, vertex colours, brushes |
| `examples/models.nb` | glTF loading, `CopyEntity` |
| `examples/animation.nb` | animated models, sequences, `FindChild` |
| `examples/collisions.nb` | gravity, sliding, ramps, jumping |
| `examples/physics.nb` | rigid bodies, impulses, hinge joint, ray cast |
| `examples/terrain.nb` | terrain, `TerrainY`, upright sprites, fog |
| `examples/sound.nb` | sounds, channels, 3D sound |
| `examples/draw2d.nb` | 2D shapes, animated sprites, pixel collisions |
| `examples/buffers.nb` | drawing into textures and images |
| `examples/picking.nb` | mouse picking, 3D-to-2D labels |
| `examples/tilemap.nb` | 2D tile map: paint autotiled grass and water with the mouse, scroll, save and load |
| `examples/scenes.nb` | loading screen with background loading, then saving and loading the scene |
| `examples/movie.nb` | makes a movie from images, then plays it over a 3D scene |
| `examples/games/starcourier.nb` | **complete 2D game**: shooter with states, pickups, particles, sound, gamepad, high score, attract mode |
| `examples/games/orbrun.nb` | **complete 3D game**: animated fox on terrain, orbs, hazards, follow camera, HUD, best time, attract mode |
