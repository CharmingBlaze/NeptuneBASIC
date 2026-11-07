# 8. WGSL and Fyrox shader files

[Contents](../SHADERS.md) · Previous: [7. Shadows](07-shadows.md) · Next: [9. Debugging and performance](09-debugging.md)

Besides NeptuneBASIC's GLSL, shaders can be written in two other formats.
You only need this chapter if you already have shaders in one of them, or
prefer WGSL.

| Format | File | Good for |
|---|---|---|
| NeptuneBASIC GLSL | `.glsl` | almost everything: it writes the passes, lighting and shadows for you |
| WGSL | `.wgsl` | shaders shared with WebGPU programs, or if you prefer WGSL's syntax |
| Fyrox shader | `.shader` | shaders from Fyrox projects, or full control over every pass |

`LoadShader` picks the format from the file's extension. `CreateShader`
takes the language as its second argument: `CreateShader(text$, "wgsl")`
or `CreateShader(text$, "fyrox")`.

## WGSL

WGSL is the shading language of WebGPU. NeptuneBASIC reads WGSL with
**naga** (the shader translator used by the wgpu project), which checks it
and translates it to the GLSL that Fyrox runs.

```wgsl
// tinted.wgsl: a textured, tinted surface
//! cull none
struct Params {
    tint: vec4<f32>,
    nb_time: f32,
};
struct Instance {
    worldMatrix: mat4x4<f32>,
    worldViewProjection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> properties: Params;
@group(0) @binding(1) var<uniform> fyrox_instanceData: Instance;
@group(1) @binding(0) var albedo: texture_2d<f32>;
@group(1) @binding(1) var albedoSampler: sampler;
//! default tint 1 0.5 0.2 1

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var o: VOut;
    o.pos = fyrox_instanceData.worldViewProjection * vec4<f32>(position, 1.0);
    o.uv = uv;
    return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let pulse = 0.75 + 0.25 * sin(properties.nb_time * 3.0);
    return textureSample(albedo, albedoSampler, i.uv) * properties.tint * pulse;
}
```

```basic
tinted = LoadShader("tinted.wgsl")
ShaderSetColor tinted, "tint", 255, 200, 100
ShaderSetTexture tinted, "albedo", LoadTexture("examples/media/checker.png")
ShaderApply tinted, ball
```

The rules:

- **One `@vertex` and one `@fragment` function.** WGSL shaders are
  complete programs: there is no `nb_surface`, no automatic lighting and
  no automatic shadow passes.
- **Parameters** are the members of a `var<uniform>` struct (any name;
  `properties` above). Set them by member name: `ShaderSetColor s,
  "tint", ...`. WGSL declarations cannot have starting values, so give
  them with `//! default name values`.
- **Automatic values**: struct members named `nb_time`, `nb_deltaTime`,
  `nb_resolution`, `nb_ambient`, `nb_fogColor` or `nb_fogRange` are filled
  in every frame.
- **Fyrox's built-in blocks** are `var<uniform>` structs with Fyrox's
  names (`fyrox_instanceData`, `fyrox_cameraData`, ...); declare the
  members you read, with Fyrox's member names, in Fyrox's order.
- **Textures** are a `texture_2d<f32>` (or `texture_cube<f32>`, ...) with a
  `sampler`. The texture's name is the parameter name.
- **Vertex inputs** use the attribute locations in
  [chapter 5](05-shader-language.md#vertex-attributes): `@location(0)` is
  the position, `@location(1)` the UVs, `@location(2)` the normal, and so on.
- **Directives** (`//! cull none`, `//! blend alpha`, ...) work as in GLSL.
- **Keywords**: `#ifdef NAME`, `#ifndef NAME`, `#else` and `#endif` lines
  select code, as in GLSL.
- Clip space is OpenGL's, because the matrices come from Fyrox.
- **Errors** point at your WGSL line: `tinted.wgsl:12: no definition in
  scope for identifier: 'x'`.

Not supported in WGSL: more than one entry point of a kind, array
textures, storage buffers and compute entry points.

`examples/media/shaders/ripple.wgsl` is a complete example.

## Fyrox shader files

A `.shader` file is Fyrox's own format: a description of the shader's
resources (textures and parameter groups) and its passes, written in RON
(a notation like a Rust struct), with the GLSL of each pass inside it.
NeptuneBASIC uses it as it is: its properties become parameters.

```basic
wave = LoadShader("examples/media/wave.shader")
EntityShader mesh, wave
ShaderFloat mesh, "time", MilliSecs() / 1000.0
```

A Fyrox shader has, in outline:

```
(
    name: "MyShader",
    resources: [
        (name: "diffuseTexture", kind: Texture(kind: Sampler2D, fallback: White), binding: 0),
        (name: "properties", kind: PropertyGroup([
            (name: "time", kind: Float(value: 0.0)),
        ]), binding: 0),
        (name: "fyrox_instanceData", kind: PropertyGroup([]), binding: 1),
    ],
    disabled_passes: ["GBuffer", "DirectionalShadow", "PointShadow", "SpotShadow"],
    passes: [
        (
            name: "Forward",
            draw_parameters: DrawParameters( ... ),
            vertex_shader: "...GLSL...",
            fragment_shader: "...GLSL...",
        ),
    ],
)
```

`examples/media/wave.shader` (used by `examples/rendering.nb`) is a full
working one.

You rarely need to write one by hand: every NeptuneBASIC shader is turned
into a Fyrox shader, and you can save it to start from:

```basic
s = LoadShader("builtin:toon")
ShaderSave s, "toon_fyrox.shader"      ; .shader or .ron saves the Fyrox shader
fyroxText$ = ShaderGetSource(s, 3)     ; or get it as text
```

[Contents](../SHADERS.md) · Previous: [7. Shadows](07-shadows.md) · Next: [9. Debugging and performance](09-debugging.md)
