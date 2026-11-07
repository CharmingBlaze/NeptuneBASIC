# 9. Debugging and performance

[Contents](../SHADERS.md) · Previous: [8. WGSL and Fyrox shader files](08-wgsl-and-fyrox.md) · Next: [10. Command reference](10-reference.md)

- [When a shader does not compile](#when-a-shader-does-not-compile)
- [Reading an error](#reading-an-error)
- [Checking a shader from BASIC](#checking-a-shader-from-basic)
- [Looking inside a shader](#looking-inside-a-shader)
- [Hot reloading](#hot-reloading)
- [When a shader compiles but looks wrong](#when-a-shader-compiles-but-looks-wrong)
- [Performance](#performance)
- [Limits](#limits)
- [How it works](#how-it-works)

## When a shader does not compile

A shader with a mistake does not stop your program. Instead:

- `LoadShader` and `CompileShader` print the error to the console;
- `CompileShader` returns 0, and `ShaderCompiled` returns 0;
- `ShaderError` returns the error text;
- the entities using the shader keep drawing with the last version that
  compiled, or with their normal materials if none did.

So the usual pattern is:

```basic
glow = LoadShader("glow.glsl")
If ShaderCompiled(glow) = 0 Then RuntimeError ShaderError(glow)
```

During development you may prefer to keep running and just print it:

```basic
glow = LoadShader("glow.glsl")
If ShaderCompiled(glow) = 0 Then DebugLog ShaderError(glow)
```

## Reading an error

Take this shader, which uses a name it never declared:

```
// broken.glsl
uniform vec4 tint = vec4(1.0);
uniform float speed = 2.0;

//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = tint.rgb * glow;
}
```

`ShaderError` gives:

```
broken.glsl:6 (Forward fragment): 'glow' : undeclared identifier
broken.glsl:6 (Forward fragment): '' : compilation terminated
(Forward fragment) ERROR: 2 compilation errors.  No code generated.
```

- `broken.glsl:6` is the file and line of **your** source. NeptuneBASIC
  adds a lot of code around your shader before the GPU sees it, and maps
  the GPU's line numbers back to yours, through `#include`s too.
- `(Forward fragment)` is the pass and stage the GPU was compiling.
- After that comes the GPU driver's message. Drivers word them
  differently; the first line is the one that matters. Later lines are
  often knock-on effects.

Errors come from three places, all reported this way:

| Checked by | Finds | Example |
|---|---|---|
| NeptuneBASIC | bad directives, uniform declarations it cannot read, missing files and stages | `glow.glsl:3: blend must be none, alpha, add, multiply or premultiplied, not 'glowy'` |
| naga (WGSL only) | any mistake in WGSL | `ripple.wgsl:12: no definition in scope for identifier: 'x'` |
| the GPU driver | mistakes in the GLSL itself | `glow.glsl:6 (Forward fragment): 'glow' : undeclared identifier` |

Shaders are compiled on the GPU straight away, so errors appear when you
load the shader, not later when it is first drawn. Shaders compiled
before `Graphics3D` opens the window are checked as far as possible and
compiled on the GPU when the window opens.

Common GPU messages and what they usually mean:

| Message contains | Usually |
|---|---|
| `undeclared identifier` | a typo in a name, or a parameter that is not declared |
| `syntax error` | a missing `;`, `)` or `}` on that line or the one before |
| `cannot convert`, `wrong operand types` | mixing types: `vec3 c = 1.0;` instead of `vec3(1.0)`, or a `vec3` where a `vec4` is needed |
| `no matching overloaded function` | wrong arguments to a function, often an `int` where a `float` is needed (`pow(x, 2)` → `pow(x, 2.0)`) |
| `redefinition` | the same name declared twice, often a parameter and a local variable |

## Checking a shader from BASIC

| Command | Gives |
|---|---|
| `CompileShader(s)` | compiles; 1 on success, 0 on failure |
| `ValidateShader(s)` | checks the current source **without** replacing the version in use: try an edit before you commit to it |
| `ShaderCompiled(s)` | 1 if the last compile worked |
| `ShaderGetCompilationStatus(s)` | `SHADER_NOT_COMPILED`, `SHADER_COMPILED`, `SHADER_FAILED`, or `SHADER_VALIDATED` (checked, waiting for the window to compile on the GPU) |
| `ShaderError$(s)` | the last error, or "" |
| `ShaderLog$(s)` | every compile, warning, error and hot reload, one per line |

```basic
; try a new version and keep the old one if it is broken
Graphics3D 640, 480
s = CreateShader("//! fragment" + Chr(10) + "void main() { FragColor = vec4(1.0); }")
ShaderSource s, "//! fragment" + Chr(10) + "void main() { FragColor = vec4(oops); }"
If ValidateShader(s) Then
    CompileShader s
Else
    DebugLog "not using the new version: " + ShaderError(s)
EndIf
```

## Looking inside a shader

`ShaderDebug$(s)` returns a full report. For a small shader with two
parameters and a texture, it looks like this:

```
Shader 2 'good.glsl'
language: GLSL
backend: OpenGL 3.3 core / GLES 3.0 (Fyrox); GLSL 330, WGSL via naga
status: compiled
pass: Forward, lighting: Pbr, blend: None, cull: Back, shadows: false
passes: Forward
uniforms: tint:vec4,speed:float,nb_resolution:vec2,nb_ambient:vec3,nb_fogColor:vec3,nb_fogRange:vec2
bindings: picture:sampler2D@0,properties:block@0,fyrox_instanceData:block@1,...
attributes: vertexPosition@0,vertexTexCoord@1,vertexNormal@2,...
files: good.glsl
keywords:
enabled: true
hot reload: false
entities=1,materials=1,passes=1,parameters=7,keywords=,compiles=1,last_compile_ms=71.1,cache_entries=1,cache_hits=0,source_bytes=192
```

The parts are also available one at a time:

| Command | Returns |
|---|---|
| `ShaderGetUniforms$(s)` | the parameters, `name:type` separated by commas |
| `ShaderGetAttributes$(s)` | the vertex attributes, `name@location` |
| `ShaderGetBindings$(s)` | textures and uniform blocks, `name:kind@binding` |
| `ShaderGetResourceInfo$(s)` | entities using it, materials, passes, parameters, keywords, compiles, last compile time, cache size and hits |
| `ShaderGetBackend$([s])` | the renderer and languages, and for a shader its source language |
| `ShaderParameterCount(s)`, `ShaderParameterName$(s, i)`, `ShaderParameterType$(s, name$)`, `ShaderParameterExists(s, name$)` | the parameters one by one |
| `ShaderGetParameter$(s, name$)` | a parameter's value as text |
| `ShaderGetSource$(s, stage)` | 0: your source. 1: the vertex program and 2: the fragment program the GPU compiles. 3: the Fyrox shader |

The parameter list includes the automatic values the shader uses
(`nb_resolution`, `nb_ambient`, ...).

`ShaderGetSource(s, 2)` is the most useful when an error makes no sense:
it shows exactly what the GPU compiled, with everything NeptuneBASIC
added. Write it to a file to read it:

```basic
s = LoadShader("glow.glsl")
f = WriteFile("glow_fragment.txt")
WriteLine f, ShaderGetSource(s, 2)
CloseFile f
```

A program that lists every parameter of a shader:

```basic
s = LoadShader("builtin:water")
For i = 0 To ShaderParameterCount(s) - 1
    n$ = ShaderParameterName(s, i)
    DebugLog n + " (" + ShaderParameterType(s, n) + ") = " + ShaderGetParameter(s, n)
Next
```

## Hot reloading

```basic
s = LoadShader("shaders/water.glsl")
ShaderHotReload s                  ; ShaderHotReload s, False turns it off
```

With hot reloading on, the runtime checks four times a second whether the
shader's file, or any file it includes, has changed. When one has, the
shader is compiled again and every entity using it updates at once,
keeping the parameter values you set.

If the new version has an error, the old one stays on screen, and the
error is printed once (not every frame) and kept in `ShaderError` and
`ShaderLog`. Fix the file and save again.

`ReloadShader(s)` does the same once, when you choose (from a key press,
say). Both need a shader loaded from a file.

## When a shader compiles but looks wrong

| You see | Check |
|---|---|
| the object is black | a lighting model with no lights near it; or `s.albedo` set from a texture that is not set (unset textures read white, so look at the maths); or a normal pointing the wrong way |
| the object is white | an unset texture (`ShaderGetParameter` shows 0); a parameter name mistyped in BASIC (that is an error, so check `ShaderError`) |
| colours too bright or washed out | an sRGB colour used as linear: in `FragColor` and `s.emission`, convert with `nb_linear` |
| colours too dark | a linear colour used as sRGB, or a colour from BASIC passed through `nb_linear` twice |
| a see-through object hides things behind it | add `//! blend alpha`; and draw order: see-through objects should not overlap much |
| the inside of an object shows through | `//! cull none` draws both sides: use it only for thin surfaces |
| moving vertices tear the mesh apart | vertices at the same position with different normals (the corners of a cube) move apart: displace along a smooth direction, or use a mesh with smooth normals |
| no waves on water | the mesh has too few vertices: use a grid |
| an effect moves the wrong way in the world | world-space X is mirrored inside shaders: negate X of directions passed from BASIC |
| nothing changes when a parameter is set | the entity has its own value (`ShaderFloat`), which wins over the shader's; or another entity's material |
| no shadows | see [chapter 7](07-shadows.md#fixing-common-problems) |

A quick way to see a value: put it in the colour.
`FragColor = vec4(vec3(value), 1.0);` or `s.albedo = vec3(value);` shows it
as grey levels; `s.albedo = nb_normal * 0.5 + 0.5;` shows the normals as
colours.

## Performance

The shader system is built to cost no more than the engine's own
materials. To keep it that way:

- **Compiled once.** Compiled shaders are kept in a cache, keyed by their
  final text. Every object with the same source and keywords shares one
  GPU program. Loading the same file twice, or making a thousand PBR
  materials, compiles once.
- **Share materials.** Entities that use a shader without per-entity
  values share one material, which Fyrox draws in batches. Setting a
  parameter on the shader is one change, however many entities use it.
  Per-entity values (`ShaderFloat entity, ...`) give that entity a
  material of its own: fine for a few, not for thousands.
- **No per-frame rebuilds.** Setting a parameter changes the material in
  place. Automatic values (`nb_time`, ...) are written once per frame per
  material, and only for shaders that use them.
- **Keywords are cached.** Each combination of keywords is compiled the
  first time it is used and kept.
- **Pixels cost more than vertices.** A fragment shader runs for every
  pixel an object covers, many times more often than its vertex shader.
  Keep heavy work (many noise octaves, loops) for small objects, or move
  it into a texture with `BakeProceduralTexture`.
- **Effects run on every pixel of the screen.** Each one costs about the
  same however busy the scene is. Use `PostProcessResolution e, 0.5` for
  blurs and glows; disable effects you are not using.
- **Shadows** cost a drawing of the scene per shadow map: limit
  shadow-casting lights, and use `SetShadowQuality` for slow machines.

To measure: `ShaderGetResourceInfo(s)` shows compile time, materials and
cache hits; `TrisRendered()` the triangles drawn; and `neptune bench`
compares whole scenes. `examples/shadertests.nb` checks that 200 cubes
with one shader material cost no more than 200 with the standard material.

## Limits

| | |
|---|---|
| stages | vertex and fragment only: no geometry, tessellation or compute (see [chapter 1](01-introduction.md#what-the-engine-can-do) for alternatives) |
| lights in forward shaders | the 16 most important visible lights |
| shadows in forward shaders | not received; use `//! pass gbuffer` |
| soft shadows | on or off, with a fixed filter size; at most 3 cascades; no contact shadows |
| reflections | from cube maps (`SetEnvironmentMap`), the sky box and reflection probes (`EnvironmentLighting 1`), and the `ssr` screen effect |
| post-processing effects | at most 16 textures and 64 parameters each |
| array textures | stored as 3D textures (`ShaderSetTextureArray`, `nb_textureArray`) |
| WGSL | one vertex and one fragment entry point; no array textures or storage buffers |

## How it works

For those curious, or working on NeptuneBASIC itself.

| Piece | File |
|---|---|
| source processing: includes, directives, stages, uniforms, generated passes, error line maps | `crates/neptune-runtime/src/shadersrc.rs` |
| the GLSL library, standard vertex program, built-in shaders | `crates/neptune-runtime/src/shaderlib.rs` |
| WGSL through naga | `crates/neptune-runtime/src/wgsl.rs` |
| shader objects, compile cache, GPU compile, parameters, materials, hot reload, reflection | `crates/neptune-runtime/src/shader.rs` |
| ready-made materials, CPU procedural baking | `crates/neptune-runtime/src/matcmds.rs` |
| post-processing chains and the Fyrox render pass | `crates/neptune-runtime/src/postfx.rs` |
| shadow settings | `crates/neptune-runtime/src/quality.rs` |

**Compiling a shader.** The source is preprocessed: includes are
expanded, keyword `#define`s added, directives read, and `uniform` lines
turned into parameters. Each pass is assembled from the shader's code and
the library (shadow passes reuse the vertex code), parameters are
qualified with their uniform block, and the result is written as a Fyrox
shader. Fyrox parses it, and every pass's GPU program is built straight
away; driver messages are mapped back through per-line source maps. The
compiled shader goes into the cache, keyed by its text. Materials are
Fyrox materials made from it and shared between entities.

**WGSL** is parsed and validated by naga, which writes GLSL; NeptuneBASIC
then renames naga's uniform blocks to parameter names and treats the
result like a GLSL shader.

**Post-processing** is a Fyrox scene render pass. Each frame the
program's chains are turned into a list of passes; the render pass copies
the frame, runs each effect's program on a full-screen quad (switching
between two reused targets, or into an effect's own target or texture),
and writes the result back: in the HDR stage for negative priorities, and
the final stage otherwise.

[Contents](../SHADERS.md) · Previous: [8. WGSL and Fyrox shader files](08-wgsl-and-fyrox.md) · Next: [10. Command reference](10-reference.md)
