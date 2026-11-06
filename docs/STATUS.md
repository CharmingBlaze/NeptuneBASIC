# Implementation status

What works today and what is still missing. Every item marked **done** has
automated tests or was checked by running an example program. Run
`neptune commands` for the full list of available commands (447 at the
time of writing, including the modern alias names).

## Language and compiler: done

* Lexer, parser, semantic analysis, typed IR, bytecode, resumable VM.
* Blitz3D syntax: types, arrays, `Data`/`Read`, `Gosub`, `Select`, `Goto`,
  type tags (`%`, `#`, `$`), `Each`/`First`/`Last`/`Before`/`After`/`Insert`,
  `Handle`/`Object`.
* Modern additions: type inference ("smart variables"), `Variant`,
  `Strict` and `Classic` modes, `Enum`, type methods, `Try`/`Catch`/`Throw`,
  `$"..."` string interpolation, `For Each x In`, `IIf`, `AndAlso`/`OrElse`,
  `+=`-style operators, `Continue`.
* Standard library: maths, strings, files, time, random numbers.
* Modern command names (ext) for the classic ones, such as
  `SetEntityPosition`, `LoadModel`, `UpdatePhysics` and `SetWindowTitle`
  (see `ALIASES` in `crates/neptune-runtime/src/commands.rs`).

## 3D runtime (Fyrox)

| Area | Status | Commands |
|---|---|---|
| Window and frames | done | `Graphics3D`, `RenderWorld`, `Flip`, `UpdateWorld`, `AppTitle`, `GraphicsWidth/Height` |
| Graphics quality (ext) | done | `GraphicsQuality 0-3`, `GetGraphicsQuality`, `PostEffect` (bloom, SSAO, FXAA, shadows, light scattering), `neptune run --quality low` |
| Entities | done | `CreatePivot/Cube/Sphere/Cylinder/Cone/Plane`, `FreeEntity`, `CopyEntity`, parenting, names, `Hide/ShowEntity` |
| Transforms | done | `Position/Move/Translate/Rotate/Turn/Scale/PointEntity`, `EntityX/Y/Z`, `EntityPitch/Yaw/Roll`, `EntityDistance`, `LookAt`, `SetLightDirection`, `SetEntityDirection` (ext) |
| Cameras | done | `CameraZoom`, `CameraRange`, `CameraViewport`, `CameraClsColor`, `CameraProjMode` 0/1/2 (orthographic), `CameraFogMode/Color/Range`, `CameraProject` |
| Lights | done | directional, point, spot; `LightColor`, `LightRange`, `LightConeAngles`, `AmbientLight`, `LightShadows` (ext) |
| Materials | done | `EntityColor`, `EntityAlpha`, `EntityShininess`, `EntityBlend`, `EntityFX` (all six flags), `EntityOrder`, `WireFrame`, `EntityNormalMap` (ext) |
| Textures | done | `LoadTexture` (colour, alpha, masked, clamp, sphere map flags), `CreateTexture`, `EntityTexture` layers 0-3, `TextureBlend`, `ScaleTexture`, `PositionTexture`, `RotateTexture`, `TextureCoords`, `TextureWidth/Height/Name`, `FreeTexture` |
| Custom meshes | done | `CreateMesh`, `CreateSurface`, `GetSurface`, `CountSurfaces`, `AddVertex`, `AddTriangle`, `VertexCoords/Normal/Color/TexCoords` and getters, `UpdateNormals`, `FlipMesh`, `ScaleMesh`, `PositionMesh`, `RotateMesh`, `FitMesh`, `AddMesh`, `CopyMesh`, `ClearSurface` |
| Brushes | done | `CreateBrush`, `LoadBrush`, `BrushColor/Alpha/Shininess/Texture/Blend/FX`, `PaintSurface/Entity/Mesh`, `GetSurfaceBrush`, `GetEntityBrush`, `FreeBrush` |
| Models | partial | `LoadMesh`/`LoadAnimMesh` for glTF and FBX, `CopyEntity` (animated copies are independent), `FindChild`, `CountChildren`/`GetChild` into models, `MeshWidth/Height/Depth`. Blitz `.b3d`/`.x`/`.3ds` files are not supported |
| Animation | done | `Animate` (loop, ping-pong, one-shot), `SetAnimTime`, `Animating`, `AnimTime`, `AnimSeq`, `AnimLength`, `ExtractAnimSeq`, `CountAnimSeqs` (ext). The `transition` blend argument is accepted but sequences switch instantly |
| Collisions | done | `EntityType`, `EntityRadius`, `EntityBox`, `Collisions` (methods 1–3, responses 1–3), `ResetEntity`, `EntityCollided`, `CountCollisions`, `CollisionX/Y/Z/NX/NY/NZ/Time/Entity`, `ClearCollisions` |
| Picking | done | `EntityPickMode`, `LinePick`, `CameraPick`, `EntityPick`, `Picked*`, `CameraProject`, `Projected*`, `EntityVisible`, `EntityInView` |
| Sprites | done | `CreateSprite`, `LoadSprite`, `SpriteViewMode` 1-4, `RotateSprite`, `ScaleSprite`, `HandleSprite`. Sprites face the first active camera |
| Terrain | done | `CreateTerrain`, `LoadTerrain`, `TerrainSize`, `TerrainHeight`, `ModifyTerrain`, `TerrainY/X/Z`; `TerrainDetail`/`TerrainShading` are accepted (the GPU draws the full grid) |
| Physics (ext) | done | `EntityPhysics` (dynamic, static, kinematic; box, sphere, capsule, cylinder, mesh, hull), `PhysicsGravity`, `EntityMass/Friction/Bounce/Damping/GravityScale`, `LockRotation`, `ApplyForce/Impulse/ImpulseAt/Torque/TorqueImpulse`, `SetVelocity`, `SetAngularVelocity`, `Velocity*`, `EntityTouching`, `CountContacts`, `ContactEntity`, `PhysicsRayCast`, `CreateBallJoint/HingeJoint/FixedJoint`, `FreeJoint`. Physics entities must not have a parent |
| Sound | done | `LoadSound`, `FreeSound`, `LoopSound`, `SoundVolume/Pan/Pitch`, `PlaySound`, `PlayMusic`, `StopChannel`, `PauseChannel`, `ResumeChannel`, `ChannelVolume/Pan/Pitch`, `ChannelPlaying`, `CreateListener`, `Load3DSound`, `EmitSound` (WAV, OGG, MP3, FLAC) |
| 2D text | done | `Text`, `Color`, `ClsColor`, `Cls`, `LoadFont`, `SetFont`, `FreeFont`, `FontWidth/Height`, `StringWidth/Height` |
| 2D drawing | done | `Rect`, `Oval`, `Line`, `Plot`, `Origin`, on the screen or into image and texture buffers |
| 2D images | done | `LoadImage`, `LoadAnimImage`, `CreateImage`, `CopyImage`, `GrabImage`, `DrawImage`, `DrawBlock`, `DrawImageRect`, `DrawBlockRect`, `TileImage`, `TileBlock`, `MidHandle`, `HandleImage`, `AutoMidHandle`, `MaskImage`, `ScaleImage`, `ResizeImage`, `RotateImage`, `ImagesCollide`, `ImagesOverlap`, `ImageRectCollide`, `ImageRectOverlap`, `RectsOverlap` |
| Buffers and pixels | done | `SetBuffer`, `BackBuffer`, `FrontBuffer`, `ImageBuffer`, `TextureBuffer`, `GraphicsBuffer`, `LockBuffer`, `UnlockBuffer`, `ReadPixel(Fast)`, `WritePixel(Fast)`, `CopyRect`. The screen cannot be read back, and `Text` does not draw into image or texture buffers yet |
| Input | done | `KeyDown/Hit`, `GetKey`, `MouseX/Y/Z`, `MouseDown/Hit`, mouse speed, flush commands |

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
| `neptune build`: stand-alone game folder (executable + media) | done |
| VS Code extension: highlighting, errors on save, run (F5), build | done (`editors/vscode`) |
| Language server (completion, hover) | not started |

## Not done yet

* Blitz `.b3d`, `.x` and `.3ds` model files (glTF and FBX work).
* Blending between animation sequences (`Animate` transition argument).
* `Text` into image and texture buffers; reading pixels from the screen.
* Physics on parented entities, and `CopyEntity` of physics entities
  (the copy has no body).
* Hiding the console window of packaged games on Windows.

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
