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
* Interpreter speed: superinstructions, an in-place value stack and typed
  `Dim` arrays (see *VM design* in [ARCHITECTURE.md](ARCHITECTURE.md) and
  the language table in [BENCHMARKS.md](BENCHMARKS.md)). Not done: a
  just-in-time compiler; faster string building (`s = s + x` copies the
  whole string each time).
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
  used DirectPlay; here games run over iroh (encrypted QUIC with NAT traversal
  and a relay fallback) on a background thread, and are found on the local
  network over UDP (port 47701), so an empty address in `JoinNetGame` searches
  the LAN. `reliable=0` messages are real datagrams.
* Easy multiplayer (ext): `NetPlay`, `NetCreate`, `NetJoin`, `NetInvite`,
  shared values (`NetSet`, `NetGet`), events (`NetSend`, `NetRecv`), entities
  that sync and interpolate by themselves (`NetTemplate`, `NetSpawn`, `NetRate`),
  `NetSimulate` for testing bad connections, `NetRelay` for your own relay,
  and `neptune run --players n`. Character controllers on a template are
  run by the host from the player's inputs, predicted and corrected on the
  player's computer (`NetCharacter`, `NetCorrections`; 3D only). Shots are
  judged on the host against what the shooter saw (`NetFire`, `NetHit`,
  half a second at most). Not done yet: sending only what changed, a new
  host taking over, voice chat, networked 2D characters.
* Modern command names (ext) for the classic ones, such as
  `SetEntityPosition`, `LoadModel`, `UpdatePhysics` and `SetWindowTitle`
  (see `ALIASES` in `crates/neptune-runtime/src/commands.rs`).

## 3D runtime (Fyrox)

| Area | Status | Commands |
|---|---|---|
| Window and frames | done | `Graphics3D`/`Graphics` (mode 1 = borderless fullscreen), `RenderWorld` (with `CaptureWorld` tweening), `Flip`, `UpdateWorld`, `ClearWorld`, `AppTitle`, `GraphicsWidth/Height/Depth`, `SetFullscreen`/`IsFullscreen` (ext), `TrisRendered`, `VWait`, `ScanLine` |
| Graphics modes | done | `CountGfxModes`, `GfxModeWidth/Height/Depth`, `GfxModeExists`, `GfxMode3DExists`, `GfxMode3D`, `CountGfxDrivers`, `GfxDriverName`, `SetGfxDriver`, `GfxDriver3D`, `Windowed3D`, `TotalVidMem`, `AvailVidMem`, `GfxDriverCaps3D`, `HWTexUnits`, `HWMultiTex`, `AntiAlias`, `Dither`, `WBuffer` |
| Gamma | done | `SetGamma`, `UpdateGamma`, `GammaRed/Green/Blue` (applied through a colour-grading table on every camera) |
| Graphics quality (ext) | done | `GraphicsQuality 0-3`, `GetGraphicsQuality`, `PostEffect` (bloom, SSAO, FXAA, shadows, light scattering), `AutoQuality`, `RenderScale`, `GetRenderScale`, `GraphicsCardName`, `neptune run --quality low|auto`, `--max-quality`, `--render-scale` |
| Entities | done | `CreatePivot/Cube/Sphere/Cylinder/Cone/Plane`, `FreeEntity`, `CopyEntity`, parenting, names, `Hide/ShowEntity` |
| Transforms | done | `Position/Move/Translate/Rotate/Turn/Scale/PointEntity`, `AlignToVector`, `EntityX/Y/Z`, `EntityPitch/Yaw/Roll`, `EntityDistance`, `EntityScaleX/Y/Z` (ext), `GetMatElement`, `TFormPoint/Vector/Normal` + `TFormedX/Y/Z`, `VectorYaw/Pitch`, `DeltaYaw/Pitch`, `EntityClass`, `LookAt`, `SetLightDirection`, `SetEntityDirection` (ext) |
| Tweens and easing (ext) | done | `TweenPosition`, `TweenRotation`, `TweenScale`, `TweenAlpha`, `TweenColor` along 17 easing curves (`EASE_*`), `Tweening`, `StopTweens`; `Ease` and `EaseBetween` for anything else |
| Cameras | done | `CameraZoom`, `CameraRange`, `CameraViewport`, `CameraClsColor`, `CameraClsMode` (a camera that does not clear is drawn over the cameras before it), `CameraProjMode` 0/1/2 (orthographic), `CameraFogMode/Color/Range`, `CameraProject` |
| Camera shake and screen fades (ext) | done | `CameraShake` jolts the picture without moving the camera; `ScreenFade` lays a colour over 3D and 2D, `ScreenFading` |
| Lights | done | directional, point, spot; `LightColor`, `LightRange`, `LightConeAngles`, `AmbientLight`; (ext) `LightShadows`, `LightIntensity` (brighter than white), `LightScatter` (glow in the air around point and spot lights, per light, tinted), `LightCookie` (spot lights project a texture: bars, leaves, stained glass), `LightShadowBias` |
| Materials | done | `EntityColor`, `EntityAlpha`, `EntityAutoFade`, `EntityShininess`, `EntityBlend`, `EntityFX` (all six flags), `EntityOrder`, `WireFrame`, `EntityNormalMap` (ext) |
| Textures | done | `LoadTexture` (colour, alpha, masked, clamp, sphere map and cube map flags, DDS files including DXT-compressed ones, flag 4096 (ext) to store a texture compressed), `LoadAnimTexture`, `CreateTexture`, `EntityTexture` layers 0-3 and frames, `TextureBlend`, `ScaleTexture`, `PositionTexture`, `RotateTexture`, `TextureCoords`, `TextureFilter`, `ClearTextureFilters`, `SetCubeFace`, `SetCubeMode`, `TextureWidth/Height/Name`, `ActiveTextures`, `FreeTexture` |
| Mirrors | done | `CreateMirror` (the reflection is rendered from a reflected camera each frame, before the main view) |
| Custom meshes | done | `CreateMesh`, `CreateSurface`, `GetSurface`, `CountSurfaces`, `AddVertex`, `AddTriangle`, `VertexCoords/Normal/Color/TexCoords` and getters, `UpdateNormals`, `FlipMesh`, `ScaleMesh`, `PositionMesh`, `RotateMesh`, `FitMesh`, `AddMesh`, `CopyMesh`, `ClearSurface`, `FindSurface`, `MeshesIntersect`, `LightMesh`, `MeshCullBox` |
| Brushes | done | `CreateBrush`, `LoadBrush`, `BrushColor/Alpha/Shininess/Texture/Blend/FX`, `PaintSurface/Entity/Mesh`, `GetSurfaceBrush`, `GetEntityBrush`, `GetBrushTexture`, `FreeBrush` |
| Background loading (ext) | done | `LoadMeshAsync` (also `LoadSceneAsync`), `LoadTextureAsync`, `LoadImageAsync` (with frames) and `LoadSoundAsync` return their handle at once and fill it when loaded; `EntityLoaded`, `TextureLoaded`, `ImageLoaded`, `SoundLoaded` (1 loaded, 0 loading, -1 failed), `PendingLoads` for loading screens |
| Asset management (ext) | done | Loading a file again shares what was decoded before (models through Fyrox; textures, images and sounds through a cache keyed by file and modification time), while every load keeps its own handle and settings. `ReloadAssets` and `WatchAssets` read changed art again while the game runs (textures, images, sounds and models, keeping settings); `AssetsReloaded`; `TextureName`, `ImageName`, `SoundName`, `ModelName`. Formats: PNG, JPG, TGA, BMP, GIF, DDS (also compressed); glTF, GLB, FBX, Fyrox scenes; WAV, OGG, MP3, FLAC |
| AI and pathfinding (ext) | done | Navigation meshes from a mesh (`CreateNavMesh`) or generated by sampling the level (`GenerateNavMesh`: slopes, steps, holes at walls), `NavMeshEntity` to see them, closest walkable point; `FindPath` (Fyrox A* with string pulling); grid A* with costs and diagonal moves (`CreatePathGrid`, `FindGridPath`, `TileMapPathGrid`); agents that walk, follow, turn and keep apart in `UpdateWorld` (`CreateAgent`, `AgentMoveTo`, `AgentFollow`); behaviour trees (selector, sequence, invert, conditions on parameters, actions the program carries out) built with commands or from text. Crowd steering (`AgentCrowd`: agents look ahead and choose velocities that miss each other). Off-mesh links for jumps, ladders and doors (`CreateNavLink`, one-way or two-way, with a cost and a jump arc; `NavLinkEnable`, `FreeNavLink`, `AgentOnLink`), used by `FindPath` and agents |
| Batching (ext) | done | `EntityBatch` draws a mesh's children merged per material (Fyrox static or dynamic batching). Identical classic materials are shared between entities, so copies can batch |
| Prefabs (ext) | done | `LoadPrefab` (model or saved scene, loaded once) and `CreatePrefab` (an entity built in the program) placed with `InstancePrefab`; file copies share meshes and materials (Fyrox instancing) and follow `ReloadAssets`; `PrefabName`, `EntityPrefab`, `FreePrefab` |
| Multiple scenes (ext) | done | `CreateScene`, `SetScene`, `CurrentScene`, `ShowScene`, `HideScene`, `SceneVisible`, `FreeScene`, `EntityScene`. Each is a Fyrox scene with its own entities, lights, physics and sound; later scenes draw over earlier ones with see-through backgrounds; `UpdateWorld` and `RenderWorld` handle every visible scene; using an entity of another scene is an error |
| Saved scenes (ext) | done | `SaveScene` (all top-level entities except cameras) and `SaveEntity` write a Fyrox scene file (`.rgs`); `LoadMesh`/`LoadScene` load it back as one entity, `FindChild` finds named entities and loaded models can be animated again. Models are saved by file name and loaded again from their files. Scenes made in the Fyrox editor load the same way |
| Models | done | `LoadMesh`/`LoadAnimMesh` for glTF (`.gltf`/`.glb`) and FBX, `LoaderMatrix`, `CopyEntity` (animated copies are independent), `FindChild`, `CountChildren`/`GetChild` into models, `MeshWidth/Height/Depth`. Old Blitz formats (`.b3d`, `.x`, `.3ds`) are deliberately not supported: export to glTF from Blender or any modern tool |
| Levels from Blender (ext) | done | `LoadLevelFile` reads a glTF file with lights (sun, point, spot, with colour, reach and cones matched to Blender's look), cameras, and custom properties (`EntityProperty`, `EntityPropertyInt`, `EntityPropertyFloat`, `EntityHasProperty`, `SetEntityProperty`, `CountEntityProperties`, `EntityPropertyName`); `CountLevelObjects` and `LevelObject` list objects, by `type` if wanted. Names starting `col_` or `hit_` make objects solid; properties set physics, collision types, pick modes and hidden objects and place prefabs. Add-on in `tools/blender`; guide in [BLENDER.md](BLENDER.md). Tested with exports from Blender 5.1. A Fyrox scene (`.rgs`) from Neptune Level Editor loads by the same rules: its lights and cameras become light and camera entities with their settings kept, and its nodes' properties and `col_`/`hit_` names are read ([EDITOR.md](EDITOR.md)) |
| Animation | done | `Animate` (loop, ping-pong, one-shot, with `transition` blending between sequences), `SetAnimTime`, `Animating`, `AnimTime`, `AnimSeq`, `AnimLength`, `ExtractAnimSeq`, `LoadAnimSeq` (animations from another file, retargeted), `CountAnimSeqs` (ext); keyframe animation of any entity with `AddAnimSeq`/`SetAnimKey` |
| Advanced animation (ext) | done | Layers on part of the skeleton or the whole model, mixed by weight (`AnimLayer`, `AnimLayerWeight`, `AnimLayerStop`); blend spaces on a line or a plane, kept in step (`CreateBlendSpace`, `BlendSpacePoint`, `AnimBlend`); state machines with parameter conditions, one-shot ends and crossfades (`AddAnimState`, `AddBlendState`, `AddAnimTransition`, `SetAnimParam`, `AnimParam`, `StartAnimStates`, `AnimStateName`); root motion (`AnimRootMotion`); property tracks for colour, alpha, emission, light, camera zoom, fog, texture offset and transforms on any entity (`CreateTrack`, `TrackKey`, `PlayTrack`, `SetTrackTime`, `TrackTime`, `TrackPlaying`, `FreeTrack`). Skinning runs on the GPU |
| Collisions | done | `EntityType`, `EntityRadius`, `EntityBox`, `Collisions` (methods 1–3, responses 1–3), `ResetEntity`, `EntityCollided`, `CountCollisions`, `CollisionX/Y/Z/NX/NY/NZ/Time/Entity/Surface/Triangle`, `ClearCollisions` |
| Picking | done | `EntityPickMode`, `LinePick`, `CameraPick`, `EntityPick`, `Picked*`, `CameraProject`, `Projected*`, `EntityVisible`, `EntityInView` |
| Debug drawing (ext) | done | `DebugLine`, `DebugBox`, `DebugSphere`, `DebugEntity` (bounding box), `DebugAxes`, `DebugNavMesh` for the next frame; `DebugPhysics` (every 3D and 2D collider) and `DebugCollisions` (`EntityRadius` and `EntityBox` shapes) every frame |
| Cloth and ropes (ext) | done | `CreateCloth` and `CreateRope` simulated on the CPU (Verlet points with stretch, shear and bend links, 120 steps a second): `ClothPin`, `ClothPinEdge` (to a place or to a moving entity: capes, curtains), `ClothUnpin`, `ClothStiffness`, `ClothDamping`, `ClothGravity`, `ClothWind` (the global wind, with gusts), `ClothCollide` (spheres, with friction), `ClothFloor`, `ClothTear`, `ResetCloth`, `ClothPointX/Y/Z`. Cloth is drawn on both sides |
| PBR materials (ext) | done | `EntityMetallic`, `EntityRoughness`, `EntityEmissive` (glows with `PostEffect EFFECT_BLOOM`; the glow is multiplied by the surface colour), and maps: `EntityMetallicMap`, `EntityRoughnessMap`, `EntityEmissiveMap`, `EntityAOMap`, `EntityLightmap` (second UV set), `EntityHeightMap` (parallax mapping). They apply to the standard material, not the classic shader |
| Sky box (ext) | done | `SkyBox` shows a cube map texture (`LoadTexture file, 128` or `CreateTexture w, h, 128`) as the sky |
| Lens effects (ext) | done | `SetDepthOfField` (each sample spreads by its own blur size, so sharp edges stay clean), `SetGodRays` and `SetLensFlare` (follow a light entity, blocked by the scene); also `builtin:dof`, `builtin:godrays`, `builtin:lensflare` |
| Camera effects (ext) | done | `CameraExposure` (fixed or automatic exposure on the HDR image), `CameraColorGrading` (a 256x16 colour lookup table per camera), `CameraToTexture` (a camera draws into a texture: monitors, mini-maps) |
| Decals (ext) | done | `CreateDecal` projects a texture onto the scene inside its box (sized with `ScaleEntity`); `EntityColor`, `EntityAlpha`, `EntityTexture`, `EntityNormalMap` work on decals |
| Particles (ext) | done | `CreateEmitter` with `EmitterRate`, `EmitterMax`, `EmitterLife`, `EmitterVelocity`, `EmitterSize`, `EmitterSpin`, `EmitterColor`, `EmitterFadeColor`, `EmitterGravity`, `EmitterShape` (sphere or box), `EmitterSoftness` (soft particles), `EmitterLocal`, `EmitterEnable`, `EmitterClear` |
| Reflection probes (ext) | done | `CreateReflectionProbe` takes a cube map picture of the scene (once, again with `UpdateProbe`, or every frame with `ProbeMode`); while the camera is inside its box (`ScaleEntity`) PBR surfaces reflect it and are lit by it. `ProbeRange`, `ProbeOffset`, `ProbeTexture` (the picture for custom shaders, e.g. the water shader's `envMap`). `EnvironmentLighting` chooses the `AmbientLight` colour (Blitz) or light and reflections from the sky box and probes |
| Baked lighting (ext) | done | `BakeLightmaps` ray-traces lightmaps on the CPU for meshes with the standard material: sky light (the `AmbientLight` colour) with ambient occlusion, one bounce of the lights in the colours of the surfaces, and optionally the direct light with shadows; edge-aware smoothing and padding; UVs from Fyrox's UV generator. `ClearLightmaps`. Baked light survives material changes and is dropped when a mesh's geometry changes |
| Storing parts of the world (ext) | done | `StoreEntity` takes an entity and its children out of the scene without freeing them, so they cost nothing each frame (a hidden entity does); `RestoreEntity` puts them back as they were; `EntityStored`. Commands on a stored entity fail with a message; `FreeEntity` and `ClearWorld` work. Not for entities with physics, cameras or mirrors |
| Level of detail (ext) | done | `EntityLOD` shows a child entity only within a distance range from the camera |
| Shaders (ext) | done | Shader objects from GLSL (`.glsl`, with directives, includes, `nb_surface` surface shaders and vertex hooks), WGSL (checked and translated by naga) or Fyrox `.shader` files, and 32 built-in shaders (`LoadShader("builtin:toon")`); `CreateShader`, `CompileShader` (GPU-compiled up front, errors with file and line, the last working version kept), `ValidateShader`, `ReloadShader`, `ShaderHotReload`, keywords (cached variants), typed parameters (`ShaderSetFloat` ... `ShaderSetMatrix4`, textures, cube maps, texture arrays as 3D textures, arrays), camera and light parameters, `ShaderApply`/`EntityMaterial`, per-entity overrides (`ShaderFloat`...), reflection and debugging (`ShaderDebug`, `ShaderGetUniforms`, ...). Compiled shaders are cached by text and materials shared, so one pipeline serves many objects. Geometry and compute stages do not exist in Fyrox's OpenGL renderer: those commands fail with an explanation. Documented in the shader manual, [SHADERS.md](SHADERS.md): ten chapters from a beginner's tour to a reference entry with an example for every command; `tools/check_doc_examples.py` checks its examples |
| Materials (ext) | done | `CreatePBRMaterial` (metallic/roughness and specular workflows, normal, AO, emissive, opacity, height/parallax, environment reflections, deferred lighting and shadows), `CreateToonMaterial` (toon, cel, flat, unlit, vertex-lit, pixel art, PS1, matcap, posterize, hand-painted; ramps, steps, inverted-hull outlines, rim light), `CreateWaterMaterial` (Gerstner waves, depth colour, foam, reflection, refraction), `CreateFoliageMaterial` and global wind, `CreateProceduralShader` (gradients, value/Perlin/simplex/Voronoi/cellular noise with fBm, patterns, distortion) and `BakeProceduralTexture` |
| Post-processing (ext) | done | Effect chains per camera (`LoadPostProcess`, `CreatePostProcess`, `AddPostProcess`, order, input, output to texture, resolution, HDR or LDR stage) run by a Fyrox render pass; 30 built-in effects (bloom, exposure, tone mapping, vignette, chromatic aberration, grain, colour grading, gamma/contrast/saturation/brightness, FXAA, outlines, pixelation, CRT, distortion, heat haze, blur, Gaussian and radial blur, sharpen, depth of field, motion blur, SSR, SSAO, height fog, underwater, rain, snow) and custom `nb_post` shaders |
| Shadow settings (ext) | done | `SetShadowMode` (off, hard, PCF soft), `SetShadowQuality`, `SetShadowResolution`, `SetShadowBias`, `SetShadowSoftness`, `SetShadowDistance`, `SetShadowCascadeCount` (1-3), `EnableShadows`, `DisableShadows`. Fyrox has no contact shadows and a fixed PCF size |
| Vertex data (ext) | done | `VertexData`/`VertexDataValue` give vertices four extra `vec4` values (shader locations 8-11) |
| Sprites | done | `CreateSprite`, `LoadSprite`, `SpriteViewMode` 1-4, `RotateSprite`, `ScaleSprite`, `HandleSprite`. Sprites face the first active camera. Sprites of the same size share one quad and are drawn as instances |
| Terrain | done | `CreateTerrain`, `LoadTerrain`, `TerrainSize`, `TerrainHeight`, `ModifyTerrain`, `TerrainY/X/Z`; `TerrainDetail` sets a triangle budget for level of detail (chunks of 16 squares drawn coarser with distance, edges stitched to coarser neighbours so no gaps open; collisions use the full grid); `TerrainShading` is accepted |
| Painted terrain (ext) | done | `TerrainLayer` gives a terrain a base and three painted texture layers; `PaintTerrain` (soft brush, world units), `ModifyTerrainLayer` (one grid point, for painting by height or slope), `LoadTerrainLayers` (splat map image), `TerrainLayerWeight`. Weights are stored per grid vertex and mixed by the classic shader |
| Terrain shader (ext) | done | `LoadShader("builtin:terrain")`: grass, rock, sand and snow per pixel from height and slope or the painted weights, with dry grass patches, layered rock, wet sand at `seaLevel` and detail that fades with distance; deferred, so the terrain receives shadows |
| Terrain erosion (ext) | done | `ErodeTerrain` (hydraulic: simulated raindrops carve gullies and ridges and carry soil into valleys; seeded, so repeatable) and `ThermalErodeTerrain` (slopes steeper than an angle crumble), working in the terrain's own proportions |
| Physics (ext) | done | `EntityPhysics` (dynamic, static, kinematic; box, sphere, capsule, cylinder, mesh, hull), `PhysicsGravity`, `EntityMass/Friction/Bounce/Damping/GravityScale`, `LockRotation`, `ApplyForce/Impulse/ImpulseAt/Torque/TorqueImpulse`, `SetVelocity`, `SetAngularVelocity`, `Velocity*`, `EntityTouching`, `CountContacts`, `ContactEntity`, `PhysicsRayCast`, `CreateBallJoint/HingeJoint/FixedJoint`, `FreeJoint`. Bodies move in world space: `EntityPhysics` detaches a parented entity in place, and `CopyEntity` gives copies their own body |
| Character controllers (ext) | done | One set of commands for 3D (`CreateCharController`, a capsule on an entity) and 2D (`CreateCharController2D`, box or capsule): sweep-based movement against physics bodies with acceleration, slopes up to a limit, steps, ground snapping, moving and turning platforms with momentum, ceilings, pushing dynamic bodies, coyote time, jump buffering, variable jump height, air jumps, wall slide and wall jump, crouching that waits for headroom, sprint, dash, knockback, one-way platforms and drop-through, fly and noclip modes, landing and impact events, `CharState` for animation, about 25 settings by name. Guide in [CHARACTERS.md](CHARACTERS.md) |
| Joint motors and ragdolls (ext) | done | `CreateSliderJoint`; for hinges and sliders, 3D and 2D: `JointMotor`/`JointMotor2D` (drive at a speed), `JointTarget`/`JointTarget2D` (spring to an angle or position), `JointMotorOff`, `JointLimits`, `JointAngle`, `JointPosition` (and the `2D` versions). 3D angles follow Blitz's left-hand rule (the direction of `EntityPitch`/`EntityRoll`). `CreateRagdoll` builds limbs and joints along an animated model's skeleton; `RagdollEnable`, `RagdollActive`, `RagdollImpulse`, `FreeRagdoll` |
| Sound | done | `LoadSound`, `FreeSound`, `LoopSound`, `SoundVolume/Pan/Pitch`, `PlaySound`, `PlayMusic`, `PlayCDTrack` (no CD audio: returns no channel), `StopChannel`, `PauseChannel`, `ResumeChannel`, `ChannelVolume/Pan/Pitch`, `ChannelPlaying`, `CreateListener`, `Load3DSound`, `EmitSound` (WAV, OGG, MP3, FLAC) |
| Settings and save data (ext) | done | `UserFolder` (the per-user application data folder on Windows, macOS and Linux), `SetSetting`, `GetSetting`, `GetSettingInt`, `GetSettingFloat`, `SettingExists`, `DeleteSetting`, `ClearSettings`, `SaveSettings`, `LoadSettings` (readable name = value files) |
| Audio buses and effects (ext) | done | `CreateAudioBus` (mixer channels feeding the master bus 0 or each other), `BusVolume`, `BusReverb` (decay, dry, brightness), `BusFilter` (low-pass, high-pass, band-pass, shelves, all-pass), `SoundBus`, `ChannelBus`; head-related 3D sound with `SetHRTF` (the game supplies the head-measurement file); guide in [SOUND.md](SOUND.md) |
| 2D text | done | `Text`, `Print`/`Write` on the screen in graphics mode, `Locate`, `Color`, `GetColor`, `ClsColor`, `Cls`, `LoadFont`, `SetFont`, `FreeFont`, `FontWidth/Height`, `StringWidth/Height` |
| 2D drawing | done | `Rect`, `Oval`, `Line`, `Plot`, `Origin`, `Viewport`, on the screen or into image and texture buffers. Drawing that follows with the same image or colour is one draw call |
| Movies | done | `OpenMovie`, `DrawMovie`, `MovieWidth`, `MovieHeight`, `MoviePlaying`, `CloseMovie` for animated GIFs, Motion-JPEG AVI files and folders of numbered images (30 fps); no sound track |
| 2D camera (ext) | done | `Camera2D x, y, zoom, rotation` puts all 2D screen drawing (images, tile maps, shapes, text) in world coordinates; `Camera2DEnable` switches back to screen coordinates for a HUD; `ScreenToWorld2DX/Y`, `WorldToScreen2DX/Y` |
| 2D drawing extras (ext) | done | `DrawImageEx` (rotation, scale and alpha about the image handle) |
| 2D animation (ext) | done | `CreateAnim2D` (loop, ping-pong or once), `Anim2DPlay`, `Anim2DPause`, `Anim2DFrame`, `Anim2DPlaying`, `DrawAnim2D`, `FreeAnim2D` |
| 2D particles (ext) | done | `CreateParticles2D` (an image or a soft dot), steady rate or bursts, life, speed and direction, gravity, size, spin and colour over life, `DrawParticles2D` (follows `Camera2D`) |
| 2D physics (ext) | done | Fyrox's 2D Rapier world in pixels (Y down, degrees clockwise): `CreateBody2D` (dynamic, static, kinematic), shapes `Body2DBox`, `Body2DCircle`, `Body2DCapsule`, `Body2DPolygon`, `Body2DSegment`, `TileMapBody2D`; `Body2DFriction/Bounce/Density/Sensor/FixedRotation/GravityScale/Damping`; `PositionBody2D`, `RotateBody2D`, `Body2DX/Y/Angle`, `SetVelocity2D`, `Body2DVX/VY`, `SetSpin2D`, `Body2DSpin`, `ApplyForce2D`, `ApplyImpulse2D`, `ApplyTorque2D`; `Body2DTouching`, `CountContacts2D`, `ContactBody2D`; `RayCast2D` with `RayCast2DX/Y/NX/NY`; hinge, fixed and slider joints (`CreateHingeJoint2D`, `CreateFixedJoint2D`, `CreateSliderJoint2D`, `FreeJoint2D`); `Physics2DGravity`, `Physics2DScale`; `DrawBody2D`, `DrawBodies2D` (outlines for debugging) |
| GUI widgets (ext) | done | Fyrox's retained UI: windows, panels (column, row, wrap, free), grids, scroll areas, borders, buttons, labels, text boxes, check boxes, sliders, progress bars, images (custom widgets with `GuiMouseX/Y`), drop-down lists, list views, trees, tabs, menus, popups, message boxes, file browsers and file dialogs, docking, number boxes, Vector3 editors, colour pickers and fields, curve editors. Events with `GuiClicked`/`GuiChanged`, values with `GuiValue`/`GuiText`/`GuiSelected`/`GuiVectorValue`, layout with `SetGuiPosition`/`SetGuiSize`/`GuiMargin`/`GuiAlign`/`GuiCell`, styling with `GuiColor`/`GuiTextColor`/`GuiFontSize`/`GuiTheme`/`GuiThemeColor`/`GuiThemeFontSize`. Sizes are logical pixels, so the GUI keeps its size on high-DPI screens |
| UI widgets (ext) | done | Immediate widgets in screen space: `UIButton`, `UICheckBox`, `UISlider`, `UITextBox`, `UIPanel`, `UILabel`, `UIHover` (the mouse is over the UI), `UITheme` |
| Tile maps (ext) | done | `CreateTileMap` (frames of an animation image), `SetTile`, `GetTile`, `DrawTileMap` (draws only what is on screen), `TileAt`, `TileMapCellX/Y`, `TileMapWidth/Height`, `TileMapImage`, `FreeTileMap`; autotiling with `TileMapTerrain` (16-frame or 47-frame "blob" layouts), `AutoTile`, `GetTileTerrain`; `SaveTileMap`/`LoadTileMap` (also reads CSV exported from the Tiled editor) |
| Maps from Tiled (ext) | done | `LoadTiledMap` reads `.tmx`, `.tmj` and `.world` files: orthogonal, isometric, staggered, hexagonal and infinite maps; CSV, XML, Base64, zlib and gzip layers; tilesets from one image or separate images, of any tile size, with offsets, alignment and grid scaling; flipped, rotated and animated tiles; tile layers, groups and picture layers with opacity, tint, parallax and repeat; objects with shapes, text, templates and custom properties; tile types, properties and collision shapes (`TiledMapBody2D`). `DrawTiledMap`, `DrawTiledObject`, `ShowTiledLayer`, `FreeTiledMap`. Not read: Zstandard layers (no decoder without a new crate), `.world` files that only have patterns, project-wide property types and 60-degree hexagon rotation. Guide in [TOOLS2D.md](TOOLS2D.md). Written to Tiled's published format; not yet tried with files saved by Tiled itself |
| Maps from other editors (ext) | done | `LoadTiledMap` also reads LDtk (levels, separate level files, multi-world projects, IntGrid values, stacked and auto-layer tiles, entities and fields), Ogmo Editor 3 (tile, grid, entity and decal layers), Sprite Fusion's JSON export, Pyxel Edit's JSON and XML tilemap exports, and tilemap layers of `.aseprite` files. `CountTiledLevels`, `TiledLevelName`, `TiledLevelX/Y`. Written from each editor's published format and source, with two exceptions that need checking against real files: the layer order of Sprite Fusion, and all of Pyxel Edit's format, which is not published. No files saved by these editors have been tried yet |
| Sprite sheets (ext) | done | `LoadSheetImage` reads the JSON of Aseprite and TexturePacker (hash or array, trimmed frames) into an ordinary animation image; tags and frame times with `Anim2DPlayTag`, `Anim2DTag`, `CountSheetTags`, `SheetTagName`, `SheetTagFirst`, `SheetTagLast`, `SheetFrameTime`. Not read: rotated frames. Written to those tools' published format; not yet tried with files saved by them |
| 2D images | done | `LoadImage`, `LoadAnimImage`, `CreateImage`, `CopyImage`, `GrabImage`, `DrawImage`, `DrawBlock`, `DrawImageRect`, `DrawBlockRect`, `TileImage`, `TileBlock`, `MidHandle`, `HandleImage`, `AutoMidHandle`, `MaskImage`, `ScaleImage`, `ResizeImage`, `RotateImage`, `TFormImage`, `TFormFilter`, `SaveImage`, `ImagesCollide`, `ImagesOverlap`, `ImageRectCollide`, `ImageRectOverlap`, `RectsOverlap` |
| Buffers and pixels | done | `SetBuffer`, `BackBuffer`, `FrontBuffer`, `ImageBuffer`, `TextureBuffer`, `GraphicsBuffer`, `LockBuffer`, `UnlockBuffer`, `ReadPixel(Fast)`, `WritePixel(Fast)`, `CopyPixel(Fast)`, `CopyRect`, `BufferWidth/Height`, `SaveBuffer` (the screen too), `LoadBuffer`; `Text` and every 2D drawing command work in image and texture buffers. Pixel reads come from image and texture buffers, not the screen |
| Keyboard and mouse | done | `KeyDown/Hit`, `GetKey`, `WaitKey`, `FlushKeys`, `MouseX/Y/Z`, `MouseXSpeed/YSpeed/ZSpeed` (raw motion, so mouse-look works with a hidden pointer), `MouseDown/Hit`, `GetMouse`, `WaitMouse`/`MouseWait`, `MoveMouse`, `HidePointer`, `ShowPointer`, `FlushMouse`, `CaptureMouse` (ext) |
| Joysticks and gamepads | done | `JoyType`, `JoyDown`, `JoyHit`, `GetJoy`, `WaitJoy`/`JoyWait`, `EnableDirectInput`, `DirectInputEnabled` (input always comes straight from the OS), `FlushJoy`, `JoyX/Y/Z/U/V`, `JoyPitch/Yaw/Roll`, `JoyHat`, `JoyXDir/YDir/ZDir/UDir/VDir`, `CountJoys` and `JOY_A`... constants (ext); Xbox, PlayStation and generic controllers through gilrs |
| Input actions (ext) | done | Named actions bound to keys, mouse buttons, gamepad buttons and sticks: `BindKey`, `BindMouse`, `BindJoy`, `BindJoyAxis`, `BindAction`, `ClearAction`, `ActionDown`, `ActionHit`, `ActionValue` (analog), `LastInput` and `InputName` for rebinding menus, `SaveActions`, `LoadActions`; `JoyRumble` vibrates gamepads |

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

* **The player has the last word.** `neptune run --max-quality low` (or the
  `NEPTUNE_MAX_QUALITY` environment variable, which also works for packaged
  games) is a ceiling: a program that asks for `QUALITY_ULTRA` gets low.
* **Automatic quality.** `AutoQuality 50` in a program, or `--quality auto`
  from the player, steps the level down while the game runs slower than
  that, and takes a step back if it did not help.
* **The fast graphics card is used.** On computers with two graphics cards
  the game asks for the fast one (Windows: the NVIDIA and AMD driver
  switches; Linux: `DRI_PRIME`; macOS does it unasked). `GraphicsCardName`
  says which card is drawing.
* Measured on an integrated card (AMD Radeon 780M, 1920x1080): the quality
  benchmark scene runs at 41 frames a second on ultra, 52 on high, 71 on
  medium and 89 on low.
* **Render scale.** `RenderScale 0.5` (or `--render-scale 0.5` from the
  player, as a ceiling) draws the 3D view at a lower resolution and
  stretches it over the window; text, 2D drawing and the GUI stay sharp. On
  the same integrated card and scene: ultra goes from 42 to 63 frames a
  second at 0.75 and 81 at 0.5; low from 97 to 155 and 265. First scene
  only; `AutoQuality` does not change it yet.
* Not done: nothing has been run on a graphics card from before 2015 yet.

## Tooling

| Item | Status |
|---|---|
| `neptune run/check/disasm/commands` CLI | done |
| `neptune build`: stand-alone game folder (executable + media; no console window on Windows) | done |
| `neptune bench`: runs the programs in `benchmarks/` (language, entities, models, physics, queries, rendering, graphics quality levels, a large world in chunks) and the island and beach demos uncapped (`neptune run --uncapped`), detects the machine, writes [docs/BENCHMARKS.md](BENCHMARKS.md) | done |
| `neptune plugins`: list loaded plugins and plugin folders | done |
| `neptune lsp`: language server (errors as you type, completion, hover, signature help, outline, go to definition, rename) | done |
| `neptune dap`: debugger for editors (Debug Adapter Protocol on a local TCP port): breakpoints with conditions (`lives < 2 And player\x > 10`) and hit counts, step in/over/out, pause (also in loops that never yield), call stack, locals, globals, `Dim` arrays, objects and vectors, watches and hover (`a\b`, `grid(1, i)`, `v[0]`), stops at `Stop` and on uncaught runtime errors. While stopped, a game's window stays alive and physics is frozen; sound and Fyrox animations and particles go on. Not yet: changing variables, `Input` (reads nothing under the debugger) | done |
| Runtime errors list the calls that led to them (`Gosub`s included; the middle of very deep stacks is left out) | done |
| `tools/gen_big_program.py`: writes a console program of any size (200,000 lines and more) to time the compiler on very large games | done |
| `tools/make_sample_2d.py`: writes the example Tiled maps, LDtk project, tileset and sprite sheet | done |
| `tools/blender`: Blender add-on (mark solid objects, set level properties, export) and `make_sample_level.py`, which builds the example level | done |
| VS Code extension: highlighting, language server, debug (F5), run (Ctrl+F5), build | done (`editors/vscode`) |
| Neptune IDE (`crates/neptune-ide`, guide in [IDE.md](IDE.md)): tabs, split view, multiple carets, folding, minimap, Blitz-style capitals and block indenting, live errors, completion, signature help and hover, outline, file tree, find in files, command palette, command reference, built-in manual, run, build, and the debugger. Uses `neptune lsp` and `neptune dap`, so it never links Fyrox. Not yet: word wrap, regular expressions in Find, replacing across files, changing variables while stopped | done |
| Neptune Level Editor (`crates/neptune-editor`, run `neptune-editor <game folder>`, guide in [EDITOR.md](EDITOR.md)): Fyrox's scene editor (`fyroxed_base`) from our own copy of its source (`vendor/fyroxed_base`, changes marked `// Neptune:`), as its own program, in Neptune IDE's colours (Neptune Deep and Daylight) with square buttons, panels arranged like Blender (the 3D view fills the left, object list over the Inspector on the right, asset browser, log and undo list as tabs under the view), Blender's tool keys (G, R, S, X; the camera on the arrow keys) and a starter level (ground, sun, spawn) when no scene is named, a short Inspector without the engine's own settings (`--full-inspector` shows them), plain object names, and panels called Objects, Level and Undo History; saves Fyrox scenes (`.rgs`) that `LoadLevelFile` reads. Play checks and runs the game's `main.nb` with `neptune`, or "Look Around" flies through the level with no program; `--play` presses it at start | started. Not done: a Neptune panel (type, solid, physics, prefab), opening it from the IDE, roomier rows, hiding the engine's built-in assets and script tools, buttons for type, solid and physics in the Inspector, packing `data/resources.registry` for scenes that use model and texture files (untested) |
| Documentation: [the manual](MANUAL.md) (25 chapters teaching the language and game making from the beginning), the [shader manual](SHADERS.md), a [language summary](LANGUAGE.md), and the [command reference](COMMANDS.md) with an example for every command (`docs/examples/*.txt`, put in by `tools/gen_reference.py`) | done |
| `tools/check_doc_examples.py`: checks every BASIC example in the docs with `neptune check`, runs the ones with an output box and compares what they print; `--gpu` compiles every documented shader | done |

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
* Multiplayer games use iroh and LAN discovery instead of DirectPlay; the
  commands and messages are Blitz3D's. There is no lobby dialog.
* `WBuffer`, `MeshCullBox` and `TerrainShading` are accepted; the GPU
  handles depth precision and culling itself. `TerrainDetail` uses chunked
  level of detail rather than Blitz3D's ROAM; `vertex_morph` is ignored.
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
| `examples/level.nb` | a level made in Blender (`courtyard.blend`): solid walls, lights, falling crates, a prefab, gems and an exit found by their properties, the level's own camera |
| `examples/editorlevel.nb` | a level saved as a scene by Neptune Level Editor: solid ground and walls, an invisible fence, a sun, a lamp, a falling crate, coins found by their properties, the level's own camera |
| `examples/animation.nb` | animated models, sequences, `FindChild` |
| `examples/collisions.nb` | gravity, sliding, ramps, jumping |
| `examples/charcontrol3d.nb` | 3D character controller: stairs, ramps, moving and turning platforms, crates, a low tunnel, wall jumps, a one-way shelf |
| `examples/charcontrol2d.nb` | 2D character controller: a slope, a lift, one-way platforms, a ladder, wall jumps, dash, a crate to push |
| `examples/physics.nb` | rigid bodies, impulses, hinge joint, ray cast |
| `examples/terrain.nb` | terrain, `TerrainY`, upright sprites, fog |
| `examples/sound.nb` | sounds, channels, 3D sound |
| `examples/draw2d.nb` | 2D shapes, animated sprites, pixel collisions |
| `examples/buffers.nb` | drawing into textures and images |
| `examples/picking.nb` | mouse picking, 3D-to-2D labels |
| `examples/gui.nb` | a settings app over a 3D scene: menu bar, tabbed window, drop-down, slider, colour picker, number box, tree, list, curve editor driving the scene, file dialog and message boxes |
| `examples/animstates.nb` | a fox driven by a state machine with a locomotion blend space, a look-around layer and a one-shot state; a lamp animated by property tracks |
| `examples/joints.nb` | windmill on a motor, door on a spring target, lift on a slider, limited arm, a fox that goes limp and gets up |
| `examples/platformer2d.nb` | 2D physics platformer: follow camera with zoom and tilt, animated player, crates, balls, a seesaw on a hinge, a settings panel with UI widgets |
| `examples/tiledmap.nb` | a platformer whose level, layers, coins, pits and goal come from a Tiled map and whose hero animates from sprite sheet tags |
| `examples/mapeditors.nb` | levels from different editors with one command: a Tiled map with parallax and animated tiles, an isometric Tiled map and an LDtk level |
| `examples/tilemap.nb` | 2D tile map: paint autotiled grass and water with the mouse, scroll, save and load |
| `examples/scenes.nb` | loading screen with background loading, then saving and loading the scene |
| `examples/ai.nb` | navigation mesh generated from the level, agents that walk around walls, a behaviour tree that patrols or chases |
| `examples/crowd.nb` | two crowds swapping sides without touching, and a runner that jumps a gap on a link; Space shuts the link |
| `examples/island.nb` | an island generated from noise and worn by erosion, drawn by the terrain shader: beaches, rock, snow, forests, palms, grass and boulders planted as prefabs and batched with shadows, see-through water from the water shader, post-process haze, drifting clouds; orbit the island or walk and fly over it |
| `examples/beach.nb` | a tropical beach: see-through water (depth from the depth buffer) with caustics, foam, breaking lines and surge, terrain shader sand that darkens where wet, post-process haze, drifting clouds, palms, rocks and grass |
| `examples/bigworld.nb` | 40,000 towers in 100 districts, the far ones stored with `StoreEntity` and restored as you fly near; Tab hides them instead, to compare frame rates |
| `examples/worlds.nb` | painted terrain with level of detail, prefabs placed many times, a second scene drawn over the first |
| `examples/lensfx.nb` | depth of field, god rays and a lens flare from one sun, each on its own key, with the focus on the arrow keys |
| `examples/shaders.nb` | every ready-made material and custom GLSL/WGSL shaders side by side, with post-processing effects on keys 1-7 |
| `examples/shadertests.nb` | the shader acceptance tests: 20 GPU checks with screenshot pixel comparisons (`neptune run --uncapped`) |
| `examples/soundfx.nb` | audio buses: music and effects volumes, room, hall and cave reverb, underwater and radio filters |
| `examples/probes.nb` | reflection probes: chrome, gold and brushed metal reflecting a courtyard; probe on/off and every-frame updates |
| `examples/lightbake.nb` | baked lighting: soft sky light, shade under a roof and light bounced off a red wall; B toggles it |
| `examples/lights.nb` | spot lights through window bars and stained glass (cookies), a lantern glowing in hazy air (scattering) |
| `examples/debugdraw.nb` | debug drawing: physics colliders, collision spheres, a navigation mesh, an entity's axes and box, lines and boxes of your own |
| `examples/essentials.nb` | game essentials: input actions with keys or gamepad and a rebindable dash, gems that pop in with tweens, sparks (2D particles), camera shake, rumble, fades and a saved best score |
| `examples/cloth.nb` | cloth and ropes: a flag in the wind, a curtain a ball pushes through, a cape on a walking figure, a lantern on a rope, a sheet dropped onto a rock (Space) |
| `examples/rendering.nb` | sky box, PBR materials, glow, parallax, decal, soft particles, level of detail, a camera on a screen, automatic exposure, a custom shader |
| `examples/movie.nb` | makes a movie from images, then plays it over a 3D scene |
| `examples/games/starcourier.nb` | **complete 2D game**: shooter with states, pickups, particles, sound, gamepad, high score, attract mode |
| `examples/games/orbrun.nb` | **complete 3D game**: animated fox on terrain, orbs, hazards, follow camera, HUD, best time, attract mode |
| `examples/blending.nb` | see-through surfaces (alpha, vertex alpha, multiply, additive, particles) in front of a very large solid mesh |
| `examples/games/dirtbike.nb` | **racing demo**: motocross in a sunset canyon on character controllers, with jumps, air tricks, rivals and laps |
| `examples/games/jetski.nb` | **racing demo**: jet skis round a lagoon, with animated waves, floating ramps, buoys and rivals |
| `examples/games/quadrace.nb` | **racing demo**: four-wheelers over a snowy mountain pass, with forests, snowfall and rivals |
| `examples/games/skyrace.nb` | **racing demo**: biplanes round a volcano island on `CHAR_FLY` controllers, with boost hoops, barrel rolls and rivals |
| `examples/games/retro64.nb` | toolkit shared by the racing demos (included by them, does nothing alone): baked vertex lighting, track building, vehicles on character controllers, rider AI, chunky HUD |
