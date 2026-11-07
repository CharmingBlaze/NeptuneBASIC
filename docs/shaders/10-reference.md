# 10. Command reference

[Contents](../SHADERS.md) · Previous: [9. Debugging and performance](09-debugging.md)

Every command of the shader system, by area, with its syntax, what each
argument means, what it returns, and an example. All of them are
extensions (ext): Blitz3D had no shaders.

**How to read the syntax.** `result = Command(a, b [, c])` returns a
value; `Command a, b [, c]` does not. Arguments in `[ ]` can be left out,
and their default is given. Arguments ending in `$` are text; all others
are numbers. Colours are red, green, blue (and alpha) from 0 to 255.

| Area | Commands |
|---|---|
| [Creating and compiling shaders](#creating-and-compiling-shaders) | `LoadShader`, `CreateShader`, `CompileShader`, `ValidateShader`, `ShaderValidate`, `ReloadShader`, `ShaderHotReload`, `ShaderSource`, `ShaderVertexSource`, `ShaderFragmentSource`, `ShaderGeometrySource`, `ShaderComputeSource`, `ShaderGetSource`, `ShaderSetKeyword`, `ShaderClone`, `ShaderSave`, `FreeShader`, `ShaderExists`, `ShaderCompiled`, `ShaderError`, `ShaderCompileError`, `ShaderLog` |
| [Using shaders](#using-shaders) | `ShaderApply`, `EntityMaterial`, `EntityShader`, `ShaderEnable`, `ShaderDisable` |
| [Shader parameters](#shader-parameters) | `ShaderSetFloat`, `ShaderSetInt`, `ShaderSetBool`, `ShaderSetVector2`, `ShaderSetVector3`, `ShaderSetVector4`, `ShaderSetColor`, `ShaderSetMatrix3`, `ShaderSetMatrix4`, `ShaderSetBuffer`, `ShaderSetParameter`, `ShaderGetParameter`, `ShaderSetTexture`, `ShaderSetCubeMap`, `ShaderSetTextureArray`, `ShaderSetTime`, `ShaderSetResolution`, `ShaderSetCamera`, `ShaderSetLight`, `ShaderSetMaterial`, `ShaderParameterExists`, `ShaderParameterCount`, `ShaderParameterName`, `ShaderParameterType` |
| [Per-entity values and vertex data](#per-entity-values-and-vertex-data) | `ShaderFloat`, `ShaderVector`, `ShaderTexture`, `VertexData`, `VertexDataValue` |
| [Shader debugging](#shader-debugging) | `ShaderDebug`, `ShaderGetUniforms`, `ShaderGetAttributes`, `ShaderGetBindings`, `ShaderGetResourceInfo`, `ShaderGetCompilationStatus`, `ShaderGetBackend` |
| [PBR materials](#pbr-materials) | `CreatePBRMaterial`, `SetAlbedo`, `SetAlbedoMap`, `SetMetallic`, `SetMetallicMap`, `SetRoughness`, `SetRoughnessMap`, `SetNormalMap`, `SetAOMap`, `SetEmissive`, `SetEmissiveMap`, `SetOpacity`, `SetOpacityMap`, `SetHeightMap`, `SetParallaxScale`, `SetEnvironmentMap`, `SetReflectionIntensity`, `SetFresnel`, `SetMaterialWorkflow` |
| [Toon and stylized materials](#toon-and-stylized-materials) | `CreateToonMaterial`, `SetShadingMode`, `SetToonSteps`, `SetToonRamp`, `SetLightingRamp`, `SetToonOutline`, `SetOutlineColor`, `SetOutlineWidth`, `SetRimLighting`, `SetRimColor`, `SetRimPower`, `SetVertexLighting`, `SetFlatShading`, `SetPixelShading` |
| [Water, wind and foliage](#water-wind-and-foliage) | `CreateWaterMaterial`, `SetWaterWaves`, `SetWaterSpeed`, `SetWaterColor`, `SetWaterDepth`, `SetWaterFoam`, `SetWaterReflection`, `SetWaterRefraction`, `CreateFoliageMaterial`, `SetFoliageWind`, `SetWindDirection`, `SetWindStrength` |
| [Procedural materials](#procedural-materials) | `CreateProceduralShader`, `SetGradient`, `SetGradientColor`, `SetGradientDirection`, `SetNoiseType`, `SetNoiseScale`, `SetNoiseOctaves`, `SetNoisePersistence`, `SetNoiseLacunarity`, `SetNoiseSpeed`, `SetNoiseStrength`, `SetDistortion`, `SetProceduralSeed`, `SetPattern`, `SetPatternScale`, `SetPatternColor`, `SetPatternRotation`, `BakeProceduralTexture` |
| [Post-processing](#post-processing) | `LoadPostProcess`, `CreatePostProcess`, `AddPostProcess`, `RemovePostProcess`, `SetPostProcess`, `FreePostProcess`, `SetPostProcessParameter`, `SetPostProcessEnabled`, `SetPostProcessOrder`, `PostProcessCamera`, `PostProcessPriority`, `PostProcessInput`, `PostProcessOutput`, `PostProcessResolution`, `PostProcessShader`, `SetRainEffect`, `SetSnowEffect`, `SetAtmosphericFog`, `SetUnderwaterEffect` |
| [Shadows](#shadows) | `SetShadowMode`, `EnableShadows`, `DisableShadows`, `SetShadowQuality`, `SetShadowResolution`, `SetShadowDistance`, `SetShadowCascadeCount`, `SetShadowBias`, `SetShadowSoftness`, `LightShadows` |
| [Constants](#constants) | every constant these commands use |

Related commands documented elsewhere: reflection probes
(`CreateReflectionProbe`, `ProbeTexture`, ...) and `EnvironmentLighting`,
`PostEffect`, `GraphicsQuality`, `CameraExposure`, `CameraColorGrading`,
in the [command reference](../COMMANDS.md).

---

## Creating and compiling shaders

Explained in [chapter 4](04-writing-shaders.md) and [chapter 5](05-shader-language.md).

### LoadShader

```
shader = LoadShader(file$)
```

Loads a shader and compiles it. `file$` is a GLSL file (`.glsl`), a WGSL
file (`.wgsl`), a Fyrox shader file (`.shader`), or a built-in shader
written `"builtin:name"` (see [chapter 3](03-materials.md#built-in-shaders)).

**Returns** the shader. If it has an error, the shader is still returned:
`ShaderCompiled` is 0 and `ShaderError` says why. Returns 0 only if the
file cannot be read.

```basic
glow = LoadShader("shaders/glow.glsl")
If ShaderCompiled(glow) = 0 Then RuntimeError ShaderError(glow)
holo = LoadShader("builtin:hologram")
```

### CreateShader

```
shader = CreateShader([source$ = "", language$ = "glsl"])
```

Makes a shader from text. `language$` is `"glsl"`, `"wgsl"` or `"fyrox"`.
The shader is compiled by `CompileShader`, or when it is first used.
With no source, give it one later with `ShaderSource`.

**Returns** the shader.

```basic
red = CreateShader("//! fragment" + Chr(10) + "void main() { FragColor = vec4(1.0, 0.0, 0.0, 1.0); }")
CompileShader red
empty = CreateShader()
```

### CompileShader

```
ok = CompileShader(shader)
```

Compiles the shader's current source, on the GPU when the window is
open. On failure the last version that compiled stays in use.

**Returns** 1 on success; 0 on failure, with the file, line and message in
`ShaderError`.

```basic
s = CreateShader()
ShaderSource s, "//! fragment" + Chr(10) + "void main() { FragColor = vec4(1.0); }"
If CompileShader(s) = 0 Then DebugLog ShaderError(s)
```

### ValidateShader

```
ok = ValidateShader(shader)
```

Checks the shader's current source (including a GPU compile once the
window is open) **without** using it: the version in use does not change.

**Returns** 1 if it is valid; 0 if not, with the reason in `ShaderError`.

```basic
s = LoadShader("shaders/glow.glsl")
ShaderSource s, "//! fragment" + Chr(10) + "void main() { FragColor = vec4(oops); }"
If ValidateShader(s) = 0 Then DebugLog "edit rejected: " + ShaderError(s)
```

### ShaderValidate

```
ok = ShaderValidate(shader)
```

The same as `ValidateShader`.

```basic
s = LoadShader("shaders/glow.glsl")
If ShaderValidate(s) Then DebugLog "fine"
```

### ReloadShader

```
ok = ReloadShader(shader)
```

Reads the shader's file again and compiles it. For a shader loaded from a
file.

**Returns** 1 on success, 0 on failure (the old version stays in use).

```basic
s = LoadShader("shaders/glow.glsl")
If KeyHit(KEY_F5) Then ReloadShader s
```

### ShaderHotReload

```
ShaderHotReload shader [, enable = 1]
```

With `enable` 1, the shader is compiled again whenever its file, or a
file it includes, changes. A broken edit keeps the previous version and
is reported once in the console and in `ShaderError`. 0 turns it off.

```basic
s = LoadShader("shaders/water.glsl")
ShaderHotReload s
```

### ShaderSource

```
ShaderSource shader, source$
```

Replaces the whole source of the shader. Compile it again with
`CompileShader`.

```basic
s = CreateShader()
ShaderSource s, "uniform float level = 0.5;" + Chr(10) + "//! fragment" + Chr(10) + "void main() { FragColor = vec4(vec3(level), 1.0); }"
CompileShader s
```

### ShaderVertexSource

```
ShaderVertexSource shader, source$
```

Replaces the vertex stage of the shader (the text after `//! vertex`).
Compile it again with `CompileShader`.

```basic
s = LoadShader("shaders/ball.glsl")
ShaderVertexSource s, "void nb_displace(inout vec3 p, inout vec3 n, vec2 uv) { p *= 1.2; }"
CompileShader s
```

### ShaderFragmentSource

```
ShaderFragmentSource shader, source$
```

Replaces the fragment stage of the shader (the text after `//! fragment`).
Compile it again with `CompileShader`.

```basic
s = CreateShader()
ShaderFragmentSource s, "void nb_surface(inout NbSurface s) { s.albedo = vec3(0.2, 0.6, 1.0); }"
CompileShader s
```

### ShaderGeometrySource

```
ShaderGeometrySource shader, source$
```

Always fails with an explanation: Fyrox's OpenGL 3.3 renderer has no
geometry stage. Build geometry with the mesh commands (`CreateMesh`,
`AddVertex`, `AddTriangle`), or move vertices in a vertex shader. It
exists so that programs written for other engines get a clear message.

### ShaderComputeSource

```
ShaderComputeSource shader, source$
```

Always fails with an explanation: Fyrox's renderer has no compute stage.
Compute into a texture with a post-processing effect and
`PostProcessOutput`, or on the CPU.

### ShaderGetSource

```
text$ = ShaderGetSource(shader [, stage = 0])
```

**Returns** the shader's source. `stage` 0 is the source as written, 1
the vertex program and 2 the fragment program the GPU compiles (with
everything NeptuneBASIC adds), 3 the Fyrox shader text.

```basic
toon = LoadShader("builtin:toon")
DebugLog ShaderGetSource(toon)        ; a good starting point for your own
DebugLog ShaderGetSource(toon, 2)     ; what the GPU sees
```

### ShaderSetKeyword

```
ShaderSetKeyword shader, keyword$ [, enable = 1]
```

Turns a keyword on (1) or off (0). An enabled keyword becomes
`#define keyword 1` at the top of the shader, and directives ending in
`if keyword` apply. The shader is compiled again; each set of keywords is
compiled once and reused. See [keywords](04-writing-shaders.md#lesson-13-keywords).

```basic
crystal = LoadShader("shaders/crystal.glsl")
ShaderSetKeyword crystal, "GLOW", True
ShaderSetKeyword crystal, "GLOW", False
```

### ShaderClone

```
copy = ShaderClone(shader)
```

**Returns** a copy of the shader with its own parameter values. It shares
the compiled program, so it is cheap. Use it for variations of a
material.

```basic
red = CreatePBRMaterial()
SetAlbedo red, 220, 30, 30
blue = ShaderClone(red)
SetAlbedo blue, 30, 60, 220
```

### ShaderSave

```
ok = ShaderSave(shader, file$)
```

Saves the shader. A `.shader` or `.ron` file gets the Fyrox shader it was
turned into; any other file gets its source.

**Returns** 1 on success.

```basic
s = LoadShader("builtin:toon")
ShaderSave s, "toon.glsl"           ; the source
ShaderSave s, "toon_fyrox.shader"   ; the Fyrox shader
```

### FreeShader

```
FreeShader shader
```

Frees the shader. Entities drawn with it go back to their normal
materials. Materials from `CreatePBRMaterial` and the other `Create...`
commands are freed the same way.

```basic
s = LoadShader("builtin:hologram")
FreeShader s
```

### ShaderExists

```
exists = ShaderExists(shader)
```

**Returns** 1 if `shader` is a shader that exists (not freed), else 0.

```basic
s = LoadShader("builtin:fire")
FreeShader s
If ShaderExists(s) = 0 Then DebugLog "gone"
```

### ShaderCompiled

```
ok = ShaderCompiled(shader)
```

**Returns** 1 if the last compile of the shader succeeded.

```basic
s = LoadShader("shaders/glow.glsl")
If Not ShaderCompiled(s) Then DebugLog ShaderError(s)
```

### ShaderError

```
message$ = ShaderError(shader)
```

**Returns** the error of the shader's last compile or validation, with
file and line, or `""` if there was none. See
[reading an error](09-debugging.md#reading-an-error).

```basic
s = LoadShader("shaders/glow.glsl")
If ShaderError(s) <> "" Then RuntimeError ShaderError(s)
```

### ShaderCompileError

```
message$ = ShaderCompileError(shader)
```

The same as `ShaderError`.

```basic
s = LoadShader("shaders/glow.glsl")
DebugLog ShaderCompileError(s)
```

### ShaderLog

```
log$ = ShaderLog(shader)
```

**Returns** the shader's log: each compile (with its time), warning,
error and hot reload, one per line.

```basic
s = LoadShader("shaders/glow.glsl")
DebugLog ShaderLog(s)          ; "compiled in 71.1 ms (1 passes)"
```

## Using shaders

### ShaderApply

```
ShaderApply shader, entity
```

Draws the entity with the shader (any shader or material). Shader 0 puts
it back to its normal materials. The same as `EntityMaterial entity,
shader`.

```basic
holo = LoadShader("builtin:hologram")
ball = CreateSphere(32)
ShaderApply holo, ball
ShaderApply 0, ball         ; normal again
```

### EntityMaterial

```
EntityMaterial entity, material
```

Draws the entity with a material from `CreatePBRMaterial`,
`CreateToonMaterial` and the other `Create...` commands, or any shader.
Material 0 puts it back to its normal materials.

```basic
gold = CreatePBRMaterial()
SetMetallic gold, 1
ring = CreateCylinder(32)
EntityMaterial ring, gold
```

### EntityShader

```
EntityShader entity, shader
```

The same as `EntityMaterial`; the name of the first version of the shader
commands, kept for older programs.

```basic
wave = LoadShader("examples/media/wave.shader")
mesh = CreateCube()
EntityShader mesh, wave
```

### ShaderEnable

```
ShaderEnable shader
```

Turns a shader back on after `ShaderDisable`.

```basic
s = LoadShader("builtin:forcefield")
ShaderDisable s
ShaderEnable s
```

### ShaderDisable

```
ShaderDisable shader
```

Turns a shader off: its entities are drawn with their normal materials
until `ShaderEnable`. Their link to the shader is kept.

```basic
xray = LoadShader("shaders/xray.glsl")
If KeyDown(KEY_X) Then ShaderEnable xray Else ShaderDisable xray
```

## Shader parameters

Explained in [chapter 4](04-writing-shaders.md#lesson-4-parameters) and
[chapter 5](05-shader-language.md#parameters). Each command sets a
parameter of the shader by its name; every entity using the shader sees
the new value. A name the shader does not declare is an error.

### ShaderSetFloat

```
ShaderSetFloat shader, name$, value
```

Sets a `float` parameter.

```basic
s = LoadShader("builtin:hologram")
ShaderSetFloat s, "speed", 3.5
```

### ShaderSetInt

```
ShaderSetInt shader, name$, value
```

Sets an `int` (or `uint`) parameter.

```basic
s = LoadShader("shaders/levels.glsl")
ShaderSetInt s, "steps", 4
```

### ShaderSetBool

```
ShaderSetBool shader, name$, value
```

Sets a `bool` parameter: 0 is false, anything else true.

```basic
s = LoadShader("shaders/door.glsl")
ShaderSetBool s, "locked", True
```

### ShaderSetVector2

```
ShaderSetVector2 shader, name$, x, y
```

Sets a `vec2` parameter.

```basic
s = LoadShader("builtin:unlit")
ShaderSetVector2 s, "uvScale", 4, 4      ; repeat the texture 4 times
```

### ShaderSetVector3

```
ShaderSetVector3 shader, name$, x, y, z
```

Sets a `vec3` parameter.

```basic
s = LoadShader("shaders/explosion.glsl")
ShaderSetVector3 s, "center", 0, 1, 0
```

### ShaderSetVector4

```
ShaderSetVector4 shader, name$, x, y, z, w
```

Sets a `vec4` parameter.

```basic
s = LoadShader("builtin:unlit")
ShaderSetVector4 s, "albedo", 1, 0.5, 0.2, 1
```

### ShaderSetColor

```
ShaderSetColor shader, name$, red, green, blue [, alpha = 255]
```

Sets a colour parameter (`vec3` or `vec4`) from 0-255 values; the shader
gets them as 0-1.

```basic
s = LoadShader("builtin:forcefield")
ShaderSetColor s, "color", 80, 255, 120
```

### ShaderSetMatrix3

```
ShaderSetMatrix3 shader, name$, values$
```

Sets a `mat3` parameter from 9 numbers in a string, column by column.

```basic
s = LoadShader("shaders/colormix.glsl")
ShaderSetMatrix3 s, "mixer", "0.39,0.35,0.27, 0.77,0.69,0.53, 0.19,0.17,0.13"   ; sepia
```

### ShaderSetMatrix4

```
ShaderSetMatrix4 shader, name$, values$
```

Sets a `mat4` parameter from 16 numbers in a string, column by column.

```basic
s = LoadShader("shaders/projector.glsl")
ShaderSetMatrix4 s, "projector", "1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1"
```

### ShaderSetBuffer

```
ShaderSetBuffer shader, name$, values$
```

Sets an array parameter (`float[n]`, `vec4[n]`, ...) from the numbers in
`values$`, separated by commas or spaces. For arrays of vectors, give
their numbers one after another. Each call replaces the whole array.

```basic
s = LoadShader("shaders/blur.glsl")
ShaderSetBuffer s, "weights", "0.06, 0.24, 0.4, 0.24, 0.06"
```

### ShaderSetParameter

```
ShaderSetParameter shader, name$, value$
```

Sets a parameter of any type from text: numbers separated by commas or
spaces, or for a texture parameter a texture handle. One number fills
every part of a vector.

```basic
s = LoadShader("builtin:hologram")
ShaderSetParameter s, "color", "1, 0.3, 0.8"
ShaderSetParameter s, "speed", "2"
```

### ShaderGetParameter

```
value$ = ShaderGetParameter(shader, name$)
```

**Returns** a parameter's value as text: numbers separated by commas, or
a texture handle (0 if none is set).

```basic
s = LoadShader("builtin:water")
DebugLog ShaderGetParameter(s, "waveHeight")     ; "0.25"
```

### ShaderSetTexture

```
ShaderSetTexture shader, name$, texture
```

Sets a texture parameter (`sampler2D` or `sampler3D`). 0 removes it (it
then reads as its fallback: white, black or a flat normal).

```basic
s = LoadShader("builtin:unlit")
ShaderSetTexture s, "albedoMap", LoadTexture("examples/media/checker.png")
```

### ShaderSetCubeMap

```
ShaderSetCubeMap shader, name$, texture
```

Sets a `samplerCube` parameter to a cube map: a texture loaded with flag
128, or `ProbeTexture(probe)`.

```basic
s = LoadShader("builtin:glass")
ShaderSetCubeMap s, "envMap", LoadTexture("sky.png", 128)
```

### ShaderSetTextureArray

```
ShaderSetTextureArray shader, name$, textures$
```

Stacks several textures of the same size (handles separated by commas)
into one array texture for the parameter. Read layer `n` in GLSL with
`nb_textureArray(name, uv, n)`.

```basic
grass = LoadTexture("grass.png")
dirt = LoadTexture("dirt.png")
rock = LoadTexture("rock.png")
s = LoadShader("shaders/layers.glsl")
ShaderSetTextureArray s, "layers", Str(grass) + "," + Str(dirt) + "," + Str(rock)
```

### ShaderSetTime

```
ShaderSetTime shader [, time = -1]
```

Sets the `nb_time` the shader sees to `time` seconds, for pausing,
replays or slow motion. -1 goes back to the running clock.

```basic
s = LoadShader("builtin:fire")
ShaderSetTime s, 2.5       ; frozen at 2.5 seconds
ShaderSetTime s            ; running again
```

### ShaderSetResolution

```
ShaderSetResolution shader [, width = 0, height = 0]
```

Sets the `nb_resolution` the shader sees, in pixels. 0, 0 goes back to
the window size. Useful when a shader draws for a texture of another
size.

```basic
s = LoadShader("shaders/pixel.glsl")
ShaderSetResolution s, 320, 180
```

### ShaderSetCamera

```
ShaderSetCamera shader, camera
```

Gives the shader the camera's position and matrices, in the parameters it
declares: `nb_cameraPosition` (vec3), `nb_viewMatrix`,
`nb_projectionMatrix` and `nb_viewProjection` (mat4). Call it again when
the camera moves. The values are in Fyrox's mirrored space (see
[coordinates](05-shader-language.md#coordinates-in-shaders)).

```basic
camera = CreateCamera()
s = LoadShader("shaders/radar.glsl")
ShaderSetCamera s, camera
```

### ShaderSetLight

```
ShaderSetLight shader, light [, index = 0]
```

Gives the shader the light's position, direction (towards the light),
colour and range, in the parameters it declares: `nb_lightPosition`,
`nb_lightDirection`, `nb_lightColor` (vec3) and `nb_lightRange` (float).
If they are arrays, `index` picks the element.

```basic
lamp = CreateLight(LIGHT_POINT)
s = LoadShader("shaders/glowlamp.glsl")
ShaderSetLight s, lamp
```

### ShaderSetMaterial

```
count = ShaderSetMaterial(shader, material)
```

Copies into `shader` every parameter value of `material` that both
declare.

**Returns** how many were copied.

```basic
old = LoadShader("builtin:toon")
ShaderSetColor old, "albedo", 255, 100, 50
newer = LoadShader("builtin:cel")
n = ShaderSetMaterial(newer, old)       ; newer is orange too
```

### ShaderParameterExists

```
exists = ShaderParameterExists(shader, name$)
```

**Returns** 1 if the shader declares a parameter called `name$`.

```basic
s = LoadShader("builtin:water")
If ShaderParameterExists(s, "foamAmount") Then ShaderSetFloat s, "foamAmount", 1
```

### ShaderParameterCount

```
count = ShaderParameterCount(shader)
```

**Returns** how many parameters the shader declares, textures and the
automatic values it uses included.

```basic
s = LoadShader("builtin:toon")
DebugLog ShaderParameterCount(s)
```

### ShaderParameterName

```
name$ = ShaderParameterName(shader, index)
```

**Returns** the name of parameter number `index` (0 to
`ShaderParameterCount - 1`).

```basic
s = LoadShader("builtin:toon")
For i = 0 To ShaderParameterCount(s) - 1
    DebugLog ShaderParameterName(s, i)
Next
```

### ShaderParameterType

```
type$ = ShaderParameterType(shader, name$)
```

**Returns** the parameter's type: `"float"`, `"int"`, `"bool"`, `"vec3"`,
`"mat4"`, `"float[16]"`, `"sampler2D"`, `"samplerCube"`, ...

```basic
s = LoadShader("builtin:toon")
DebugLog ShaderParameterType(s, "rimColor")    ; "vec3"
```

## Per-entity values and vertex data

### ShaderFloat

```
ShaderFloat entity, name$, value
```

Sets a `float` parameter for this entity only, overriding the shader's
value. The entity gets a material of its own. See
[one entity, different settings](03-materials.md#one-entity-different-settings).

```basic
wood = LoadShader("builtin:diffuse")
crate = CreateCube()
EntityShader crate, wood
ShaderFloat crate, "shininess", 8
```

### ShaderVector

```
ShaderVector entity, name$, x, y [, z = 0, w = 0]
```

Sets a vector or colour parameter for this entity only. Colours are 0-1
here.

```basic
paint = LoadShader("builtin:diffuse")
car = CreateCube()
EntityShader car, paint
ShaderVector car, "albedo", 1, 0.2, 0.2, 1
```

### ShaderTexture

```
ShaderTexture entity, name$, texture
```

Sets a texture parameter for this entity only. An entity given
`ProbeTexture(probe)` this way is left out of the probes' own pictures.

```basic
poster = LoadShader("builtin:unlit")
frame = CreateCube()
EntityShader frame, poster
ShaderTexture frame, "albedoMap", LoadTexture("examples/media/ship.png")
```

### VertexData

```
VertexData surface, index, slot, x [, y = 0, z = 0, w = 0]
```

Stores four extra numbers in vertex `index` of the surface, in `slot`
0-3. Shaders read them as `nb_data0`-`nb_data3` (fragment stage) or
`vertexData0`-`vertexData3` (vertex stage, locations 8-11).

```basic
mesh = CreateMesh()
surf = CreateSurface(mesh)
v = AddVertex(surf, 0, 0, 0)
VertexData surf, v, 0, Rnd(0, 6.28), 1      ; a random phase and a height
```

### VertexDataValue

```
value# = VertexDataValue(surface, index, slot, component)
```

**Returns** one of the numbers stored with `VertexData` (`component` 0-3
is x, y, z, w).

```basic
mesh = CreateMesh()
surf = CreateSurface(mesh)
v = AddVertex(surf, 0, 0, 0)
VertexData surf, v, 0, 2.5
DebugLog VertexDataValue(surf, v, 0, 0)     ; 2.5
```

## Shader debugging

Explained in [chapter 9](09-debugging.md#looking-inside-a-shader).

### ShaderDebug

```
report$ = ShaderDebug(shader)
```

**Returns** a report on the shader: language, backend, status, passes,
parameters, bindings, attributes, files, keywords, resource use and the
last error.

```basic
s = LoadShader("builtin:water")
DebugLog ShaderDebug(s)
```

### ShaderGetUniforms

```
list$ = ShaderGetUniforms(shader)
```

**Returns** the shader's parameters as `name:type`, separated by commas.

```basic
s = LoadShader("builtin:rim")
DebugLog ShaderGetUniforms(s)   ; "albedo:vec4,rimColor:vec3,rimPower:float,..."
```

### ShaderGetAttributes

```
list$ = ShaderGetAttributes(shader)
```

**Returns** the vertex attributes the shader reads as `name@location`,
separated by commas.

```basic
s = LoadShader("builtin:toon")
DebugLog ShaderGetAttributes(s)  ; "vertexPosition@0,vertexTexCoord@1,..."
```

### ShaderGetBindings

```
list$ = ShaderGetBindings(shader)
```

**Returns** the shader's textures and uniform blocks as
`name:kind@binding`, separated by commas.

```basic
s = LoadShader("builtin:unlit")
DebugLog ShaderGetBindings(s)    ; "albedoMap:sampler2D@0,properties:block@0,..."
```

### ShaderGetResourceInfo

```
info$ = ShaderGetResourceInfo(shader)
```

**Returns** the shader's resource use: entities using it, materials,
passes, parameters, keywords, number of compiles, the last compile's
time, compiled shaders in the cache and cache hits.

```basic
s = LoadShader("builtin:toon")
DebugLog ShaderGetResourceInfo(s)
```

### ShaderGetCompilationStatus

```
status = ShaderGetCompilationStatus(shader)
```

**Returns** `SHADER_NOT_COMPILED` (0), `SHADER_COMPILED` (1),
`SHADER_FAILED` (2), or `SHADER_VALIDATED` (3: checked, waiting for the
window to open for the GPU compile).

```basic
s = LoadShader("shaders/glow.glsl")
If ShaderGetCompilationStatus(s) = SHADER_FAILED Then DebugLog ShaderError(s)
```

### ShaderGetBackend

```
text$ = ShaderGetBackend([shader = 0])
```

**Returns** the rendering backend and shader languages; given a shader,
also its source language.

```basic
DebugLog ShaderGetBackend()     ; "OpenGL 3.3 core / GLES 3.0 (Fyrox); GLSL 330, WGSL via naga"
```

## PBR materials

Explained in [chapter 3](03-materials.md#pbr-materials). These commands
work on materials from `CreatePBRMaterial`; most also work on any
material or shader that has the matching parameter (`albedo`,
`roughness`, ...).

### CreatePBRMaterial

```
material = CreatePBRMaterial()
```

Makes a physically based material: colour, metallic and roughness,
texture maps, normal maps, parallax, emission, see-through modes and
reflections. It is lit by every light and casts and receives shadows.
Draw entities with it using `EntityMaterial`.

**Returns** the material. It starts white, not metallic, roughness 0.5.

```basic
steel = CreatePBRMaterial()
SetAlbedo steel, 160, 160, 165
SetMetallic steel, 1
SetRoughness steel, 0.45
pipe = CreateCylinder(24)
EntityMaterial pipe, steel
```

### SetAlbedo

```
SetAlbedo material, red, green, blue [, alpha = 255]
```

Sets the base colour, 0-255. With an albedo map, the map is multiplied
by this colour. Works on PBR, toon, foliage and most built-in materials.

```basic
m = CreatePBRMaterial()
SetAlbedo m, 200, 30, 30
```

### SetAlbedoMap

```
SetAlbedoMap material, texture
```

Sets the base colour texture. 0 removes it.

```basic
m = CreatePBRMaterial()
SetAlbedoMap m, LoadTexture("examples/media/checker.png")
```

### SetMetallic

```
SetMetallic material, metallic
```

How metallic the surface is: 0 (plastic, stone, wood) to 1 (metal).
Values in between are for dirty or painted metal.

```basic
m = CreatePBRMaterial()
SetMetallic m, 1
```

### SetMetallicMap

```
SetMetallicMap material, texture
```

A texture whose red channel scales the metallic value across the
surface: white where it is metal. Set `SetMetallic m, 1` so the map is
used as it is.

```basic
m = CreatePBRMaterial()
SetMetallic m, 1
SetMetallicMap m, LoadTexture("armour_metal.png")
```

### SetRoughness

```
SetRoughness material, roughness
```

How rough the surface is: 0 (a mirror, sharp highlights) to 1 (matte,
no visible highlight).

```basic
m = CreatePBRMaterial()
SetRoughness m, 0.2
```

### SetRoughnessMap

```
SetRoughnessMap material, texture
```

A texture whose red channel scales the roughness across the surface:
white where it is rough. Set `SetRoughness m, 1` so the map is used as
it is.

```basic
m = CreatePBRMaterial()
SetRoughness m, 1
SetRoughnessMap m, LoadTexture("floor_rough.png")
```

### SetNormalMap

```
SetNormalMap material, texture [, strength = 1]
```

Sets a normal map, which fakes small bumps and dents. `strength` 0 is
flat, 1 as painted, above 1 deeper. Texture 0 removes it.

```basic
m = CreatePBRMaterial()
SetNormalMap m, LoadTexture("brick_normal.png"), 1.5
```

### SetAOMap

```
SetAOMap material, texture
```

Sets the ambient occlusion texture: dark in creases and corners where
ambient light cannot reach.

```basic
m = CreatePBRMaterial()
SetAOMap m, LoadTexture("brick_ao.png")
```

### SetEmissive

```
SetEmissive material, red, green, blue [, strength = 1]
```

Makes the material glow with a colour (0-255) times `strength`. Glow is
not lit or shadowed; above about strength 1 it makes bloom. 0, 0, 0
turns it off.

```basic
screen = CreatePBRMaterial()
SetAlbedo screen, 10, 10, 10
SetEmissive screen, 80, 200, 255, 3
```

### SetEmissiveMap

```
SetEmissiveMap material, texture
```

A texture that colours the glow: black parts do not glow. Use it with
`SetEmissive` (the map is multiplied by its colour and strength).

```basic
m = CreatePBRMaterial()
SetEmissive m, 255, 255, 255, 2
SetEmissiveMap m, LoadTexture("windows_lit.png")
```

### SetOpacity

```
SetOpacity material, opacity [, mode = OPACITY_BLEND]
```

How opaque the material is, 0 (invisible) to 1 (solid). `mode` is
`OPACITY_BLEND` (smooth see-through, the default), `OPACITY_CUTOUT`
(drawn only where opacity is 0.5 or more) or `OPACITY_OPAQUE`.

```basic
glassy = CreatePBRMaterial()
SetAlbedo glassy, 180, 220, 255
SetOpacity glassy, 0.3
```

### SetOpacityMap

```
SetOpacityMap material, texture [, mode = OPACITY_CUTOUT]
```

A texture whose red channel is the opacity: black is see-through.
`OPACITY_CUTOUT` (the default) cuts holes, keeps the surface solid and
keeps shadows; `OPACITY_BLEND` blends; `OPACITY_OPAQUE` ignores it.

```basic
leaves = CreatePBRMaterial()
SetAlbedoMap leaves, LoadTexture("leaf.png")
SetOpacityMap leaves, LoadTexture("leaf_mask.png")
```

### SetHeightMap

```
SetHeightMap material, texture [, scale = 0.05]
```

Sets a height map (white is high) for parallax mapping, so flat surfaces
look deep as the camera moves. `scale` is the depth; keep it small.
Texture 0 turns parallax off.

```basic
cobbles = CreatePBRMaterial()
SetAlbedoMap cobbles, LoadTexture("cobbles.png")
SetHeightMap cobbles, LoadTexture("cobbles_height.png"), 0.04
```

### SetParallaxScale

```
SetParallaxScale material, scale
```

Changes the depth of parallax mapping; 0 turns it off.

```basic
m = CreatePBRMaterial()
SetHeightMap m, LoadTexture("cobbles_height.png")
SetParallaxScale m, 0.08
```

### SetEnvironmentMap

```
SetEnvironmentMap material, cubemap [, intensity = 1]
```

Makes the material reflect a cube map (loaded with flag 128, or
`ProbeTexture(probe)`), with `intensity`.

```basic
chrome = CreatePBRMaterial()
SetMetallic chrome, 1
SetRoughness chrome, 0.05
SetEnvironmentMap chrome, LoadTexture("sky.png", 128)
```

### SetReflectionIntensity

```
SetReflectionIntensity material, intensity
```

How strongly the material reflects its environment map.

```basic
m = CreatePBRMaterial()
SetEnvironmentMap m, LoadTexture("sky.png", 128)
SetReflectionIntensity m, 0.4
```

### SetFresnel

```
SetFresnel material, amount
```

How much stronger reflections get at glancing angles: 0 not at all, 1
fully (as real surfaces do).

```basic
m = CreatePBRMaterial()
SetFresnel m, 0.5
```

### SetMaterialWorkflow

```
SetMaterialWorkflow material, workflow [, red = 10, green = 10, blue = 10]
```

`WORKFLOW_METALLIC` (the default) or `WORKFLOW_SPECULAR` with a specular
colour (0-255), for texture sets made with specular maps.

```basic
m = CreatePBRMaterial()
SetMaterialWorkflow m, WORKFLOW_SPECULAR, 40, 40, 40
```

## Toon and stylized materials

Explained in [chapter 3](03-materials.md#toon-and-stylized-materials).

### CreateToonMaterial

```
material = CreateToonMaterial([mode = SHADING_TOON])
```

Makes a stylized material. `mode` is a shading mode: `SHADING_TOON`,
`SHADING_CEL`, `SHADING_FLAT`, `SHADING_UNLIT`, `SHADING_VERTEXLIT`,
`SHADING_PIXEL`, `SHADING_PS1`, `SHADING_MATCAP`, `SHADING_POSTERIZE`,
`SHADING_HANDPAINTED`, `SHADING_LAMBERT`, `SHADING_PHONG`, `SHADING_PBR` or
`SHADING_RIM` (see the [table](03-materials.md#shading-modes)).

**Returns** the material.

```basic
hero = CreateToonMaterial(SHADING_CEL)
SetAlbedo hero, 90, 200, 255
```

### SetShadingMode

```
SetShadingMode material, mode
```

Switches the material to another shading mode, keeping the settings both
modes have (colour, texture, ...).

```basic
m = CreateToonMaterial()
SetShadingMode m, SHADING_PS1
```

### SetToonSteps

```
SetToonSteps material, steps
```

How many flat bands of light the material has; 1 gives two tones.

```basic
m = CreateToonMaterial()
SetToonSteps m, 4
```

### SetToonRamp

```
SetToonRamp material, texture
```

Lights the material through a ramp picture instead of steps: the light
level picks a colour from left (dark) to right (lit). Texture 0 goes back
to steps.

```basic
m = CreateToonMaterial()
SetToonRamp m, LoadTexture("ramp_warm.png")
```

### SetLightingRamp

```
SetLightingRamp material, texture
```

The same as `SetToonRamp`, for any ramp-lit material.

```basic
m = CreateToonMaterial(SHADING_CEL)
SetLightingRamp m, LoadTexture("ramp_anime.png")
```

### SetToonOutline

```
SetToonOutline material, enable [, width = 0.03]
```

Gives every entity drawn with the material an outline `width` units wide
(enable 1), or removes it (0).

```basic
m = CreateToonMaterial()
SetToonOutline m, True, 0.05
```

### SetOutlineColor

```
SetOutlineColor material, red, green, blue [, alpha = 255]
```

Sets the outline colour, 0-255.

```basic
m = CreateToonMaterial()
SetToonOutline m, True
SetOutlineColor m, 40, 20, 10
```

### SetOutlineWidth

```
SetOutlineWidth material, width
```

Sets the outline width in world units.

```basic
m = CreateToonMaterial()
SetToonOutline m, True
SetOutlineWidth m, 0.08
```

### SetRimLighting

```
SetRimLighting material, enable [, strength = 0.6]
```

Turns rim lighting on or off: a glow along the edges facing away from
the camera. `strength` is how bright.

```basic
m = CreateToonMaterial()
SetRimLighting m, True, 0.8
```

### SetRimColor

```
SetRimColor material, red, green, blue
```

Sets the rim light colour, 0-255.

```basic
m = CreateToonMaterial()
SetRimLighting m, True
SetRimColor m, 255, 200, 120
```

### SetRimPower

```
SetRimPower material, power
```

How thin the rim is: higher is thinner (3 to start with).

```basic
m = CreateToonMaterial()
SetRimLighting m, True
SetRimPower m, 6
```

### SetVertexLighting

```
SetVertexLighting material, enable
```

1 lights the material per vertex (the look of early 3D hardware); 0 goes
back to toon shading.

```basic
m = CreateToonMaterial()
SetVertexLighting m, True
```

### SetFlatShading

```
SetFlatShading material, enable
```

1 shades the material with one flat colour per triangle (low poly); 0
goes back to toon shading.

```basic
m = CreateToonMaterial()
SetFlatShading m, True
```

### SetPixelShading

```
SetPixelShading material, enable
```

1 draws the material with crisp texture pixels and banded light (pixel
art); 0 goes back to toon shading.

```basic
m = CreateToonMaterial()
SetAlbedoMap m, LoadTexture("examples/media/checker.png")
SetPixelShading m, True
```

## Water, wind and foliage

Explained in [chapter 3](03-materials.md#water).

### CreateWaterMaterial

```
material = CreateWaterMaterial()
```

Makes an animated water material: waves that move the vertices, a colour
that deepens with depth, foam, reflection and refraction. Give the water
mesh enough vertices for the waves (a grid, not a single quad).

**Returns** the material.

```basic
sea = CreateWaterMaterial()
pond = CreateMesh()     ; build a grid here: see chapter 3
EntityMaterial pond, sea
```

### SetWaterWaves

```
SetWaterWaves material, height [, length = 6, dir_x = 1, dir_z = 0.3]
```

Sets the waves: their height, length (units between crests) and the
direction `dir_x`, `dir_z` they travel in.

```basic
sea = CreateWaterMaterial()
SetWaterWaves sea, 0.5, 10, 1, 0.2      ; a big, slow swell
```

### SetWaterSpeed

```
SetWaterSpeed material, speed
```

How fast the waves move (1 to start with).

```basic
sea = CreateWaterMaterial()
SetWaterSpeed sea, 0.5
```

### SetWaterColor

```
SetWaterColor material, shallow_red, shallow_green, shallow_blue, deep_red, deep_green, deep_blue
```

Sets the colour of shallow and of deep water, 0-255.

```basic
swamp = CreateWaterMaterial()
SetWaterColor swamp, 90, 110, 40, 20, 30, 10
```

### SetWaterDepth

```
SetWaterDepth material, depth
```

The depth, in units, at which the water is fully its deep colour.

```basic
sea = CreateWaterMaterial()
SetWaterDepth sea, 10       ; clear water
```

### SetWaterFoam

```
SetWaterFoam material, amount [, red = 255, green = 255, blue = 255]
```

How much foam shows on wave crests and where the water meets things, and
its colour. 0 is none.

```basic
sea = CreateWaterMaterial()
SetWaterFoam sea, 0.8
```

### SetWaterReflection

```
SetWaterReflection material, amount [, cubemap = 0]
```

How much the water reflects, 0-1, optionally from a cube map of the sky.

```basic
sea = CreateWaterMaterial()
SetWaterReflection sea, 0.8, LoadTexture("sky.png", 128)
```

### SetWaterRefraction

```
SetWaterRefraction material, amount
```

How much the water bends the view of what is under it.

```basic
sea = CreateWaterMaterial()
SetWaterRefraction sea, 1
```

### CreateFoliageMaterial

```
material = CreateFoliageMaterial()
```

Makes a material for leaves and grass that sway in the wind. Higher
vertices move more than lower ones, so plants should stand upright with
their base at y = 0. It follows the global wind (`SetWindDirection`,
`SetWindStrength`) unless `SetFoliageWind` gives it its own.

**Returns** the material.

```basic
grass = CreateFoliageMaterial()
SetAlbedo grass, 80, 180, 60
bush = CreateCone(12)
EntityMaterial bush, grass
```

### SetFoliageWind

```
SetFoliageWind material, strength [, speed = 1.5, stiffness = 1]
```

Gives a foliage material its own wind instead of the global one.
Stiffness above 1 bends less, below 1 more.

```basic
reeds = CreateFoliageMaterial()
SetFoliageWind reeds, 0.6, 2.5, 0.5     ; strong, fast, floppy
```

### SetWindDirection

```
SetWindDirection x, z
```

Sets the direction of the global wind, used by foliage and wind
materials.

```basic
SetWindDirection -1, 0.5
```

### SetWindStrength

```
SetWindStrength strength
```

Sets the strength of the global wind (0.2 to start with). Change it over
time for gusts.

```basic
SetWindStrength 0.2 + Sin(MilliSecs() / 20.0) * 0.1
```

## Procedural materials

Explained in [chapter 3](03-materials.md#procedural-materials).

### CreateProceduralShader

```
material = CreateProceduralShader([kind = 0])
```

Makes a procedural material: `kind` 0 starts as a gradient, 1 as noise,
2 as a pattern (checker). All three can be mixed with the commands below.

**Returns** the material.

```basic
sky = CreateProceduralShader(0)
clouds = CreateProceduralShader(1)
tiles = CreateProceduralShader(2)
```

### SetGradient

```
SetGradient material, type
```

The shape of the gradient: `GRADIENT_LINEAR` (across), `GRADIENT_RADIAL`
(from the centre out), `GRADIENT_ANGULAR` (around the centre) or
`GRADIENT_NONE` (only colour 0).

```basic
m = CreateProceduralShader(0)
SetGradient m, GRADIENT_RADIAL
```

### SetGradientColor

```
SetGradientColor material, index, red, green, blue [, alpha = 255]
```

Sets gradient colour `index`: 0 the start, 1 the end. In noise materials,
colour 0 is the colour where the noise is low.

```basic
m = CreateProceduralShader(0)
SetGradientColor m, 0, 255, 120, 40     ; orange
SetGradientColor m, 1, 40, 30, 120      ; to deep blue
```

### SetGradientDirection

```
SetGradientDirection material, angle
```

The angle of a linear gradient in degrees (0 along u, 90 along v), or the
starting angle of an angular one.

```basic
m = CreateProceduralShader(0)
SetGradientDirection m, 45
```

### SetNoiseType

```
SetNoiseType material, type
```

The kind of noise: `NOISE_VALUE`, `NOISE_PERLIN`, `NOISE_SIMPLEX`,
`NOISE_VORONOI` or `NOISE_CELLULAR` (see the
[table](03-materials.md#procedural-materials)).

```basic
m = CreateProceduralShader(1)
SetNoiseType m, NOISE_VORONOI
```

### SetNoiseScale

```
SetNoiseScale material, scale
```

The size of the noise's features: higher gives smaller, more numerous
features (4 to start with).

```basic
m = CreateProceduralShader(1)
SetNoiseScale m, 12
```

### SetNoiseOctaves

```
SetNoiseOctaves material, octaves
```

How many layers of finer noise are added together, 1-12 (4 to start
with). More layers give more natural detail.

```basic
m = CreateProceduralShader(1)
SetNoiseOctaves m, 7
```

### SetNoisePersistence

```
SetNoisePersistence material, persistence
```

How strong each finer layer is compared with the one before (usually
0.5). Higher is rougher.

```basic
m = CreateProceduralShader(1)
SetNoisePersistence m, 0.65
```

### SetNoiseLacunarity

```
SetNoiseLacunarity material, lacunarity
```

How much finer each layer is than the one before (usually 2).

```basic
m = CreateProceduralShader(1)
SetNoiseLacunarity m, 2.5
```

### SetNoiseSpeed

```
SetNoiseSpeed material, speed
```

How fast the noise moves; 0 is still.

```basic
m = CreateProceduralShader(1)
SetNoiseSpeed m, 0.3
```

### SetNoiseStrength

```
SetNoiseStrength material, strength
```

How much the noise disturbs the gradient, in gradient and pattern
materials (0 to start with).

```basic
m = CreateProceduralShader(0)
SetNoiseStrength m, 0.3
```

### SetDistortion

```
SetDistortion material, amount
```

Warps the gradient and pattern with the noise: marble veins, wood grain,
wobbly tiles.

```basic
m = CreateProceduralShader(2)
SetDistortion m, 0.2
```

### SetProceduralSeed

```
SetProceduralSeed material, seed
```

Gives a different variation of the same noise and cells, so two rocks
made from one recipe don't look identical.

```basic
rockA = CreateProceduralShader(1)
rockB = ShaderClone(rockA)
SetProceduralSeed rockB, 17
```

### SetPattern

```
SetPattern material, type
```

The pattern drawn over the gradient: `PATTERN_NONE`, `PATTERN_CHECKER`,
`PATTERN_GRID`, `PATTERN_STRIPES`, `PATTERN_DOTS`, `PATTERN_CELLS` or
`PATTERN_NOISE` (the noise itself).

```basic
m = CreateProceduralShader(2)
SetPattern m, PATTERN_STRIPES
```

### SetPatternScale

```
SetPatternScale material, scale
```

How many times the pattern repeats across the surface (8 to start with).

```basic
m = CreateProceduralShader(2)
SetPatternScale m, 16
```

### SetPatternColor

```
SetPatternColor material, red, green, blue [, alpha = 255]
```

The pattern's colour; `alpha` sets how strongly it covers the gradient.
In noise materials, the colour where the noise is high.

```basic
m = CreateProceduralShader(2)
SetPatternColor m, 20, 20, 20, 180
```

### SetPatternRotation

```
SetPatternRotation material, angle
```

Turns the pattern by `angle` degrees.

```basic
m = CreateProceduralShader(2)
SetPattern m, PATTERN_STRIPES
SetPatternRotation m, 45
```

### BakeProceduralTexture

```
texture = BakeProceduralTexture(material [, width = 256, height = 256, normal_map = 0])
```

Draws a procedural material into a new texture, on the CPU, so it can be
used like any texture. With `normal_map` 1 it makes a normal map from the
material's noise instead. Bake while loading, not every frame; a baked
texture does not move with `SetNoiseSpeed`.

**Returns** the texture.

```basic
bumps = CreateProceduralShader(1)
SetNoiseScale bumps, 8
rock = CreatePBRMaterial()
SetNormalMap rock, BakeProceduralTexture(bumps, 512, 512, True)
```

## Post-processing

Explained in [chapter 6](06-post-processing.md).

### LoadPostProcess

```
effect = LoadPostProcess(file$)
```

Loads an effect from a file (`.glsl` with a `nb_post` function, or
`.wgsl`), or a built-in one written `"builtin:name"`: `bloom`,
`exposure`, `tonemap`, `vignette`, `chromatic`, `grain`, `colorgrade`,
`gamma`, `contrast`, `saturation`, `brightness`, `fxaa`, `outline`,
`pixelate`, `crt`, `distortion`, `heathaze`, `blur`, `gaussian`,
`radialblur`, `sharpen`, `dof`, `motionblur`, `ssr`, `ssao`, `fog`,
`underwater`, `rain`, `snow`, `invert` (parameters in the
[table](06-post-processing.md#built-in-effects)).

**Returns** the effect, or 0 on failure. It does nothing until added to a
camera with `AddPostProcess`.

```basic
bloom = LoadPostProcess("builtin:bloom")
mine = LoadPostProcess("shaders/scanlines.glsl")
```

### CreatePostProcess

```
effect = CreatePostProcess(source$ [, language$ = "glsl"])
```

Makes an effect from source text: GLSL with `vec4 nb_post(vec2 uv)`, or
WGSL.

**Returns** the effect.

```basic
src$ = "//! fragment" + Chr(10)
src$ = src$ + "vec4 nb_post(vec2 uv) { return vec4(1.0) - texture(nb_screenTexture, uv); }"
negative = CreatePostProcess(src$)
```

### AddPostProcess

```
AddPostProcess effect [, camera = 0]
```

Adds the effect to the camera's chain. Camera 0 adds it to every camera.

```basic
camera = CreateCamera()
AddPostProcess LoadPostProcess("builtin:vignette"), camera
```

### RemovePostProcess

```
RemovePostProcess effect [, camera = 0]
```

Takes the effect out of the camera's chain (0: out of every chain). The
effect still exists and can be added again.

```basic
camera = CreateCamera()
crt = LoadPostProcess("builtin:crt")
AddPostProcess crt, camera
RemovePostProcess crt, camera
```

### SetPostProcess

```
SetPostProcess camera, effect
```

Makes the effect the only effect of the camera. Effect 0 clears the
camera's chain.

```basic
camera = CreateCamera()
SetPostProcess camera, LoadPostProcess("builtin:pixelate")
SetPostProcess camera, 0
```

### FreePostProcess

```
FreePostProcess effect
```

Removes the effect from every chain and frees it.

```basic
fx = LoadPostProcess("builtin:grain")
FreePostProcess fx
```

### SetPostProcessParameter

```
SetPostProcessParameter effect, name$, value$
```

Sets an effect's parameter from text: numbers separated by commas (one
for a number, three for a colour 0-1), or a texture handle.

```basic
fog = LoadPostProcess("builtin:fog")
SetPostProcessParameter fog, "density", "0.05"
SetPostProcessParameter fog, "color", "0.8, 0.7, 0.6"
```

### SetPostProcessEnabled

```
SetPostProcessEnabled effect, enable
```

Turns the effect on (1) or off (0) without removing it from its chains.

```basic
bloom = LoadPostProcess("builtin:bloom")
SetPostProcessEnabled bloom, False
```

### SetPostProcessOrder

```
SetPostProcessOrder effect, order
```

Where the effect runs in its chain: lower orders run first. Effects start
in the order they were made.

```basic
grain = LoadPostProcess("builtin:grain")
SetPostProcessOrder grain, 100      ; always last
```

### PostProcessCamera

```
PostProcessCamera effect, camera
```

Makes the effect run only for this camera.

```basic
player1 = CreateCamera()
hurt = LoadPostProcess("builtin:vignette")
PostProcessCamera hurt, player1
```

### PostProcessPriority

```
PostProcessPriority effect, priority
```

A negative priority runs the effect on the HDR image before tone mapping
(linear light, values above 1); 0 or more runs it on the final image.

```basic
fx = LoadPostProcess("builtin:exposure")
PostProcessPriority fx, -1
```

### PostProcessInput

```
PostProcessInput effect, source
```

Chooses what the effect reads: `POST_PREVIOUS` (0, the picture so far),
`POST_ORIGINAL` (-1, the scene before any effect), or another effect's
result (when that effect draws at a reduced resolution).

```basic
fx = LoadPostProcess("builtin:blur")
PostProcessInput fx, POST_ORIGINAL
```

### PostProcessOutput

```
PostProcessOutput effect, texture
```

Makes the effect draw into a texture instead of the screen chain (render
to texture). 0 draws into the chain again.

```basic
monitor = CreateTexture(512, 512)
fx = LoadPostProcess("builtin:crt")
PostProcessOutput fx, monitor
```

### PostProcessResolution

```
PostProcessResolution effect, scale
```

Draws the effect at `scale` (0.05 to 1) of the screen size, for cheaper
blurs and glows.

```basic
fx = LoadPostProcess("builtin:gaussian")
PostProcessResolution fx, 0.5
```

### PostProcessShader

```
shader = PostProcessShader(effect)
```

**Returns** the effect's shader, to use with the `ShaderSet...` and
debugging commands.

```basic
vig = LoadPostProcess("builtin:vignette")
ShaderSetColor PostProcessShader(vig), "color", 60, 0, 0
```

### SetRainEffect

```
SetRainEffect camera, amount [, speed = 1]
```

Draws rain streaks over the camera's view, `amount` 0-1 (0 turns it
off).

```basic
camera = CreateCamera()
SetRainEffect camera, 0.6
```

### SetSnowEffect

```
SetSnowEffect camera, amount [, speed = 0.3]
```

Draws falling snow over the camera's view, `amount` 0-1 (0 turns it
off).

```basic
camera = CreateCamera()
SetSnowEffect camera, 0.5, 0.2
```

### SetAtmosphericFog

```
SetAtmosphericFog camera, density [, red = 166, green = 184, blue = 204, height_falloff = 0.08]
```

Adds height fog to the camera, worked out from the scene's depth:
`density` (0 turns it off), its colour (0-255), and how fast it thins
with height.

```basic
camera = CreateCamera()
SetAtmosphericFog camera, 0.03, 200, 190, 170
```

### SetUnderwaterEffect

```
SetUnderwaterEffect camera, enable [, water_level = 0, red = 26, green = 89, blue = 115]
```

Turns the underwater effect on or off: a tinted, wavy, foggy view
whenever the camera is below `water_level`, in the water colour (0-255).

```basic
camera = CreateCamera()
SetUnderwaterEffect camera, True, 2.5
```

## Shadows

Explained in [chapter 7](07-shadows.md).

### SetShadowMode

```
SetShadowMode mode
```

`SHADOW_OFF`, `SHADOW_HARD` (sharp edges) or `SHADOW_SOFT` (filtered
edges).

```basic
SetShadowMode SHADOW_SOFT
```

### EnableShadows

```
EnableShadows
```

Turns real-time shadows on.

```basic
EnableShadows
```

### DisableShadows

```
DisableShadows
```

Turns real-time shadows off.

```basic
DisableShadows
```

### SetShadowQuality

```
SetShadowQuality quality
```

Shadow map size and precision: 0 (512, half precision), 1 (1024, half),
2 (2048, full), 3 (4096, full).

```basic
SetShadowQuality 3
```

### SetShadowResolution

```
SetShadowResolution size
```

Sets the shadow map size of directional and spot lights, 64-8192. Point
lights use half.

```basic
SetShadowResolution 3072
```

### SetShadowDistance

```
SetShadowDistance distance
```

How far from the camera shadows are drawn, in world units (64 to start
with). Shorter is sharper.

```basic
SetShadowDistance 40
```

### SetShadowCascadeCount

```
SetShadowCascadeCount count
```

How many cascades (1 to 3) directional light shadows are split into.
More keep near shadows sharp.

```basic
SetShadowCascadeCount 2
```

### SetShadowBias

```
SetShadowBias bias
```

Moves shadows slightly away from surfaces, to stop speckled "shadow
acne". Applies to every light, including ones made later. Too much makes
shadows start away from objects.

```basic
SetShadowBias 0.0005
```

### SetShadowSoftness

```
SetShadowSoftness softness
```

Above 0 turns on filtered soft shadow edges; 0 turns them off. Fyrox's
soft shadows have a fixed filter size.

```basic
SetShadowSoftness 1
```

### LightShadows

```
LightShadows light, enable
```

Turns shadows on (1) or off (0) for one light.

```basic
fill = CreateLight(LIGHT_POINT)
LightShadows fill, False
```

## Constants

| Constant | Value | Used by |
|---|---|---|
| `SHADING_TOON`, `SHADING_CEL`, `SHADING_FLAT`, `SHADING_UNLIT`, `SHADING_VERTEXLIT`, `SHADING_PIXEL`, `SHADING_LAMBERT`, `SHADING_PHONG`, `SHADING_PBR`, `SHADING_MATCAP`, `SHADING_PS1`, `SHADING_POSTERIZE`, `SHADING_HANDPAINTED`, `SHADING_RIM` | 0-13, in that order | `CreateToonMaterial`, `SetShadingMode` |
| `GRADIENT_LINEAR`, `GRADIENT_RADIAL`, `GRADIENT_ANGULAR`, `GRADIENT_NONE` | 0-3 | `SetGradient` |
| `NOISE_VALUE`, `NOISE_PERLIN`, `NOISE_SIMPLEX`, `NOISE_VORONOI`, `NOISE_CELLULAR` | 0-4 | `SetNoiseType` |
| `PATTERN_NONE`, `PATTERN_CHECKER`, `PATTERN_GRID`, `PATTERN_STRIPES`, `PATTERN_DOTS`, `PATTERN_CELLS`, `PATTERN_NOISE` | 0-6 | `SetPattern` |
| `WORKFLOW_METALLIC`, `WORKFLOW_SPECULAR` | 0, 1 | `SetMaterialWorkflow` |
| `OPACITY_OPAQUE`, `OPACITY_CUTOUT`, `OPACITY_BLEND` | 0-2 | `SetOpacity`, `SetOpacityMap` |
| `POST_PREVIOUS`, `POST_ORIGINAL` | 0, -1 | `PostProcessInput` |
| `SHADER_NOT_COMPILED`, `SHADER_COMPILED`, `SHADER_FAILED`, `SHADER_VALIDATED` | 0-3 | `ShaderGetCompilationStatus` |
| `SHADOW_OFF`, `SHADOW_HARD`, `SHADOW_SOFT` | 0-2 | `SetShadowMode` |

[Contents](../SHADERS.md) · Previous: [9. Debugging and performance](09-debugging.md)
