# Implementation status

What works today, what is partial, and what is not started. Every item marked
**done** has automated tests or was checked by running an example program.
Run `neptune commands` for the full list of available commands (188 at the
time of writing).

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

## 3D runtime (Fyrox)

| Area | Status | Commands |
|---|---|---|
| Window and frames | done | `Graphics3D`, `RenderWorld`, `Flip`, `UpdateWorld`, `AppTitle`, `GraphicsWidth/Height` |
| Entities | done | `CreatePivot/Cube/Sphere/Cylinder/Cone/Plane`, `FreeEntity`, `CopyEntity`, parenting, names, `Hide/ShowEntity` |
| Transforms | done | `Position/Move/Translate/Rotate/Turn/Scale/PointEntity`, `EntityX/Y/Z`, `EntityPitch/Yaw/Roll`, `EntityDistance`, `LookAt` (ext) |
| Cameras | partial | `CameraZoom`, `CameraRange`, `CameraViewport`, `CameraClsColor`; orthographic mode (`CameraProjMode 2`) not yet |
| Lights | done | directional, point, spot; `LightColor`, `LightRange`, `LightConeAngles`, `LightShadows` (ext) |
| Materials | partial | `EntityColor`, `EntityAlpha`, `WireFrame`; no `EntityShininess`, `EntityFX` or `EntityBlend` yet |
| Textures | partial | `LoadTexture` (flags: colour, alpha, masked, clamp U/V), `EntityTexture` (layer 0), `ScaleTexture`, `EntityNormalMap` (ext); no `PositionTexture`/`RotateTexture`, multitexturing, `CreateTexture` or pixel access yet |
| Models | partial | `LoadMesh`/`LoadAnimMesh` for glTF and FBX via Fyrox, `MeshWidth/Height/Depth`, `CountMeshTriangles` (ext). Blitz `.b3d`/`.x`/`.3ds` files are not supported. No animation playback or `FindChild` yet |
| Collisions | done | `EntityType`, `EntityRadius`, `EntityBox`, `Collisions` (methods 1–3, responses 1–3), `ResetEntity`, `EntityCollided`, `CountCollisions`, `CollisionX/Y/Z/NX/NY/NZ/Time/Entity`, `ClearCollisions` |
| 2D text | done | `Text`, `Color`, `ClsColor`, `Cls`, `LoadFont` (files or installed system fonts), `SetFont`, `FreeFont`, `FontWidth/Height`, `StringWidth/Height` |
| 2D drawing | not started | `Rect`, `Line`, `Oval`, `Plot`, images |
| Input | done | `KeyDown/Hit`, `GetKey`, `MouseX/Y/Z`, `MouseDown/Hit`, mouse speed, flush commands |
| Picking | not started | `LinePick`, `CameraPick`, `EntityPick`, `PickedX/Y/Z` |
| Custom meshes | not started | `CreateMesh`, `CreateSurface`, `AddVertex`, `AddTriangle` |
| Sound | not started | Fyrox has a sound engine ready to expose |
| Physics | not started | Fyrox's Rapier physics, as (ext) commands |
| Terrain, water, fog, post-processing | not started | |

Programs work in logical pixels. On a high-DPI display, `Graphics3D 1280,720`
opens a window 1280×720 logical pixels in size. `GraphicsWidth`, `MouseX`
and `Text` coordinates all use the same units.

## Tooling

| Item | Status |
|---|---|
| `neptune run/check/disasm/commands` CLI | done |
| IDE / editor integration | not started |
| Packaging stand-alone games | not started |

## Examples

| File | Shows |
|---|---|
| `examples/cube.nb` | basic scene, keyboard movement |
| `examples/modern.nb` | modern language features (console) |
| `examples/hud.nb` | 2D text and fonts over 3D |
| `examples/textures.nb` | textures, tiling, masking |
| `examples/models.nb` | glTF loading, `CopyEntity` |
| `examples/collisions.nb` | gravity, sliding, ramps, jumping |
