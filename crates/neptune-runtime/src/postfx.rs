//! Post-processing (ext): chains of full-screen shader passes run after
//! the scene is drawn, per camera or for every camera.
//!
//! An effect is a shader object whose pass is `post` (`//! pass post`):
//! its fragment writes `FragColor` or defines `vec4 nb_post(vec2 uv)` and
//! reads `nb_screenTexture` (the image so far), `nb_originalTexture` (the
//! scene before any effect), `nb_depthTexture` and `nb_normalTexture`
//! (G-buffer), with `nb_resolution`, `nb_time`, `nb_cameraPos`, `nb_clip`
//! (near, far), `nb_invViewProj` and `nb_prevViewProj` (last frame, for
//! motion blur). `LoadPostProcess("builtin:<name>")` gives ready-made
//! effects (see `BUILTIN`).
//!
//! Effects run in order (`SetPostProcessOrder`); a negative priority
//! (`PostProcessPriority`) runs the effect on the HDR image before tone
//! mapping, otherwise on the final LDR image. `PostProcessInput` reads the
//! original scene or another effect's output instead of the previous
//! result, `PostProcessOutput` renders into a texture instead of the
//! screen, and `PostProcessResolution` renders at a fraction of the size.
//!
//! The chain is rebuilt into a `Frame` each update; the render pass
//! (registered with Fyrox's renderer) draws it, compiling each effect's
//! GPU program once and reusing its render targets between frames.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use fyrox::{
    core::{
        algebra::{Matrix4, Vector2, Vector3},
        math::Rect,
        pool::Handle,
        sstorage::ImmutableString,
    },
    graphics::{
        error::FrameworkError,
        framebuffer::{Attachment, GpuFrameBuffer},
        gpu_texture::{GpuTexture, GpuTextureDescriptor, GpuTextureKind, PixelKind},
        sampler::GpuSampler,
        server::GraphicsServer,
    },
    material::MaterialProperty,
    renderer::{
        cache::shader::{GpuResourceBinding, NamedValue, PropertyGroup, RenderMaterial, RenderPassContainer},
        make_viewport_matrix, RenderPassStatistics, SceneRenderPass, SceneRenderPassContext,
    },
    resource::texture::TextureResource,
    scene::node::Node,
};
use neptune_vm::value::Value;

use crate::commands::Cmd;
use crate::shader::{parse_numbers, property, ShaderObj, Val};
use crate::shadersrc::Lang;
use crate::world::{Host3D, Kind};

const VOID: Result<Value, String> = Ok(Value::Null);

/// What an effect reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Previous,
    Original,
    Effect(i32),
}

#[derive(Debug, Clone)]
pub struct Effect {
    pub shader: i32,
    pub name: String,
    pub enabled: bool,
    pub order: i32,
    pub priority: i32,
    /// Camera entities (0 = every camera); empty = not in any chain.
    pub cameras: Vec<i32>,
    pub input: Input,
    pub output: i32,
    pub scale: f32,
}

/// One effect ready to draw.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: i32,
    pub key: u64,
    pub ron: Arc<str>,
    pub props: Vec<(String, MaterialProperty)>,
    pub textures: Vec<(String, TextureResource)>,
    /// Fyrox camera nodes; empty = every camera.
    pub cameras: Vec<Handle<Node>>,
    pub hdr: bool,
    pub input: Input,
    pub output: Option<TextureResource>,
    pub scale: f32,
}

#[derive(Debug, Default)]
pub struct Frame {
    pub entries: Vec<Entry>,
    pub time: f32,
}

#[derive(Debug, Default)]
pub struct PostFx {
    pub effects: Vec<Option<Effect>>,
    pub frame: Rc<RefCell<Frame>>,
    /// The render pass was given to Fyrox.
    pub registered: bool,
    /// Programs that failed on the GPU (by key), reported once.
    pub failed: Rc<RefCell<HashMap<u64, String>>>,
}

impl PostFx {
    /// Removes effects whose shader was freed.
    pub fn forget_shader(&mut self, s: i32) {
        for e in self.effects.iter_mut() {
            if e.as_ref().is_some_and(|e| e.shader == s) {
                *e = None;
            }
        }
    }
}

/// Built-in effects: name, default priority, source.
pub const BUILTIN: &[(&str, i32, &str)] = &[
    ("bloom", 0, "uniform float threshold = 0.7;
uniform float intensity = 0.8;
uniform float radius = 6.0;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 base = texture(nb_screenTexture, uv).rgb;
    vec3 glow = vec3(0.0);
    float total = 0.0;
    for (int ring = 1; ring <= 3; ++ring) {
        for (int i = 0; i < 8; ++i) {
            float a = float(i) * 0.785398 + float(ring) * 0.4;
            vec2 o = vec2(cos(a), sin(a)) * radius * float(ring) * nb_texel();
            vec3 c = texture(nb_screenTexture, uv + o).rgb;
            float w = 1.0 / float(ring);
            glow += max(c - threshold, 0.0) * w;
            total += w;
        }
    }
    return vec4(base + glow / total * intensity * 4.0, 1.0);
}
"),
    ("exposure", -1, "uniform float exposure = 0.0;
//! fragment
vec4 nb_post(vec2 uv) { return vec4(texture(nb_screenTexture, uv).rgb * exp2(exposure), 1.0); }
"),
    ("tonemap", -1, "// runs on the HDR image; mode 0 ACES, 1 Reinhard, 2 Uncharted 2, 3 none
uniform int mode = 0;
uniform float exposure = 0.0;
//! fragment
vec3 nb_uncharted(vec3 x) { const float A = 0.15, B = 0.50, C = 0.10, D = 0.20, E = 0.02, F = 0.30; return ((x * (A * x + C * B) + D * E) / (x * (A * x + B) + D * F)) - E / F; }
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb * exp2(exposure);
    if (mode == 0) c = clamp((c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14), 0.0, 1.0);
    else if (mode == 1) c = c / (1.0 + c);
    else if (mode == 2) c = nb_uncharted(c * 2.0) / nb_uncharted(vec3(11.2));
    return vec4(c, 1.0);
}
"),
    ("vignette", 0, "uniform float intensity = 0.45;
uniform float smoothness = 0.45;
uniform vec3 color = vec3(0.0, 0.0, 0.0);
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    vec2 d = (uv - 0.5) * vec2(nb_resolution.x / nb_resolution.y, 1.0);
    float v = smoothstep(0.8, 0.8 - smoothness, length(d) * (1.0 + intensity));
    return vec4(mix(color, c, mix(1.0, v, intensity * 2.0)), 1.0);
}
"),
    ("chromatic", 0, "uniform float amount = 0.004;
//! fragment
vec4 nb_post(vec2 uv) {
    vec2 d = (uv - 0.5) * amount * 2.0;
    return vec4(texture(nb_screenTexture, uv + d).r, texture(nb_screenTexture, uv).g, texture(nb_screenTexture, uv - d).b, 1.0);
}
"),
    ("grain", 0, "uniform float amount = 0.06;
uniform float size = 1.5;
//! fragment
float nb_rand(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    vec2 p = floor(uv * nb_resolution / max(size, 1.0));
    float n = nb_rand(p + fract(nb_time * 13.7)) - 0.5;
    return vec4(c + n * amount, 1.0);
}
"),
    ("colorgrade", 0, "uniform vec3 lift = vec3(0.0, 0.0, 0.0);
uniform vec3 gammaRGB = vec3(1.0, 1.0, 1.0);
uniform vec3 gain = vec3(1.0, 1.0, 1.0);
uniform float temperature = 0.0;
uniform float tint = 0.0;
uniform float contrast = 1.0;
uniform float saturation = 1.0;
uniform float brightness = 0.0;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    c *= vec3(1.0 + temperature * 0.1, 1.0 + tint * 0.1, 1.0 - temperature * 0.1);
    c = gain * (c + lift * (1.0 - c));
    c = pow(max(c, 0.0), 1.0 / max(gammaRGB, vec3(0.01)));
    c = (c - 0.5) * contrast + 0.5 + brightness;
    c = mix(vec3(nb_luma(c)), c, saturation);
    return vec4(c, 1.0);
}
"),
    ("gamma", 0, "uniform float gamma = 1.0;
//! fragment
vec4 nb_post(vec2 uv) { return vec4(pow(max(texture(nb_screenTexture, uv).rgb, 0.0), vec3(1.0 / max(gamma, 0.01))), 1.0); }
"),
    ("contrast", 0, "uniform float contrast = 1.2;
//! fragment
vec4 nb_post(vec2 uv) { return vec4((texture(nb_screenTexture, uv).rgb - 0.5) * contrast + 0.5, 1.0); }
"),
    ("saturation", 0, "uniform float saturation = 1.3;
//! fragment
vec4 nb_post(vec2 uv) { vec3 c = texture(nb_screenTexture, uv).rgb; return vec4(mix(vec3(nb_luma(c)), c, saturation), 1.0); }
"),
    ("brightness", 0, "uniform float brightness = 0.1;
//! fragment
vec4 nb_post(vec2 uv) { return vec4(texture(nb_screenTexture, uv).rgb + brightness, 1.0); }
"),
    ("fxaa", 0, "uniform float subpix = 0.75;
uniform float edgeThreshold = 0.125;
//! fragment
vec4 nb_post(vec2 uv) {
    vec2 t = nb_texel();
    vec3 c = texture(nb_screenTexture, uv).rgb;
    float lM = nb_luma(c);
    float lN = nb_luma(texture(nb_screenTexture, uv + vec2(0.0, t.y)).rgb);
    float lS = nb_luma(texture(nb_screenTexture, uv - vec2(0.0, t.y)).rgb);
    float lE = nb_luma(texture(nb_screenTexture, uv + vec2(t.x, 0.0)).rgb);
    float lW = nb_luma(texture(nb_screenTexture, uv - vec2(t.x, 0.0)).rgb);
    float lo = min(lM, min(min(lN, lS), min(lE, lW)));
    float hi = max(lM, max(max(lN, lS), max(lE, lW)));
    float range = hi - lo;
    if (range < max(0.0312, hi * edgeThreshold)) return vec4(c, 1.0);
    bool horizontal = abs(lN + lS - 2.0 * lM) >= abs(lE + lW - 2.0 * lM);
    float blend = clamp(abs((lN + lS + lE + lW) * 0.25 - lM) / range, 0.0, 1.0);
    blend = smoothstep(0.0, 1.0, blend) * smoothstep(0.0, 1.0, blend) * subpix;
    float g1 = horizontal ? lN : lE;
    float g2 = horizontal ? lS : lW;
    vec2 step1 = horizontal ? vec2(0.0, t.y) : vec2(t.x, 0.0);
    if (abs(g2 - lM) > abs(g1 - lM)) step1 = -step1;
    return vec4(mix(c, texture(nb_screenTexture, uv + step1 * 0.5).rgb, max(blend, 0.5)), 1.0);
}
"),
    ("outline", 0, "uniform float thickness = 1.0;
uniform vec3 color = vec3(0.0, 0.0, 0.0);
uniform float depthThreshold = 0.1;
uniform float normalThreshold = 0.4;
//! fragment
vec4 nb_post(vec2 uv) {
    vec2 t = nb_texel() * thickness;
    float d = nb_linearDepth(uv);
    vec3 n = nb_normalAt(uv);
    float edge = 0.0;
    vec2 offs[4] = vec2[](vec2(t.x, 0.0), vec2(-t.x, 0.0), vec2(0.0, t.y), vec2(0.0, -t.y));
    for (int i = 0; i < 4; ++i) {
        float dd = abs(nb_linearDepth(uv + offs[i]) - d) / max(d, 0.001);
        float nd = 1.0 - dot(n, nb_normalAt(uv + offs[i]));
        edge = max(edge, max(step(depthThreshold, dd), step(normalThreshold, nd)));
    }
    vec3 c = texture(nb_screenTexture, uv).rgb;
    return vec4(mix(c, color, edge), 1.0);
}
"),
    ("pixelate", 0, "uniform float pixelSize = 6.0;
//! fragment
vec4 nb_post(vec2 uv) {
    vec2 cells = nb_resolution / max(pixelSize, 1.0);
    return vec4(texture(nb_screenTexture, (floor(uv * cells) + 0.5) / cells).rgb, 1.0);
}
"),
    ("crt", 0, "uniform float curvature = 0.12;
uniform float scanlines = 0.35;
uniform float mask = 0.15;
uniform float vignette = 0.3;
//! fragment
vec4 nb_post(vec2 uv) {
    vec2 p = uv * 2.0 - 1.0;
    p *= 1.0 + curvature * dot(p, p) * 0.25;
    vec2 q = p * 0.5 + 0.5;
    if (q.x < 0.0 || q.x > 1.0 || q.y < 0.0 || q.y > 1.0) return vec4(0.0, 0.0, 0.0, 1.0);
    vec3 c = texture(nb_screenTexture, q).rgb;
    c *= 1.0 - scanlines * (0.5 + 0.5 * cos(q.y * nb_resolution.y * 3.14159));
    float m = mod(floor(q.x * nb_resolution.x), 3.0);
    c *= 1.0 - mask + mask * 3.0 * vec3(m == 0.0 ? 1.0 : 0.0, m == 1.0 ? 1.0 : 0.0, m == 2.0 ? 1.0 : 0.0);
    c *= 1.0 - vignette * dot(p, p) * 0.5;
    return vec4(c, 1.0);
}
"),
    ("distortion", 0, "uniform float amount = 0.01;
uniform float scale = 8.0;
uniform float speed = 1.0;
//! fragment
vec4 nb_post(vec2 uv) {
    vec2 o = vec2(sin(uv.y * scale * 6.2831 + nb_time * speed * 2.0), cos(uv.x * scale * 6.2831 + nb_time * speed * 1.7)) * amount;
    return vec4(texture(nb_screenTexture, uv + o).rgb, 1.0);
}
"),
    ("heathaze", 0, "// shimmer that grows with distance and towards the bottom of the screen
uniform float amount = 0.006;
uniform float speed = 2.0;
uniform float scale = 40.0;
uniform float startDistance = 10.0;
//! fragment
vec4 nb_post(vec2 uv) {
    float d = nb_linearDepth(uv);
    float k = clamp((d - startDistance) / max(startDistance, 0.01), 0.0, 1.0) * (1.0 - uv.y * 0.5);
    vec2 o = vec2(sin(uv.y * scale + nb_time * speed), cos(uv.x * scale * 0.7 + nb_time * speed * 1.3)) * amount * k;
    return vec4(texture(nb_screenTexture, uv + o).rgb, 1.0);
}
"),
    ("blur", 0, "uniform float radius = 2.0;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = vec3(0.0);
    for (int y = -2; y <= 2; ++y)
        for (int x = -2; x <= 2; ++x)
            c += texture(nb_screenTexture, uv + vec2(x, y) * radius * 0.5 * nb_texel()).rgb;
    return vec4(c / 25.0, 1.0);
}
"),
    ("gaussian", 0, "uniform float radius = 3.0;
//! fragment
vec4 nb_post(vec2 uv) {
    float sigma = max(radius, 0.5);
    vec3 c = vec3(0.0);
    float total = 0.0;
    for (int y = -4; y <= 4; ++y)
        for (int x = -4; x <= 4; ++x) {
            vec2 o = vec2(x, y) * sigma * 0.5;
            float w = exp(-dot(o, o) / (2.0 * sigma * sigma));
            c += texture(nb_screenTexture, uv + o * nb_texel()).rgb * w;
            total += w;
        }
    return vec4(c / total, 1.0);
}
"),
    ("radialblur", 0, "uniform float amount = 0.08;
uniform vec2 center = vec2(0.5, 0.5);
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = vec3(0.0);
    vec2 d = uv - center;
    for (int i = 0; i < 16; ++i) c += texture(nb_screenTexture, center + d * (1.0 - amount * float(i) / 15.0)).rgb;
    return vec4(c / 16.0, 1.0);
}
"),
    ("sharpen", 0, "uniform float amount = 0.6;
//! fragment
vec4 nb_post(vec2 uv) {
    vec2 t = nb_texel();
    vec3 c = texture(nb_screenTexture, uv).rgb;
    vec3 n = texture(nb_screenTexture, uv + vec2(t.x, 0.0)).rgb + texture(nb_screenTexture, uv - vec2(t.x, 0.0)).rgb
           + texture(nb_screenTexture, uv + vec2(0.0, t.y)).rgb + texture(nb_screenTexture, uv - vec2(0.0, t.y)).rgb;
    return vec4(c + (c * 4.0 - n) * amount, 1.0);
}
"),
    ("dof", 0, "uniform float focusDistance = 10.0;
uniform float focusRange = 6.0;
uniform float maxBlur = 6.0;
//! fragment
vec4 nb_post(vec2 uv) {
    float d = nb_linearDepth(uv);
    float coc = clamp(abs(d - focusDistance) / max(focusRange, 0.01) - 0.5, 0.0, 1.0) * maxBlur;
    vec3 c = texture(nb_screenTexture, uv).rgb;
    if (coc < 0.5) return vec4(c, 1.0);
    vec3 sum = c;
    float total = 1.0;
    for (int i = 0; i < 24; ++i) {
        float a = float(i) * 2.39996;
        float r = sqrt(float(i + 1) / 24.0) * coc;
        vec2 o = vec2(cos(a), sin(a)) * r * nb_texel();
        sum += texture(nb_screenTexture, uv + o).rgb;
        total += 1.0;
    }
    return vec4(sum / total, 1.0);
}
"),
    ("motionblur", 0, "uniform float amount = 1.0;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 world = nb_worldPosAt(uv);
    vec4 prev = nb_prevViewProj * vec4(world, 1.0);
    vec2 puv = prev.xy / prev.w * 0.5 + 0.5;
    vec2 v = (uv - puv) * amount;
    vec3 c = vec3(0.0);
    for (int i = 0; i < 12; ++i) c += texture(nb_screenTexture, uv - v * (float(i) / 11.0 - 0.5)).rgb;
    return vec4(c / 12.0, 1.0);
}
"),
    ("ssr", 0, "// screen-space reflections from the depth and normal buffers
uniform float intensity = 0.5;
uniform float maxDistance = 20.0;
uniform float thickness = 0.4;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    float depth = texture(nb_depthTexture, uv).r;
    if (depth >= 1.0) return vec4(c, 1.0);
    vec3 p = nb_worldPosAt(uv);
    vec3 n = nb_normalAt(uv);
    vec3 v = normalize(p - nb_cameraPos);
    vec3 r = reflect(v, n);
    mat4 vp = inverse(nb_invViewProj);
    vec3 hit = vec3(0.0);
    float found = 0.0;
    for (int i = 1; i <= 32; ++i) {
        vec3 q = p + r * (maxDistance * float(i) / 32.0);
        vec4 s = vp * vec4(q, 1.0);
        vec2 suv = s.xy / s.w * 0.5 + 0.5;
        if (suv.x < 0.0 || suv.x > 1.0 || suv.y < 0.0 || suv.y > 1.0) break;
        vec3 scene = nb_worldPosAt(suv);
        float qd = distance(q, nb_cameraPos), sd = distance(scene, nb_cameraPos);
        if (qd > sd && qd - sd < thickness * float(i)) { hit = texture(nb_screenTexture, suv).rgb; found = 1.0 - float(i) / 32.0; break; }
    }
    float f = pow(1.0 - max(dot(-v, n), 0.0), 3.0);
    return vec4(c + hit * found * f * intensity, 1.0);
}
"),
    ("ssao", 0, "uniform float radius = 0.6;
uniform float intensity = 1.0;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    if (texture(nb_depthTexture, uv).r >= 1.0) return vec4(c, 1.0);
    vec3 p = nb_worldPosAt(uv);
    vec3 n = nb_normalAt(uv);
    mat4 vp = inverse(nb_invViewProj);
    float occ = 0.0;
    for (int i = 0; i < 12; ++i) {
        float a = float(i) * 2.39996;
        vec3 dir = normalize(vec3(cos(a), sin(a * 1.3) * 0.5, sin(a)) + n);
        vec3 q = p + dir * radius * (float(i % 3) + 1.0) / 3.0;
        vec4 s = vp * vec4(q, 1.0);
        vec2 suv = s.xy / s.w * 0.5 + 0.5;
        vec3 scene = nb_worldPosAt(suv);
        float qd = distance(q, nb_cameraPos), sd = distance(scene, nb_cameraPos);
        occ += step(sd + 0.02, qd) * smoothstep(0.0, 1.0, radius / max(abs(qd - sd), 0.001));
    }
    return vec4(c * (1.0 - clamp(occ / 12.0 * intensity, 0.0, 0.9)), 1.0);
}
"),
    ("fog", 0, "// atmospheric height fog from the depth buffer
uniform float density = 0.03;
uniform vec3 color = vec3(0.65, 0.72, 0.8);
uniform float heightFalloff = 0.08;
uniform float start = 0.0;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    vec3 p = nb_worldPosAt(uv);
    float d = max(distance(p, nb_cameraPos) - start, 0.0);
    if (texture(nb_depthTexture, uv).r >= 1.0) d = 1.0e4;
    float h = exp(-max(p.y, 0.0) * heightFalloff);
    float f = 1.0 - exp(-d * density * h);
    return vec4(mix(c, color, clamp(f, 0.0, 1.0)), 1.0);
}
"),
    ("underwater", 0, "// tint and waviness when the camera is below waterLevel
uniform float waterLevel = 0.0;
uniform vec3 color = vec3(0.1, 0.35, 0.45);
uniform float density = 0.08;
uniform float distortion = 0.004;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    if (nb_cameraPos.y >= waterLevel) return vec4(c, 1.0);
    vec2 o = vec2(sin(uv.y * 30.0 + nb_time * 2.0), cos(uv.x * 25.0 + nb_time * 1.7)) * distortion;
    c = texture(nb_screenTexture, uv + o).rgb;
    float d = distance(nb_worldPosAt(uv), nb_cameraPos);
    float f = 1.0 - exp(-d * density);
    return vec4(mix(c * vec3(0.7, 0.9, 1.0), color, clamp(f, 0.2, 1.0)), 1.0);
}
"),
    ("rain", 0, "uniform float amount = 0.5;
uniform float speed = 1.0;
uniform float angle = 0.15;
//! fragment
float nb_rand(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    float streaks = 0.0;
    for (int layer = 0; layer < 3; ++layer) {
        float s = 60.0 + float(layer) * 40.0;
        vec2 p = vec2(uv.x + uv.y * angle, uv.y * 0.15 + nb_time * speed * (1.2 + float(layer) * 0.4));
        vec2 cell = floor(p * s);
        float r = nb_rand(cell);
        float drop = step(1.0 - amount * 0.25, r) * smoothstep(0.0, 0.5, fract(p.y * s)) * (1.0 - fract(p.y * s));
        streaks += drop * (0.5 - float(layer) * 0.12);
    }
    return vec4(mix(c, vec3(0.8, 0.85, 0.9), clamp(streaks, 0.0, 0.6)), 1.0);
}
"),
    ("snow", 0, "uniform float amount = 0.5;
uniform float speed = 0.3;
uniform float size = 1.0;
//! fragment
float nb_rand(vec2 p) { return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    float flakes = 0.0;
    vec2 aspect = vec2(nb_resolution.x / nb_resolution.y, 1.0);
    for (int layer = 0; layer < 4; ++layer) {
        float s = (8.0 + float(layer) * 6.0) / max(size, 0.1);
        vec2 p = uv * aspect * s + vec2(sin(nb_time * 0.5 + float(layer)) * 0.5, nb_time * speed * (2.0 + float(layer)));
        vec2 cell = floor(p);
        vec2 f = fract(p) - vec2(nb_rand(cell), nb_rand(cell + 3.1));
        float present = step(1.0 - amount * 0.6, nb_rand(cell + 7.7));
        flakes += present * smoothstep(0.08, 0.0, length(f + 0.5 - 0.5)) * (1.0 - float(layer) * 0.18);
    }
    return vec4(mix(c, vec3(1.0), clamp(flakes, 0.0, 1.0)), 1.0);
}
"),
    ("invert", 0, "//! fragment
vec4 nb_post(vec2 uv) { return vec4(1.0 - texture(nb_screenTexture, uv).rgb, 1.0); }
"),
];

/// Source and default priority of a built-in effect.
pub fn builtin(name: &str) -> Option<(String, i32)> {
    let name = name.to_ascii_lowercase();
    let n = match name.as_str() {
        "hdr" => "exposure",
        "tonemapping" => "tonemap",
        "chromaticaberration" => "chromatic",
        "filmgrain" => "grain",
        "colorgrading" => "colorgrade",
        "depthoffield" => "dof",
        "gaussianblur" => "gaussian",
        "pixelation" => "pixelate",
        other => other,
    };
    BUILTIN.iter().find(|(b, _, _)| *b == n).map(|(_, p, s)| (format!("//! pass post\n{s}"), *p))
}

// ---- the render pass

struct Target {
    texture: GpuTexture,
    fb: GpuFrameBuffer,
    size: (usize, usize),
}

fn make_target(server: &dyn GraphicsServer, w: usize, h: usize, hdr: bool) -> Result<Target, FrameworkError> {
    let texture = server.create_texture(GpuTextureDescriptor {
        name: "NeptunePostTarget",
        kind: GpuTextureKind::Rectangle { width: w, height: h },
        pixel_kind: if hdr { PixelKind::RGBA16F } else { PixelKind::RGBA8 },
        ..Default::default()
    })?;
    let fb = server.create_frame_buffer(None, vec![Attachment::color(texture.clone())])?;
    Ok(Target { texture, fb, size: (w, h) })
}

/// Fyrox render pass that draws the post-processing chains.
pub struct PostPass {
    pub frame: Rc<RefCell<Frame>>,
    pub failed: Rc<RefCell<HashMap<u64, String>>>,
    programs: HashMap<u64, RenderPassContainer>,
    /// ping-pong, original copy, and per-effect outputs; LDR and HDR sets
    targets: HashMap<(bool, usize), Target>,
    outputs: HashMap<i32, Target>,
    prev_vp: HashMap<Handle<Node>, Matrix4<f32>>,
    tex_fbs: HashMap<usize, GpuFrameBuffer>,
    plugin: TypeId,
}

impl PostPass {
    pub fn new(frame: Rc<RefCell<Frame>>, failed: Rc<RefCell<HashMap<u64, String>>>, plugin: TypeId) -> Self {
        PostPass { frame, failed, programs: HashMap::new(), targets: HashMap::new(), outputs: HashMap::new(), prev_vp: HashMap::new(), tex_fbs: HashMap::new(), plugin }
    }

    fn target(&mut self, server: &dyn GraphicsServer, hdr: bool, slot: usize, w: usize, h: usize) -> Result<(GpuTexture, GpuFrameBuffer), FrameworkError> {
        let t = match self.targets.get(&(hdr, slot)) {
            Some(t) if t.size == (w, h) => t,
            _ => {
                let t = make_target(server, w, h, hdr)?;
                self.targets.insert((hdr, slot), t);
                &self.targets[&(hdr, slot)]
            }
        };
        Ok((t.texture.clone(), t.fb.clone()))
    }

    fn blit(ctx: &mut SceneRenderPassContext, src: &GpuTexture, dst: &GpuFrameBuffer, viewport: Rect<i32>) -> Result<RenderPassStatistics, FrameworkError> {
        let wvp = make_viewport_matrix(viewport);
        let props = PropertyGroup::from([NamedValue::new("worldViewProjection", (&wvp).into())]);
        let material = RenderMaterial::from([
            NamedValue::new("diffuseTexture", GpuResourceBinding::texture(src, &ctx.renderer_resources.nearest_clamp_sampler)),
            NamedValue::new("properties", GpuResourceBinding::property_group(&props)),
        ]);
        let mut stats = RenderPassStatistics::default();
        stats += ctx.renderer_resources.shaders.blit.run_pass(
            1,
            &ImmutableString::new("Primary"),
            dst,
            &ctx.renderer_resources.quad,
            viewport,
            &material,
            ctx.uniform_buffer_cache,
            Default::default(),
            None,
        )?;
        Ok(stats)
    }

    fn run(&mut self, ctx: &mut SceneRenderPassContext, hdr: bool) -> Result<RenderPassStatistics, FrameworkError> {
        let mut stats = RenderPassStatistics::default();
        let cam = ctx.observer.handle;
        let frame = self.frame.clone();
        let frame = frame.borrow();
        let entries: Vec<&Entry> = frame.entries.iter().filter(|e| e.hdr == hdr && (e.cameras.is_empty() || e.cameras.contains(&cam))).collect();
        let vp_now = ctx.observer.position.view_projection_matrix;
        if entries.is_empty() {
            if !hdr {
                self.prev_vp.insert(cam, vp_now);
            }
            return Ok(stats);
        }
        let server = ctx.server;
        let viewport = ctx.observer.viewport;
        let screen = ctx.framebuffer.color_attachments()[0].texture.clone();
        let (fw, fh) = match screen.kind() {
            GpuTextureKind::Rectangle { width, height } => (width, height),
            _ => return Ok(stats),
        };
        let (original, original_fb) = self.target(server, hdr, 2, fw, fh)?;
        stats += Self::blit(ctx, &screen, &original_fb, viewport)?;
        let mut current = original.clone();
        let mut ping = 0usize;
        let mut on_screen = false;
        let inv_vp = vp_now.try_inverse().unwrap_or_else(Matrix4::identity);
        let prev_vp = self.prev_vp.get(&cam).copied().unwrap_or(vp_now);
        let cam_pos = ctx.observer.position.translation;
        let clip = Vector2::new(ctx.observer.position.z_near, ctx.observer.position.z_far);
        let last = entries.len() - 1;
        for (i, e) in entries.iter().enumerate() {
            if !self.programs.contains_key(&e.key) {
                if self.failed.borrow().contains_key(&e.key) {
                    continue;
                }
                match RenderPassContainer::from_str(server, &e.ron) {
                    Ok(p) => {
                        self.programs.insert(e.key, p);
                    }
                    Err(err) => {
                        eprintln!("post-process effect {}: {err}", e.id);
                        self.failed.borrow_mut().insert(e.key, err.to_string());
                        continue;
                    }
                }
            }
            let input = match e.input {
                Input::Previous => current.clone(),
                Input::Original => original.clone(),
                Input::Effect(id) => self.outputs.get(&id).map(|t| t.texture.clone()).unwrap_or_else(|| current.clone()),
            };
            // where it draws
            let scaled = (e.scale - 1.0).abs() > 1e-3;
            let (dest_fb, dest_tex, dest_vp, keep) = if let Some(out) = &e.output {
                let Some(data) = ctx.texture_cache.get(server, ctx.resource_manager, out) else { continue };
                let tex = data.gpu_texture.clone();
                let (tw, th) = match tex.kind() {
                    GpuTextureKind::Rectangle { width, height } => (width, height),
                    _ => continue,
                };
                let key = Rc::as_ptr(&tex.0) as *const () as usize;
                let fb = match self.tex_fbs.get(&key) {
                    Some(fb) => fb.clone(),
                    None => {
                        let fb = server.create_frame_buffer(None, vec![Attachment::color(tex.clone())])?;
                        self.tex_fbs.insert(key, fb.clone());
                        fb
                    }
                };
                (fb, tex, Rect::new(0, 0, tw as i32, th as i32), false)
            } else if scaled {
                let w = ((fw as f32 * e.scale).round() as usize).max(1);
                let h = ((fh as f32 * e.scale).round() as usize).max(1);
                let t = match self.outputs.get(&e.id) {
                    Some(t) if t.size == (w, h) => t,
                    _ => {
                        self.outputs.insert(e.id, make_target(server, w, h, hdr)?);
                        &self.outputs[&e.id]
                    }
                };
                let r = Rect::new(
                    (viewport.x() as f32 * e.scale) as i32,
                    (viewport.y() as f32 * e.scale) as i32,
                    ((viewport.w() as f32 * e.scale) as i32).max(1),
                    ((viewport.h() as f32 * e.scale) as i32).max(1),
                );
                (t.fb.clone(), t.texture.clone(), r, true)
            } else if i == last {
                (ctx.framebuffer.clone(), screen.clone(), viewport, true)
            } else {
                let (t, fb) = self.target(server, hdr, ping, fw, fh)?;
                ping ^= 1;
                (fb, t, viewport, true)
            };
            let wvp = make_viewport_matrix(dest_vp);
            let resolution = Vector2::new(dest_vp.w() as f32, dest_vp.h() as f32);
            // properties: the effect's values, then the frame values
            let mut props: Vec<(&str, MaterialProperty)> = e.props.iter().map(|(n, p)| (n.as_str(), p.clone())).collect();
            props.push(("nb_wvp", MaterialProperty::Matrix4(wvp)));
            props.push(("nb_invViewProj", MaterialProperty::Matrix4(inv_vp)));
            props.push(("nb_prevViewProj", MaterialProperty::Matrix4(prev_vp)));
            props.push(("nb_cameraPos", MaterialProperty::Vector3(Vector3::new(cam_pos.x, cam_pos.y, cam_pos.z))));
            props.push(("nb_clip", MaterialProperty::Vector2(clip)));
            props.push(("nb_resolution", MaterialProperty::Vector2(resolution)));
            props.push(("nb_time", MaterialProperty::Float(frame.time)));
            props.truncate(PROPS);
            let pad = MaterialProperty::Float(0.0);
            let group: [NamedValue<_>; PROPS] = std::array::from_fn(|k| match props.get(k) {
                Some((n, p)) => NamedValue::new(*n, p.as_ref()),
                None => NamedValue::new(PAD_NAMES[k], pad.as_ref()),
            });
            let group = PropertyGroup::from(group);
            // textures
            let mut user: Vec<(&str, GpuTexture, GpuSampler)> = Vec::new();
            for (name, res) in &e.textures {
                if let Some(d) = ctx.texture_cache.get(server, ctx.resource_manager, res) {
                    user.push((name.as_str(), d.gpu_texture.clone(), d.gpu_sampler.clone()));
                }
            }
            let linear = ctx.renderer_resources.linear_clamp_sampler.clone();
            let nearest = ctx.renderer_resources.nearest_clamp_sampler.clone();
            let mut tex: Vec<(&str, GpuTexture, GpuSampler)> = vec![
                ("nb_screenTexture", input.clone(), linear.clone()),
                ("nb_originalTexture", original.clone(), linear.clone()),
                ("nb_depthTexture", ctx.depth_texture.clone(), nearest.clone()),
                ("nb_normalTexture", ctx.normal_texture.clone(), nearest.clone()),
            ];
            tex.extend(user);
            tex.truncate(TEXTURES);
            let white = ctx.renderer_resources.white_dummy.clone();
            let bindings: [NamedValue<GpuResourceBinding>; TEXTURES + 1] = std::array::from_fn(|k| {
                if k == TEXTURES {
                    NamedValue::new("properties", GpuResourceBinding::property_group(&group))
                } else {
                    match tex.get(k) {
                        Some((n, t, s)) => NamedValue::new(*n, GpuResourceBinding::texture(t, s)),
                        None => NamedValue::new(PAD_TEX[k], GpuResourceBinding::texture(&white, &nearest)),
                    }
                }
            });
            let material = RenderMaterial::from(bindings);
            let program = &self.programs[&e.key];
            stats += program.run_pass(1, &ImmutableString::new("Primary"), &dest_fb, &ctx.renderer_resources.quad, dest_vp, &material, ctx.uniform_buffer_cache, Default::default(), None)?;
            if keep {
                current = dest_tex;
                on_screen = i == last && !scaled;
            }
        }
        // the last effect drew elsewhere: put the result on screen
        if !on_screen && !Rc::ptr_eq(&current.0, &screen.0) {
            stats += Self::blit(ctx, &current, &ctx.framebuffer.clone(), viewport)?;
        }
        if !hdr {
            self.prev_vp.insert(cam, vp_now);
        }
        Ok(stats)
    }
}

const PROPS: usize = 64;
const TEXTURES: usize = 16;
const PAD_NAMES: [&str; PROPS] = {
    // names nothing declares, to fill the fixed-size property group
    [
        "nb_p0", "nb_p1", "nb_p2", "nb_p3", "nb_p4", "nb_p5", "nb_p6", "nb_p7", "nb_p8", "nb_p9", "nb_p10", "nb_p11", "nb_p12", "nb_p13", "nb_p14", "nb_p15", "nb_p16",
        "nb_p17", "nb_p18", "nb_p19", "nb_p20", "nb_p21", "nb_p22", "nb_p23", "nb_p24", "nb_p25", "nb_p26", "nb_p27", "nb_p28", "nb_p29", "nb_p30", "nb_p31", "nb_p32",
        "nb_p33", "nb_p34", "nb_p35", "nb_p36", "nb_p37", "nb_p38", "nb_p39", "nb_p40", "nb_p41", "nb_p42", "nb_p43", "nb_p44", "nb_p45", "nb_p46", "nb_p47", "nb_p48",
        "nb_p49", "nb_p50", "nb_p51", "nb_p52", "nb_p53", "nb_p54", "nb_p55", "nb_p56", "nb_p57", "nb_p58", "nb_p59", "nb_p60", "nb_p61", "nb_p62", "nb_p63",
    ]
};
const PAD_TEX: [&str; TEXTURES] = [
    "nb_t0", "nb_t1", "nb_t2", "nb_t3", "nb_t4", "nb_t5", "nb_t6", "nb_t7", "nb_t8", "nb_t9", "nb_t10", "nb_t11", "nb_t12", "nb_t13", "nb_t14", "nb_t15",
];

impl SceneRenderPass for PostPass {
    fn on_hdr_render(&mut self, mut ctx: SceneRenderPassContext) -> Result<RenderPassStatistics, FrameworkError> {
        self.run(&mut ctx, true)
    }

    fn on_ldr_render(&mut self, mut ctx: SceneRenderPassContext) -> Result<RenderPassStatistics, FrameworkError> {
        self.run(&mut ctx, false)
    }

    fn source_type_id(&self) -> TypeId {
        self.plugin
    }
}

// ---- program side

impl Host3D<'_> {
    pub fn effect(&self, e: i32) -> Result<&Effect, String> {
        usize::try_from(e - 1).ok().and_then(|i| self.world.postfx.effects.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Post-process effect {e} does not exist"))
    }

    pub fn effect_mut(&mut self, e: i32) -> Result<&mut Effect, String> {
        usize::try_from(e - 1).ok().and_then(|i| self.world.postfx.effects.get_mut(i)).and_then(Option::as_mut).ok_or_else(|| format!("Post-process effect {e} does not exist"))
    }

    /// Makes an effect from a post shader object.
    fn new_effect(&mut self, shader: i32, name: &str, priority: i32) -> Result<i32, String> {
        self.ensure_compiled(shader)?;
        let o = self.shader_obj(shader)?;
        if o.resource.is_some() && !o.is_post() {
            return Err(format!("{name} is not a post-processing shader (it needs //! pass post)"));
        }
        if o.resource.is_none() {
            eprintln!("post-process effect {name}: {}", o.error);
        }
        let fx = Effect { shader, name: name.to_string(), enabled: true, order: 0, priority, cameras: Vec::new(), input: Input::Previous, output: 0, scale: 1.0 };
        let list = &mut self.world.postfx.effects;
        let id = match list.iter().position(Option::is_none) {
            Some(i) => {
                list[i] = Some(fx);
                i as i32 + 1
            }
            None => {
                list.push(Some(fx));
                list.len() as i32
            }
        };
        let n = self.world.postfx.effects.iter().flatten().count() as i32;
        self.effect_mut(id)?.order = n;
        Ok(id)
    }

    /// `CreatePostProcess`: an effect from source (`//! pass post` added).
    pub fn create_post(&mut self, source: &str, lang: Lang) -> Result<i32, String> {
        let src = if lang == Lang::Glsl && !source.contains("//! pass post") { format!("//! pass post\n{source}") } else { source.to_string() };
        let s = self.add_shader(ShaderObj::new("<post>", lang, src));
        self.new_effect(s, "<post>", 0)
    }

    /// `LoadPostProcess`: a file or `builtin:<name>`.
    pub fn load_post(&mut self, path: &str) -> Result<i32, String> {
        if let Some(name) = path.strip_prefix("builtin:") {
            let (src, priority) = builtin(name).ok_or_else(|| format!("there is no built-in effect '{name}'"))?;
            let mut o = ShaderObj::new(name, Lang::Glsl, src);
            o.preset = Some(format!("post:{name}"));
            let s = self.add_shader(o);
            return self.new_effect(s, name, priority);
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
        let lang = Lang::from_path(path);
        let src = if lang == Lang::Glsl && !text.contains("//! pass post") { format!("//! pass post\n{text}") } else { text };
        let mut o = ShaderObj::new(path, lang, src);
        o.path = Some(path.into());
        let s = self.add_shader(o);
        self.new_effect(s, path, 0)
    }

    fn camera_ok(&self, cam: i32) -> Result<(), String> {
        if cam == 0 {
            return Ok(());
        }
        match self.ent(cam)?.kind {
            Kind::Camera { .. } => Ok(()),
            _ => Err(format!("Entity {cam} is not a camera")),
        }
    }

    /// Rebuilds the frame the render pass draws (after the program ran).
    pub fn update_postfx(&mut self) -> Result<(), String> {
        let mut entries = Vec::new();
        let mut list: Vec<(i32, Effect)> = self.world.postfx.effects.iter().enumerate().filter_map(|(i, e)| e.clone().map(|e| (i as i32 + 1, e))).collect();
        list.sort_by_key(|(id, e)| (e.priority.signum(), e.order, *id));
        for (id, e) in list {
            if !e.enabled || e.cameras.is_empty() {
                continue;
            }
            let Ok(o) = self.shader_obj(e.shader) else { continue };
            let (Some(p), Some(_)) = (o.processed.clone(), o.resource.as_ref()) else { continue };
            let mut props = Vec::new();
            let mut textures = Vec::new();
            for par in &p.params {
                if crate::shadersrc::POST_TEXTURES.contains(&par.name.as_str()) || crate::shadersrc::POST_PARAMS.iter().any(|(n, _)| *n == par.name) {
                    continue;
                }
                match o.values.get(&par.name) {
                    Some(Val::Floats(f)) if !par.ty.is_texture() => props.push((par.name.clone(), property(par.ty, f))),
                    Some(Val::Texture(t)) if par.ty.is_texture() && *t != 0 => {
                        if let Ok(t) = self.texture(*t) {
                            textures.push((par.name.clone(), t.resource.clone()));
                        }
                    }
                    Some(Val::Resource(r)) => textures.push((par.name.clone(), r.clone())),
                    _ if !par.ty.is_texture() && par.name.starts_with("nb_") => {}
                    _ => {}
                }
            }
            if let Some((_, v)) = [("nb_deltaTime", self.world.update_dt)].iter().find(|(n, _)| p.param(n).is_some()) {
                props.push(("nb_deltaTime".into(), MaterialProperty::Float(*v)));
            }
            let mut cams = Vec::new();
            let mut all = false;
            for c in &e.cameras {
                if *c == 0 {
                    all = true;
                } else if let Some(ent) = self.ent_any(*c) {
                    cams.push(ent.node);
                }
            }
            if !all && cams.is_empty() {
                continue;
            }
            let output = if e.output != 0 { self.texture(e.output).ok().map(|t| t.resource.clone()) } else { None };
            entries.push(Entry {
                id,
                key: o.key,
                ron: Arc::from(p.ron.as_str()),
                props,
                textures,
                cameras: if all { Vec::new() } else { cams },
                hdr: e.priority < 0,
                input: e.input,
                output,
                scale: e.scale.clamp(0.05, 1.0),
            });
        }
        let mut f = self.world.postfx.frame.borrow_mut();
        f.entries = entries;
        f.time = self.world.shader_time;
        Ok(())
    }

    fn set_effect_param(&mut self, e: i32, name: &str, value: &str) -> Result<(), String> {
        let s = self.effect(e)?.shader;
        self.ensure_compiled(s)?;
        let is_tex = self.shader_obj(s)?.param(name).is_some_and(|p| p.ty.is_texture());
        let v = if is_tex { Val::Texture(value.trim().parse().map_err(|_| format!("'{value}' is not a texture handle"))?) } else { Val::Floats(parse_numbers(value)?) };
        self.shader_set(s, name, v)
    }
}

fn int(v: i32) -> Result<Value, String> {
    Ok(Value::Int(v))
}

/// Effects that a camera's settings commands turn on: finds the camera's
/// effect of that built-in, or makes one.
pub fn camera_builtin(h: &mut Host3D, cam: i32, name: &str) -> Result<i32, String> {
    h.camera_ok(cam)?;
    let found = h.world.postfx.effects.iter().enumerate().find_map(|(i, e)| e.as_ref().filter(|e| e.name == name && e.cameras == vec![cam]).map(|_| i as i32 + 1));
    if let Some(id) = found {
        return Ok(id);
    }
    let id = h.load_post(&format!("builtin:{name}"))?;
    h.effect_mut(id)?.cameras = vec![cam];
    Ok(id)
}

pub fn effect_set(h: &mut Host3D, e: i32, name: &str, v: Vec<f32>) -> Result<(), String> {
    let s = h.effect(e)?.shader;
    h.shader_set(s, name, Val::Floats(v))
}

/// The post-processing commands; see the module comment.
pub static COMMANDS: &[(&str, Cmd)] = &[
    ("%CreatePostProcess$source$language=\"glsl\"", |h, a, _| {
        let lang = Lang::parse(&a[1].str()).ok_or_else(|| format!("CreatePostProcess: language must be glsl or wgsl, not '{}'", a[1].str()))?;
        int(h.create_post(&a[0].str(), lang)?)
    }),
    ("%LoadPostProcess$file", |h, a, _| match h.load_post(&a[0].str()) {
        Ok(e) => int(e),
        Err(e) => {
            eprintln!("LoadPostProcess: {e}");
            int(0)
        }
    }),
    ("AddPostProcess%effect%camera=0", |h, a, _| {
        let (e, cam) = (a[0].int(), a[1].int());
        h.camera_ok(cam)?;
        let fx = h.effect_mut(e)?;
        if !fx.cameras.contains(&cam) {
            fx.cameras.push(cam);
        }
        VOID
    }),
    ("RemovePostProcess%effect%camera=0", |h, a, _| {
        let (e, cam) = (a[0].int(), a[1].int());
        let fx = h.effect_mut(e)?;
        if cam == 0 {
            fx.cameras.clear();
        } else {
            fx.cameras.retain(|c| *c != cam);
        }
        VOID
    }),
    ("FreePostProcess%effect", |h, a, _| {
        let e = a[0].int();
        let s = h.effect(e)?.shader;
        h.world.postfx.effects[(e - 1) as usize] = None;
        h.free_shader(s)?;
        VOID
    }),
    ("SetPostProcess%camera%effect", |h, a, _| {
        let (cam, e) = (a[0].int(), a[1].int());
        h.camera_ok(cam)?;
        if e != 0 {
            h.effect(e)?;
        }
        for fx in h.world.postfx.effects.iter_mut().flatten() {
            fx.cameras.retain(|c| *c != cam);
        }
        if e != 0 {
            h.effect_mut(e)?.cameras.push(cam);
        }
        VOID
    }),
    ("SetPostProcessParameter%effect$name$value", |h, a, _| {
        h.set_effect_param(a[0].int(), &a[1].str(), &a[2].str())?;
        VOID
    }),
    ("SetPostProcessEnabled%effect%enable", |h, a, _| {
        h.effect_mut(a[0].int())?.enabled = a[1].int() != 0;
        VOID
    }),
    ("SetPostProcessOrder%effect%order", |h, a, _| {
        h.effect_mut(a[0].int())?.order = a[1].int();
        VOID
    }),
    ("PostProcessInput%effect%source", |h, a, _| {
        let src = a[1].int();
        let input = match src {
            0 => Input::Previous,
            -1 => Input::Original,
            id => {
                h.effect(id)?;
                Input::Effect(id)
            }
        };
        h.effect_mut(a[0].int())?.input = input;
        VOID
    }),
    ("PostProcessOutput%effect%texture", |h, a, _| {
        let t = a[1].int();
        if t != 0 {
            h.texture(t)?;
        }
        h.effect_mut(a[0].int())?.output = t;
        VOID
    }),
    ("PostProcessResolution%effect#scale", |h, a, _| {
        let s = a[1].float();
        if !(0.05..=1.0).contains(&s) {
            return Err(format!("PostProcessResolution: scale must be 0.05 to 1, not {s}"));
        }
        h.effect_mut(a[0].int())?.scale = s;
        VOID
    }),
    ("PostProcessCamera%effect%camera", |h, a, _| {
        let cam = a[1].int();
        h.camera_ok(cam)?;
        h.effect_mut(a[0].int())?.cameras = vec![cam];
        VOID
    }),
    ("PostProcessPriority%effect%priority", |h, a, _| {
        h.effect_mut(a[0].int())?.priority = a[1].int();
        VOID
    }),
    ("%PostProcessShader%effect", |h, a, _| int(h.effect(a[0].int())?.shader)),
    // ---- weather and water screen effects on a camera
    ("SetUnderwaterEffect%camera%enable#water_level=0#red=26#green=89#blue=115", |h, a, _| {
        let id = camera_builtin(h, a[0].int(), "underwater")?;
        h.effect_mut(id)?.enabled = a[1].int() != 0;
        effect_set(h, id, "waterLevel", vec![a[2].float()])?;
        effect_set(h, id, "color", vec![a[3].float() / 255.0, a[4].float() / 255.0, a[5].float() / 255.0])?;
        VOID
    }),
    ("SetRainEffect%camera#amount#speed=1", |h, a, _| {
        let id = camera_builtin(h, a[0].int(), "rain")?;
        h.effect_mut(id)?.enabled = a[1].float() > 0.0;
        effect_set(h, id, "amount", vec![a[1].float()])?;
        effect_set(h, id, "speed", vec![a[2].float()])?;
        VOID
    }),
    ("SetSnowEffect%camera#amount#speed=0.3", |h, a, _| {
        let id = camera_builtin(h, a[0].int(), "snow")?;
        h.effect_mut(id)?.enabled = a[1].float() > 0.0;
        effect_set(h, id, "amount", vec![a[1].float()])?;
        effect_set(h, id, "speed", vec![a[2].float()])?;
        VOID
    }),
    ("SetAtmosphericFog%camera#density#red=166#green=184#blue=204#height_falloff=0.08", |h, a, _| {
        let id = camera_builtin(h, a[0].int(), "fog")?;
        h.effect_mut(id)?.enabled = a[1].float() > 0.0;
        effect_set(h, id, "density", vec![a[1].float()])?;
        effect_set(h, id, "color", vec![a[2].float() / 255.0, a[3].float() / 255.0, a[4].float() / 255.0])?;
        effect_set(h, id, "heightFalloff", vec![a[5].float()])?;
        VOID
    }),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shadersrc::{process, DiskIncludes, Input as SrcInput};
    use std::collections::BTreeSet;

    #[test]
    fn every_builtin_effect_processes() {
        for (name, _, _) in BUILTIN {
            let (src, _) = builtin(name).unwrap();
            let p = process(&SrcInput { source: &src, file_name: name, path: None, keywords: &BTreeSet::new(), includes: &DiskIncludes }).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(p.options.pass, crate::shadersrc::PassKind::Post);
            fyrox::material::shader::Shader::from_string(&p.ron).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
        assert!(builtin("DepthOfField").is_some());
    }
}
