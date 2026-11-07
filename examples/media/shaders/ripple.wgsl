// A WGSL shader: naga checks it and translates it to GLSL for Fyrox.
// Members of a var<uniform> struct are material parameters; a member named
// nb_time is filled in by the runtime every frame.
//! cull none
struct Params {
    color_a: vec4<f32>,
    color_b: vec4<f32>,
    frequency: f32,
    nb_time: f32,
};
struct Instance {
    worldMatrix: mat4x4<f32>,
    worldViewProjection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> properties: Params;
@group(0) @binding(1) var<uniform> fyrox_instanceData: Instance;

//! default color_a 0.1 0.3 0.9 1
//! default color_b 0.9 0.95 1.0 1
//! default frequency 12

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
    let d = distance(i.uv, vec2<f32>(0.5, 0.5));
    let w = 0.5 + 0.5 * sin(d * properties.frequency * 6.2831 - properties.nb_time * 3.0);
    let c = mix(properties.color_a, properties.color_b, w);
    // linear output for Fyrox's HDR pipeline
    return vec4<f32>(pow(c.rgb, vec3<f32>(2.2)), 1.0);
}
