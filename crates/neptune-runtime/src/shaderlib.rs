//! The built-in shader library (ext): the standard vertex program, the
//! code generated around `nb_surface`, the `#include <neptune/...>` files
//! and the ready-made shaders loaded with `LoadShader("builtin:<name>")`.
//!
//! All of it is GLSL 3.30 for Fyrox's OpenGL renderer; Fyrox adds its
//! `S_*` helpers (`S_SRGBToLinear`, `S_PBR_CalculateLight`, ...) in front.
//! World-space values inside shaders are in Fyrox space, where X is
//! mirrored compared with Blitz space (see `transform.rs`).

use crate::shadersrc::{Lighting, PassKind};

/// Vertex attributes of NeptuneBASIC meshes (name, location).
pub const ATTRIBUTES: &[(&str, u32)] = &[
    ("vertexPosition", 0),
    ("vertexTexCoord", 1),
    ("vertexNormal", 2),
    ("vertexTangent", 3),
    ("boneWeights", 4),
    ("boneIndices", 5),
    ("vertexSecondTexCoord", 6),
    ("vertexColor", 7),
    ("vertexData0", 8),
    ("vertexData1", 9),
    ("vertexData2", 10),
    ("vertexData3", 11),
];

const ATTRIBUTE_DECLS: &str = "layout(location = 0) in vec3 vertexPosition;
layout(location = 1) in vec2 vertexTexCoord;
layout(location = 2) in vec3 vertexNormal;
layout(location = 3) in vec4 vertexTangent;
layout(location = 4) in vec4 boneWeights;
layout(location = 5) in vec4 boneIndices;
layout(location = 6) in vec2 vertexSecondTexCoord;
layout(location = 7) in vec4 vertexColor;
layout(location = 8) in vec4 vertexData0;
layout(location = 9) in vec4 vertexData1;
layout(location = 10) in vec4 vertexData2;
layout(location = 11) in vec4 vertexData3;
";

const SKINNING: &str = "// Skeletal animation: blends the bone matrices of a skinned mesh.
mat4 nb_skinMatrix() {
    if (!fyrox_instanceData.useSkeletalAnimation) return mat4(1.0);
    return fyrox_boneMatrices.matrices[int(boneIndices.x)] * boneWeights.x
         + fyrox_boneMatrices.matrices[int(boneIndices.y)] * boneWeights.y
         + fyrox_boneMatrices.matrices[int(boneIndices.z)] * boneWeights.z
         + fyrox_boneMatrices.matrices[int(boneIndices.w)] * boneWeights.w;
}
";

/// Helpers shared by both stages.
const COMMON_HELPERS: &str = "#ifndef NB_PI_DEFINED
#define NB_PI_DEFINED 1
const float NB_PI = 3.14159265359;
vec3 nb_linear(vec3 c) { return S_SRGBToLinear(vec4(c, 1.0)).rgb; }
vec3 nb_srgb(vec3 c) { return S_LinearToSRGB(vec4(c, 1.0)).rgb; }
#endif
";

/// Put in front of every vertex stage.
pub fn vertex_prelude() -> String {
    format!("#define NB_VERTEX_STAGE 1\n{COMMON_HELPERS}")
}

/// Declarations of the standard vertex program: attributes, skinning and
/// the `nb_*` varyings. Hook functions (`nb_displace`) go after these.
pub fn default_vertex_decls(point_shadow: bool) -> String {
    format!(
        "{ATTRIBUTE_DECLS}{SKINNING}
out vec3 nb_worldPos;
out vec3 nb_normal;
out vec3 nb_tangent;
out vec3 nb_binormal;
out vec2 nb_uv;
out vec2 nb_uv2;
out vec4 nb_color;
out vec4 nb_data0;
out vec4 nb_data1;
out vec4 nb_data2;
out vec4 nb_data3;
{}
",
        if point_shadow { "out vec3 nb_shadowWorld;" } else { "" }
    )
}

/// The standard vertex `main`: skinning, the optional `nb_displace` /
/// `nb_displaceWorld` hooks, and the `nb_*` varyings.
pub fn default_vertex_main(point_shadow: bool) -> String {
    format!(
        "void main() {{
    mat4 skin = nb_skinMatrix();
    vec4 p = skin * vec4(vertexPosition, 1.0);
    vec3 n = mat3(skin) * vertexNormal;
    vec3 t = mat3(skin) * vertexTangent.xyz;
    vec2 uv = vertexTexCoord;
#ifdef NB_VERTEX_HOOK
    vec3 lp = p.xyz;
    nb_displace(lp, n, uv);
    p.xyz = lp;
#endif
    mat3 nm = mat3(fyrox_instanceData.worldMatrix);
    vec3 wn = normalize(nm * n);
    vec3 wt = normalize(nm * t);
    vec3 w = (fyrox_instanceData.worldMatrix * p).xyz;
#ifdef NB_WORLD_HOOK
    nb_displaceWorld(w, wn, uv);
    gl_Position = fyrox_cameraData.viewProjectionMatrix * vec4(w, 1.0);
#else
    gl_Position = fyrox_instanceData.worldViewProjection * p;
#endif
    nb_worldPos = w;
    nb_normal = wn;
    nb_tangent = wt;
    nb_binormal = normalize(vertexTangent.w * cross(wn, wt));
    nb_uv = uv;
    nb_uv2 = vertexSecondTexCoord;
    nb_color = vertexColor;
    nb_data0 = vertexData0;
    nb_data1 = vertexData1;
    nb_data2 = vertexData2;
    nb_data3 = vertexData3;
    {}
}}
",
        if point_shadow { "nb_shadowWorld = w;" } else { "" }
    )
}

/// Wraps a program's own vertex `main` for point-light shadow passes,
/// which need the world position of each vertex.
pub const SHADOW_WRAPPER: &str = "#undef main
out vec3 nb_shadowWorld;
void main() {
    nb_userMain();
    vec4 w = inverse(fyrox_cameraData.viewProjectionMatrix) * gl_Position;
    nb_shadowWorld = w.xyz / w.w;
}
";

/// Full-screen quad for post-processing passes.
pub const POST_VERTEX: &str = "layout(location = 0) in vec3 vertexPosition;
layout(location = 1) in vec2 vertexTexCoord;
out vec2 nb_uv;
void main() {
    nb_uv = vertexTexCoord;
    gl_Position = nb_wvp * vec4(vertexPosition, 1.0);
}
";

/// The surface description written by `nb_surface`, and helpers.
pub fn surface_lib() -> String {
    format!(
        "{COMMON_HELPERS}struct NbSurface {{
    vec3 albedo;        // sRGB colour 0-1
    float alpha;
    vec3 normal;        // world space
    float metallic;
    float roughness;
    float ao;
    vec3 emission;      // linear, may exceed 1 (glows with bloom)
    float alphaCutoff;
    vec3 specularColor; // phong, blinn and toon highlights
    float shininess;
    float toonSteps;
    vec3 shadeColor;    // toon: colour of the unlit side (multiplied)
}};
vec4 nb_textureArray(sampler3D s, vec2 uv, float layer) {{
    float d = float(textureSize(s, 0).z);
    return texture(s, vec3(uv, (floor(layer) + 0.5) / d));
}}
"
    )
}

/// Fragment declarations: the `nb_*` varyings (with the standard vertex
/// program) and the pass outputs.
pub fn fragment_prelude_ex(pass: Option<PassKind>, varyings: bool, outputs: bool) -> String {
    let mut s = String::new();
    if pass == Some(PassKind::Post) {
        s.push_str("in vec2 nb_uv;\n");
        if outputs {
            s.push_str("layout(location = 0) out vec4 FragColor;\n");
        }
        s.push_str(POST_LIB);
        return s;
    }
    if varyings {
        s.push_str(
            "#define NB_VARYINGS 1
in vec3 nb_worldPos;
in vec3 nb_normal;
in vec3 nb_tangent;
in vec3 nb_binormal;
in vec2 nb_uv;
in vec2 nb_uv2;
in vec4 nb_color;
in vec4 nb_data0;
in vec4 nb_data1;
in vec4 nb_data2;
in vec4 nb_data3;
",
        );
    }
    if outputs {
        match pass {
            Some(PassKind::GBuffer) => s.push_str(
                "layout(location = 0) out vec4 outColor;
layout(location = 1) out vec4 outNormal;
layout(location = 2) out vec4 outAmbient;
layout(location = 3) out vec4 outMaterial;
layout(location = 4) out uint outDecalMask;
",
            ),
            Some(_) => s.push_str("layout(location = 0) out vec4 FragColor;\n"),
            None => {}
        }
    }
    s.push_str(
        "NbSurface nb_defaultSurface() {
    NbSurface s;
    s.albedo = vec3(1.0);
    s.alpha = 1.0;
#ifdef NB_VARYINGS
    s.normal = normalize(gl_FrontFacing ? nb_normal : -nb_normal);
#else
    s.normal = vec3(0.0, 1.0, 0.0);
#endif
    s.metallic = 0.0;
    s.roughness = 0.6;
    s.ao = 1.0;
    s.emission = vec3(0.0);
    s.alphaCutoff = 0.5;
    s.specularColor = vec3(0.5);
    s.shininess = 32.0;
    s.toonSteps = 3.0;
    s.shadeColor = vec3(0.45, 0.45, 0.6);
    return s;
}
#ifdef NB_VARYINGS
// world-space normal from a tangent-space normal map sample (0-1)
vec3 nb_normalFromMap(vec3 mapValue, float strength) {
    vec3 n = mapValue * 2.0 - 1.0;
    n.xy *= strength;
    mat3 tbn = mat3(normalize(nb_tangent), normalize(nb_binormal), normalize(nb_normal));
    return normalize(tbn * normalize(n));
}
vec3 nb_viewDir() { return normalize(fyrox_cameraData.position - nb_worldPos); }
float nb_fresnel(vec3 n, float power) { return pow(1.0 - clamp(dot(normalize(n), nb_viewDir()), 0.0, 1.0), power); }
// screen position 0-1 of this fragment
vec2 nb_screenUV() { return gl_FragCoord.xy / max(nb_resolution, vec2(1.0)); }
#endif
",
    );
    s
}

/// Helpers for post-processing passes.
const POST_LIB: &str = "vec2 nb_texel() { return 1.0 / max(nb_resolution, vec2(1.0)); }
float nb_luma(vec3 c) { return dot(c, vec3(0.2126, 0.7152, 0.0722)); }
// view-space distance of the scene at uv (0 = near plane)
float nb_linearDepth(vec2 uv) {
    float d = texture(nb_depthTexture, uv).r;
    float n = nb_clip.x;
    float f = nb_clip.y;
    float z = d * 2.0 - 1.0;
    return (2.0 * n * f) / (f + n - z * (f - n));
}
vec3 nb_worldPosAt(vec2 uv) {
    float d = texture(nb_depthTexture, uv).r;
    vec4 p = nb_invViewProj * vec4(vec3(uv, d) * 2.0 - 1.0, 1.0);
    return p.xyz / p.w;
}
vec3 nb_normalAt(vec2 uv) { return normalize(texture(nb_normalTexture, uv).xyz * 2.0 - 1.0); }
";

/// The `main` generated for `nb_surface` and `nb_post` shaders.
pub fn fragment_main(pass: PassKind, lighting: Lighting) -> String {
    match pass {
        PassKind::Post => "void main() { FragColor = nb_post(nb_uv); }\n".into(),
        PassKind::GBuffer => "void main() {
    NbSurface s = nb_defaultSurface();
    nb_surface(s);
#ifdef NB_ALPHA_CLIP
    if (s.alpha < s.alphaCutoff) discard;
#endif
    outColor = vec4(s.albedo, 1.0);
    outNormal = vec4(normalize(s.normal) * 0.5 + 0.5, 1.0);
    outMaterial = vec4(s.metallic, s.roughness, s.ao, 1.0);
    outAmbient = vec4(s.emission, 1.0);
    outDecalMask = 0u;
}
"
        .into(),
        PassKind::Forward => {
            let (lib, shade) = if lighting == Lighting::Unlit { (FOG.to_string(), "nb_linear(s.albedo) + s.emission") } else { (lighting_lib(), "nb_shade(s)") };
            format!(
                "{lib}
void main() {{
    NbSurface s = nb_defaultSurface();
    nb_surface(s);
#ifdef NB_ALPHA_CLIP
    if (s.alpha < s.alphaCutoff) discard;
#endif
    vec3 c = {shade};
    FragColor = vec4(nb_applyFog(c), s.alpha);
}}
"
            )
        }
    }
}

/// Fragment of a shadow pass (depth only; point lights write distance).
pub fn shadow_fragment(point: bool, clip: bool) -> &'static str {
    match (point, clip) {
        (false, false) => "void main() {}\n",
        (true, false) => "in vec3 nb_shadowWorld;
layout(location = 0) out float depth;
void main() { depth = length(fyrox_lightData.lightPosition - nb_shadowWorld); }
",
        (false, true) => "void main() {
    NbSurface s = nb_defaultSurface();
    nb_surface(s);
    if (s.alpha < s.alphaCutoff) discard;
}
",
        (true, true) => "in vec3 nb_shadowWorld;
layout(location = 0) out float depth;
void main() {
    NbSurface s = nb_defaultSurface();
    nb_surface(s);
    if (s.alpha < s.alphaCutoff) discard;
    depth = length(fyrox_lightData.lightPosition - nb_shadowWorld);
}
",
    }
}

const FOG: &str = "#if !defined(NB_FOG_LIB) && defined(NB_VARYINGS)
#define NB_FOG_LIB 1
vec3 nb_applyFog(vec3 c) {
    if (nb_fogRange.y > nb_fogRange.x) {
        float d = distance(nb_worldPos, fyrox_cameraData.position);
        c = mix(c, nb_linear(nb_fogColor), clamp((d - nb_fogRange.x) / (nb_fogRange.y - nb_fogRange.x), 0.0, 1.0));
    }
    return c;
}
#endif
";

/// `<neptune/lighting.glsl>`: Fyrox's visible lights (up to 16) with
/// several lighting models, used by forward-pass shaders.
pub fn lighting_lib() -> String {
    format!("#ifndef NB_LIGHTING_LIB\n#define NB_LIGHTING_LIB 1\n{LIGHTS}#endif\n{FOG}{LIGHTING_SHADE}")
}

const LIGHTS: &str = "struct NbLight { vec3 color; vec3 L; float atten; };
int nb_lightCount() { return min(fyrox_lightsBlock.lightCount, 16); }
// light i as seen from world position P (L points towards the light)
NbLight nb_light(int i, vec3 P) {
    NbLight l;
    vec4 cr = fyrox_lightsBlock.lightsColorRadius[i];
    vec3 dir = normalize(fyrox_lightsBlock.lightsDirection[i]);
    vec2 cone = fyrox_lightsBlock.lightsParameters[i];
    l.color = cr.rgb;
    if (isinf(cr.w) || cr.w > 1e30) {
        l.L = dir;
        l.atten = 1.0;
    } else {
        vec3 toLight = fyrox_lightsBlock.lightsPosition[i] - P;
        float d = length(toLight);
        l.L = toLight / max(d, 1e-4);
        l.atten = S_LightDistanceAttenuation(d, cr.w);
        if (cone.y > -0.999) l.atten *= smoothstep(cone.y, cone.x, dot(dir, l.L));
    }
    return l;
}
float nb_lambertTerm(vec3 N, vec3 L) { return max(dot(N, L), 0.0); }
float nb_phongTerm(vec3 N, vec3 L, vec3 V, float shininess) { return pow(max(dot(reflect(-L, N), V), 0.0), shininess); }
float nb_blinnTerm(vec3 N, vec3 L, vec3 V, float shininess) { return pow(max(dot(N, normalize(L + V)), 0.0), shininess); }
float nb_toonTerm(float ndl, float steps) { return steps <= 1.0 ? step(0.5, ndl) : floor(ndl * steps + 0.5) / steps; }
vec3 nb_pbrTerm(vec3 N, vec3 V, vec3 albedo, float metallic, float roughness, NbLight l) {
    TPBRContext ctx;
    ctx.albedo = albedo;
    ctx.fragmentToLight = l.L;
    ctx.fragmentNormal = N;
    ctx.lightColor = l.color;
    ctx.metallic = metallic;
    ctx.roughness = roughness;
    ctx.viewVector = V;
    return S_PBR_CalculateLight(ctx);
}
";

const LIGHTING_SHADE: &str = "#if !defined(NB_SHADE_LIB) && defined(NB_VARYINGS)
#define NB_SHADE_LIB 1
// lights a surface with the shader's lighting model (linear result)
vec3 nb_shade(NbSurface s) {
    vec3 albedo = nb_linear(s.albedo);
    vec3 N = normalize(s.normal);
    vec3 P = nb_worldPos;
    vec3 V = normalize(fyrox_cameraData.position - P);
    vec3 c = nb_linear(nb_ambient) * albedo * s.ao;
    for (int i = 0; i < nb_lightCount(); ++i) {
        NbLight l = nb_light(i, P);
        if (l.atten <= 0.0) continue;
        float ndl = nb_lambertTerm(N, l.L);
#if defined(NB_LIGHTING_PBR)
        c += nb_pbrTerm(N, V, albedo, s.metallic, s.roughness, l) * l.atten * NB_PI;
#elif defined(NB_LIGHTING_LAMBERT)
        c += albedo * l.color * ndl * l.atten;
#elif defined(NB_LIGHTING_PHONG)
        c += (albedo * ndl + nb_linear(s.specularColor) * nb_phongTerm(N, l.L, V, s.shininess) * step(0.0, dot(N, l.L))) * l.color * l.atten;
#elif defined(NB_LIGHTING_BLINN)
        c += (albedo * ndl + nb_linear(s.specularColor) * nb_blinnTerm(N, l.L, V, s.shininess) * step(0.0, dot(N, l.L))) * l.color * l.atten;
#elif defined(NB_LIGHTING_TOON)
        float t = nb_toonTerm(dot(N, l.L) * 0.5 + 0.5, s.toonSteps);
        vec3 lit = mix(albedo * nb_linear(s.shadeColor), albedo, t);
        float spec = step(1.0 - 0.02 * (128.0 / max(s.shininess, 1.0)), nb_blinnTerm(N, l.L, V, s.shininess) * t);
        c += (lit + nb_linear(s.specularColor) * spec) * l.color * l.atten;
#endif
    }
    return c + s.emission;
}
#endif
";

/// The library files for `#include <neptune/...>`.
pub fn include(name: &str) -> Option<String> {
    let n = name.strip_prefix("neptune/")?.trim_end_matches(".glsl");
    Some(match n {
        "lighting" => lighting_lib(),
        "noise" => NOISE.into(),
        "pattern" | "patterns" => PATTERN.into(),
        "color" | "colour" => COLOR.into(),
        "attributes" => ATTRIBUTE_DECLS.into(),
        "skinning" => SKINNING.into(),
        "procedural" => format!("{NOISE}{PATTERN}{COLOR}"),
        _ => return None,
    })
}

pub const NOISE: &str = "#ifndef NB_NOISE_LIB
#define NB_NOISE_LIB 1
// hashes (no textures, same result on every GPU)
float nb_hash12(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * 0.1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
vec2 nb_hash22(vec2 p) { vec3 p3 = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973)); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.xx + p3.yz) * p3.zy); }
vec3 nb_hash33(vec3 p) { p = fract(p * vec3(0.1031, 0.1030, 0.0973)); p += dot(p, p.yxz + 33.33); return fract((p.xxy + p.yxx) * p.zyx); }
// value noise 0-1
float nb_valueNoise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(nb_hash12(i), nb_hash12(i + vec2(1, 0)), u.x), mix(nb_hash12(i + vec2(0, 1)), nb_hash12(i + vec2(1, 1)), u.x), u.y);
}
// Perlin (gradient) noise 0-1
float nb_perlin(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    vec2 g00 = nb_hash22(i) * 2.0 - 1.0, g10 = nb_hash22(i + vec2(1, 0)) * 2.0 - 1.0;
    vec2 g01 = nb_hash22(i + vec2(0, 1)) * 2.0 - 1.0, g11 = nb_hash22(i + vec2(1, 1)) * 2.0 - 1.0;
    float a = dot(g00, f), b = dot(g10, f - vec2(1, 0)), c = dot(g01, f - vec2(0, 1)), d = dot(g11, f - vec2(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * 0.7071 + 0.5;
}
float nb_perlin3(vec3 p) {
    vec3 i = floor(p), f = fract(p);
    vec3 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    float n000 = dot(nb_hash33(i) * 2.0 - 1.0, f);
    float n100 = dot(nb_hash33(i + vec3(1, 0, 0)) * 2.0 - 1.0, f - vec3(1, 0, 0));
    float n010 = dot(nb_hash33(i + vec3(0, 1, 0)) * 2.0 - 1.0, f - vec3(0, 1, 0));
    float n110 = dot(nb_hash33(i + vec3(1, 1, 0)) * 2.0 - 1.0, f - vec3(1, 1, 0));
    float n001 = dot(nb_hash33(i + vec3(0, 0, 1)) * 2.0 - 1.0, f - vec3(0, 0, 1));
    float n101 = dot(nb_hash33(i + vec3(1, 0, 1)) * 2.0 - 1.0, f - vec3(1, 0, 1));
    float n011 = dot(nb_hash33(i + vec3(0, 1, 1)) * 2.0 - 1.0, f - vec3(0, 1, 1));
    float n111 = dot(nb_hash33(i + vec3(1, 1, 1)) * 2.0 - 1.0, f - vec3(1, 1, 1));
    float x00 = mix(n000, n100, u.x), x10 = mix(n010, n110, u.x), x01 = mix(n001, n101, u.x), x11 = mix(n011, n111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z) * 0.6 + 0.5;
}
// simplex noise 0-1 (2D, skewed triangle grid)
float nb_simplex(vec2 p) {
    const float K1 = 0.366025404, K2 = 0.211324865;
    vec2 i = floor(p + (p.x + p.y) * K1);
    vec2 a = p - i + (i.x + i.y) * K2;
    float m = step(a.y, a.x);
    vec2 o = vec2(m, 1.0 - m);
    vec2 b = a - o + K2;
    vec2 c = a - 1.0 + 2.0 * K2;
    vec3 h = max(0.5 - vec3(dot(a, a), dot(b, b), dot(c, c)), 0.0);
    vec3 n = h * h * h * h * vec3(dot(a, nb_hash22(i) * 2.0 - 1.0), dot(b, nb_hash22(i + o) * 2.0 - 1.0), dot(c, nb_hash22(i + 1.0) * 2.0 - 1.0));
    return dot(n, vec3(70.0)) * 0.5 + 0.5;
}
// Voronoi: x = distance to the nearest cell centre, y = to the second, z = cell id 0-1
vec3 nb_voronoi(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    float d1 = 8.0, d2 = 8.0, id = 0.0;
    for (int y = -1; y <= 1; ++y)
        for (int x = -1; x <= 1; ++x) {
            vec2 g = vec2(x, y);
            vec2 o = nb_hash22(i + g);
            float d = length(g + o - f);
            if (d < d1) { d2 = d1; d1 = d; id = nb_hash12(i + g); } else if (d < d2) { d2 = d; }
        }
    return vec3(d1, d2, id);
}
// noise by type: 0 value, 1 Perlin, 2 simplex, 3 Voronoi (cells), 4 cellular (edges)
float nb_noise(int type, vec2 p) {
    if (type == 1) return nb_perlin(p);
    if (type == 2) return nb_simplex(p);
    if (type == 3) return clamp(nb_voronoi(p).x, 0.0, 1.0);
    if (type == 4) { vec3 v = nb_voronoi(p); return clamp(v.y - v.x, 0.0, 1.0); }
    return nb_valueNoise(p);
}
// fractal Brownian motion
float nb_fbmEx(int type, vec2 p, int octaves, float persistence, float lacunarity) {
    float sum = 0.0, amp = 0.5, norm = 0.0;
    for (int i = 0; i < 12; ++i) {
        if (i >= octaves) break;
        sum += nb_noise(type, p) * amp;
        norm += amp;
        p = p * lacunarity + vec2(17.3, 9.1);
        amp *= persistence;
    }
    return norm > 0.0 ? sum / norm : 0.0;
}
float nb_fbm(vec2 p, int octaves) { return nb_fbmEx(1, p, octaves, 0.5, 2.0); }
// normal from a height function sampled around uv (procedural normal maps)
#define NB_HEIGHT_NORMAL(fn, uv, eps, strength) normalize(vec3((fn((uv) - vec2(eps, 0.0)) - fn((uv) + vec2(eps, 0.0))) * (strength), (fn((uv) - vec2(0.0, eps)) - fn((uv) + vec2(0.0, eps))) * (strength), 2.0 * (eps)) )
#endif
";

pub const PATTERN: &str = "#ifndef NB_PATTERN_LIB
#define NB_PATTERN_LIB 1
vec2 nb_rotate(vec2 p, float degrees) { float a = radians(degrees); float c = cos(a), s = sin(a); return mat2(c, -s, s, c) * p; }
float nb_checker(vec2 p) { vec2 c = floor(p); return mod(c.x + c.y, 2.0); }
#ifdef NB_VERTEX_STAGE
float nb_grid(vec2 p, float width) { return 0.0; }
#else
float nb_grid(vec2 p, float width) { vec2 g = abs(fract(p - 0.5) - 0.5) / max(fwidth(p), vec2(1e-5)); return 1.0 - clamp(min(g.x, g.y) - width, 0.0, 1.0); }
#endif
float nb_stripes(float x, float width) { return step(fract(x), clamp(width, 0.0, 1.0)); }
float nb_dots(vec2 p, float radius) { return 1.0 - smoothstep(radius - 0.02, radius + 0.02, length(fract(p) - 0.5)); }
// gradients 0-1: linear along a direction in degrees, radial from the centre, angular around it
float nb_linearGradient(vec2 uv, float degrees) { vec2 d = vec2(cos(radians(degrees)), sin(radians(degrees))); return clamp(dot(uv - 0.5, d) + 0.5, 0.0, 1.0); }
float nb_radialGradient(vec2 uv) { return clamp(length(uv - 0.5) * 2.0, 0.0, 1.0); }
float nb_angularGradient(vec2 uv, float degrees) { vec2 p = nb_rotate(uv - 0.5, degrees); return atan(p.y, p.x) / (2.0 * 3.14159265) + 0.5; }
#endif
";

pub const COLOR: &str = "#ifndef NB_COLOR_LIB
#define NB_COLOR_LIB 1
vec3 nb_hsv2rgb(vec3 c) { vec3 p = abs(fract(c.xxx + vec3(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0); return c.z * mix(vec3(1.0), clamp(p - 1.0, 0.0, 1.0), c.y); }
vec3 nb_rgb2hsv(vec3 c) {
    vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
    vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
    float d = q.x - min(q.w, q.y);
    return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + 1e-10)), d / (q.x + 1e-10), q.x);
}
vec3 nb_posterize(vec3 c, float levels) { return floor(c * levels + 0.5) / max(levels, 1.0); }
float nb_luminance(vec3 c) { return dot(c, vec3(0.2126, 0.7152, 0.0722)); }
vec3 nb_saturate(vec3 c, float amount) { return mix(vec3(nb_luminance(c)), c, amount); }
#endif
";

/// Names of the ready-made shaders (`LoadShader("builtin:<name>")`).
pub const PRESETS: &[&str] = &[
    "unlit", "textured", "vertexcolor", "diffuse", "phong", "blinnphong", "pbr", "normalmap", "metal", "glass", "water", "toon",
    "cel", "outline", "rim", "hologram", "dissolve", "fire", "smoke", "forcefield", "gradient", "noise", "procedural", "matcap",
    "flat", "ps1", "pixelart", "vertexlit", "wind", "displace", "posterize", "handpainted",
];

/// Source of a ready-made shader.
pub fn preset(name: &str) -> Option<String> {
    let name = name.to_ascii_lowercase();
    Some(match name.as_str() {
        "unlit" => "//! lighting unlit
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
uniform vec2 uvScale = vec2(1.0, 1.0);
//! fragment
void nb_surface(inout NbSurface s) {
    vec4 t = texture(albedoMap, nb_uv * uvScale) * albedo;
    s.albedo = t.rgb;
    s.alpha = t.a;
}
"
        .into(),
        "textured" | "diffuse" | "phong" | "blinnphong" | "vertexcolor" => {
            let lighting = match name.as_str() {
                "textured" => "pbr",
                "diffuse" | "vertexcolor" => "lambert",
                "phong" => "phong",
                _ => "blinn",
            };
            format!(
                "//! lighting {lighting}
//! shadows on
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
uniform vec2 uvScale = vec2(1.0, 1.0);
uniform vec3 specularColor = vec3(0.6, 0.6, 0.6);
uniform float shininess = 32.0;
uniform float roughness = 0.6;
//! fragment
void nb_surface(inout NbSurface s) {{
    vec4 t = texture(albedoMap, nb_uv * uvScale) * albedo;
    {vc}
    s.albedo = t.rgb;
    s.alpha = t.a;
    s.specularColor = specularColor;
    s.shininess = shininess;
    s.roughness = roughness;
}}
",
                vc = if name == "vertexcolor" { "t *= nb_color;" } else { "" }
            )
        }
        "pbr" | "normalmap" | "metal" => {
            let (metal, rough) = if name == "metal" { (1.0, 0.25) } else { (0.0, 0.5) };
            format!(
                "// Metallic/roughness (or specular) PBR with Fyrox's deferred lighting and shadows.
// Keyword NB_TRANSPARENT draws it in the forward pass with alpha blending.
//! pass gbuffer if !NB_TRANSPARENT
//! pass forward if NB_TRANSPARENT
//! blend alpha if NB_TRANSPARENT
//! alpha_clip on if NB_CUTOUT
//! shadows on
#include <neptune/lighting.glsl>
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
uniform sampler2D normalMap;
uniform float normalStrength = 1.0;
uniform float metallic = {metal};
uniform float roughness = {rough};
uniform sampler2D metallicMap;
uniform sampler2D roughnessMap;
uniform sampler2D aoMap;
uniform vec3 emissive = vec3(0.0, 0.0, 0.0);
uniform float emissiveStrength = 1.0;
uniform sampler2D emissiveMap; //! fallback white
uniform float opacity = 1.0;
uniform sampler2D opacityMap;
uniform sampler2D heightMap; //! fallback black
uniform float parallaxScale = 0.0;
uniform samplerCube envMap; //! fallback black
uniform float reflection = 0.0;
uniform float fresnel = 1.0;
uniform vec3 specularColor = vec3(0.04, 0.04, 0.04);
uniform int workflow = 0;
uniform vec2 uvScale = vec2(1.0, 1.0);
uniform float alphaCutoff = 0.5;
//! fragment
vec2 nb_parallax(vec2 uv) {{
    if (parallaxScale <= 0.0) return uv;
    mat3 tbn = mat3(normalize(nb_tangent), normalize(nb_binormal), normalize(nb_normal));
    vec3 v = normalize(transpose(tbn) * nb_viewDir());
    // steep parallax with 16 layers
    float layers = 16.0, depth = 0.0, step1 = 1.0 / layers;
    vec2 dt = v.xy / max(v.z, 0.1) * parallaxScale / layers;
    float h = 1.0 - texture(heightMap, uv).r;
    for (int i = 0; i < 16; ++i) {{
        if (depth >= h) break;
        uv -= dt;
        depth += step1;
        h = 1.0 - texture(heightMap, uv).r;
    }}
    return uv;
}}
void nb_surface(inout NbSurface s) {{
    vec2 uv = nb_parallax(nb_uv * uvScale);
    vec4 base = texture(albedoMap, uv) * albedo;
    s.albedo = base.rgb;
    s.alpha = base.a * opacity * texture(opacityMap, uv).r;
    s.alphaCutoff = alphaCutoff;
    s.normal = nb_normalFromMap(texture(normalMap, uv).rgb, normalStrength);
    if (!gl_FrontFacing) s.normal = -s.normal;
    float m = metallic * texture(metallicMap, uv).r;
    if (workflow == 1) {{
        // specular workflow: brightness of the specular colour above dielectric F0 acts as metalness
        float spec = max(max(specularColor.r, specularColor.g), specularColor.b);
        m = clamp((spec - 0.04) / 0.96, 0.0, 1.0);
        s.albedo = mix(s.albedo, specularColor, m);
    }}
    s.metallic = m;
    s.roughness = clamp(roughness * texture(roughnessMap, uv).r, 0.02, 1.0);
    s.ao = texture(aoMap, uv).r;
    s.emission = nb_linear(emissive) * emissiveStrength * texture(emissiveMap, uv).rgb;
    if (reflection > 0.0) {{
        vec3 r = reflect(-nb_viewDir(), s.normal);
        float f = mix(1.0, nb_fresnel(s.normal, 5.0), clamp(fresnel, 0.0, 1.0));
        float lod = s.roughness * 6.0;
        s.emission += textureLod(envMap, r, lod).rgb * reflection * f * mix(vec3(1.0), nb_linear(s.albedo), s.metallic);
    }}
}}
"
            )
        }
        "glass" => "//! blend alpha
//! cull none
//! lighting blinn
uniform vec4 albedo = vec4(0.85, 0.95, 1.0, 0.25);
uniform samplerCube envMap; //! fallback black
uniform float reflection = 0.8;
uniform float fresnel = 3.0;
uniform float shininess = 96.0;
uniform float refraction = 0.03;
//! fragment
void nb_surface(inout NbSurface s) {
    vec3 n = s.normal;
    float f = nb_fresnel(n, fresnel);
    vec3 r = reflect(-nb_viewDir(), n);
    s.albedo = albedo.rgb;
    s.alpha = clamp(albedo.a + f * 0.6, 0.0, 1.0);
    s.specularColor = vec3(1.0);
    s.shininess = shininess;
    s.emission = texture(envMap, r).rgb * reflection * f;
}
"
        .into(),
        "water" => WATER.into(),
        "toon" | "cel" => format!(
            "//! lighting toon
//! shadows on
#include <neptune/lighting.glsl>
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
uniform sampler2D toonRamp; //! fallback white
uniform int useRamp = 0;
uniform float toonSteps = {steps};
uniform vec3 shadeColor = vec3(0.45, 0.45, 0.6);
uniform vec3 specularColor = vec3({spec});
uniform float shininess = 48.0;
uniform vec3 rimColor = vec3(1.0, 1.0, 1.0);
uniform float rimPower = 3.0;
uniform float rimStrength = {rim};
//! fragment
void nb_surface(inout NbSurface s) {{
    vec4 t = texture(albedoMap, nb_uv) * albedo;
    s.albedo = t.rgb;
    s.alpha = t.a;
    s.toonSteps = toonSteps;
    s.shadeColor = shadeColor;
    s.specularColor = specularColor;
    s.shininess = shininess;
    if (useRamp != 0) {{
        // a ramp texture replaces the steps: light (0-1 across) picks the colour
        float ndl = 0.5;
        for (int i = 0; i < nb_lightCount(); ++i) {{ NbLight l = nb_light(i, nb_worldPos); ndl = max(ndl, dot(s.normal, l.L) * 0.5 + 0.5); }}
        s.shadeColor = texture(toonRamp, vec2(ndl, 0.5)).rgb;
        s.toonSteps = 1000.0;
    }}
    float rim = nb_fresnel(s.normal, rimPower);
    s.emission = nb_linear(rimColor) * step(0.5, rim) * rimStrength;
}}
",
            steps = if name == "cel" { "2.0" } else { "3.0" },
            spec = if name == "cel" { "0.0, 0.0, 0.0" } else { "0.8, 0.8, 0.8" },
            rim = if name == "cel" { "0.0" } else { "0.35" },
        ),
        "outline" => "// Inverted hull: draws the back faces pushed out along the normals.
//! lighting unlit
//! cull front
uniform vec4 outlineColor = vec4(0.0, 0.0, 0.0, 1.0);
uniform float outlineWidth = 0.03;
//! vertex
void nb_displace(inout vec3 p, inout vec3 n, vec2 uv) { p += normalize(n) * outlineWidth; }
//! fragment
void nb_surface(inout NbSurface s) { s.albedo = outlineColor.rgb; s.alpha = outlineColor.a; }
"
        .into(),
        "rim" => "//! lighting lambert
uniform vec4 albedo = vec4(0.2, 0.2, 0.25, 1.0);
uniform vec3 rimColor = vec3(0.3, 0.8, 1.0);
uniform float rimPower = 2.5;
uniform float rimStrength = 2.0;
//! fragment
void nb_surface(inout NbSurface s) {
    s.albedo = albedo.rgb;
    s.emission = nb_linear(rimColor) * nb_fresnel(s.normal, rimPower) * rimStrength;
}
"
        .into(),
        "hologram" => "//! blend add
//! cull none
//! lighting unlit
uniform vec3 color = vec3(0.2, 0.9, 1.0);
uniform float lineDensity = 60.0;
uniform float speed = 1.5;
uniform float flicker = 0.15;
//! fragment
#include <neptune/noise.glsl>
void nb_surface(inout NbSurface s) {
    float lines = 0.5 + 0.5 * sin((nb_worldPos.y * lineDensity) - nb_time * speed * 6.0);
    float rim = nb_fresnel(s.normal, 2.0);
    float fl = 1.0 - flicker * nb_hash12(vec2(floor(nb_time * 20.0), 1.0));
    s.albedo = vec3(0.0);
    s.emission = nb_linear(color) * (0.25 + rim * 1.5) * (0.4 + 0.6 * lines) * fl;
    s.alpha = clamp(0.2 + rim, 0.0, 1.0);
}
"
        .into(),
        "dissolve" => "//! pass gbuffer
//! alpha_clip on
//! shadows on
#include <neptune/noise.glsl>
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
uniform float amount = 0.5;
uniform float edgeWidth = 0.05;
uniform vec3 edgeColor = vec3(1.0, 0.45, 0.05);
uniform float noiseScale = 6.0;
//! fragment
void nb_surface(inout NbSurface s) {
    float n = nb_fbm(nb_uv * noiseScale, 4);
    vec4 t = texture(albedoMap, nb_uv) * albedo;
    s.albedo = t.rgb;
    s.alpha = n - amount + 0.5;
    s.alphaCutoff = 0.5;
    float edge = 1.0 - smoothstep(0.0, edgeWidth, n - amount);
    s.emission = nb_linear(edgeColor) * edge * 4.0;
}
"
        .into(),
        "fire" => "//! blend add
//! cull none
//! depth_write off
//! lighting unlit
#include <neptune/noise.glsl>
uniform vec3 innerColor = vec3(1.0, 0.85, 0.3);
uniform vec3 outerColor = vec3(1.0, 0.25, 0.02);
uniform float speed = 1.6;
uniform float intensity = 3.0;
//! fragment
void nb_surface(inout NbSurface s) {
    vec2 uv = nb_uv;
    float n = nb_fbmEx(1, vec2(uv.x * 4.0, uv.y * 3.0 - nb_time * speed), 5, 0.55, 2.0);
    float shape = (1.0 - uv.y) * (1.0 - abs(uv.x - 0.5) * 2.0);
    float f = clamp(shape * 1.6 + (n - 0.5) * 1.2 - 0.25, 0.0, 1.0);
    s.albedo = vec3(0.0);
    s.emission = nb_linear(mix(outerColor, innerColor, f * f)) * f * intensity;
    s.alpha = f;
}
"
        .into(),
        "smoke" => "//! blend alpha
//! cull none
//! depth_write off
//! lighting unlit
#include <neptune/noise.glsl>
uniform vec4 color = vec4(0.55, 0.55, 0.58, 0.8);
uniform float speed = 0.3;
uniform float density = 1.2;
//! fragment
void nb_surface(inout NbSurface s) {
    vec2 uv = nb_uv;
    float n = nb_fbmEx(2, uv * 3.0 + vec2(0.0, -nb_time * speed), 5, 0.5, 2.0);
    float edge = 1.0 - smoothstep(0.25, 0.5, length(uv - 0.5));
    s.albedo = color.rgb;
    s.alpha = clamp((n - 0.35) * density, 0.0, 1.0) * edge * color.a;
}
"
        .into(),
        "forcefield" => "//! blend add
//! cull none
//! depth_write off
//! lighting unlit
#include <neptune/noise.glsl>
uniform vec3 color = vec3(0.3, 0.6, 1.0);
uniform float intersection = 0.6;
uniform float hexScale = 12.0;
uniform float pulse = 1.0;
//! fragment
void nb_surface(inout NbSurface s) {
    float rim = nb_fresnel(s.normal, 2.5);
    vec3 v = nb_voronoi(nb_uv * hexScale + vec2(nb_time * 0.1, 0.0));
    float cells = smoothstep(0.02, 0.0, v.y - v.x - 0.02);
    // glow where the field meets other geometry
    float scene = texture(fyrox_sceneDepth, nb_screenUV()).r;
    float here = gl_FragCoord.z;
    float touch = 1.0 - smoothstep(0.0, 0.002 * intersection, abs(scene - here));
    float wave = 0.5 + 0.5 * sin(nb_time * 3.0 * pulse - nb_worldPos.y * 4.0);
    s.albedo = vec3(0.0);
    s.emission = nb_linear(color) * (rim * 1.5 + cells * 0.6 * wave + touch * 2.0);
    s.alpha = 1.0;
}
"
        .into(),
        "gradient" | "noise" | "procedural" => PROCEDURAL.replace(
            "uniform int patternType = 0;",
            match name.as_str() {
                "noise" => "uniform int patternType = 6;",
                _ => "uniform int patternType = 0;",
            },
        ),
        "matcap" => "//! lighting unlit
uniform sampler2D matcap;
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
//! fragment
void nb_surface(inout NbSurface s) {
    // view-space normal picks the texel of a lit-sphere image
    vec3 v = nb_viewDir();
    vec3 up = vec3(0.0, 1.0, 0.0);
    vec3 right = normalize(cross(up, v));
    up = cross(v, right);
    vec2 uv = vec2(dot(s.normal, right), dot(s.normal, up)) * 0.495 + 0.5;
    s.albedo = texture(matcap, uv).rgb * albedo.rgb;
}
"
        .into(),
        "flat" => "// Faceted low-poly look: one normal per triangle from screen derivatives.
//! lighting lambert
//! shadows on
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
//! fragment
void nb_surface(inout NbSurface s) {
    s.normal = normalize(cross(dFdx(nb_worldPos), dFdy(nb_worldPos)));
    if (dot(s.normal, nb_normal) < 0.0) s.normal = -s.normal;
    vec4 t = texture(albedoMap, nb_uv) * albedo;
    s.albedo = t.rgb;
    s.alpha = t.a;
}
"
        .into(),
        "ps1" => "// PS1-style: vertices snapped to a coarse grid and affine texture mapping.
//! lighting lambert
uniform sampler2D albedoMap;
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform float snap = 160.0;
uniform float colorDepth = 32.0;
//! vertex
#include <neptune/attributes.glsl>
#include <neptune/skinning.glsl>
out vec3 nb_worldPos;
out vec3 nb_normal;
out vec3 nb_tangent;
out vec3 nb_binormal;
out vec2 nb_uv;
out vec2 nb_uv2;
out vec4 nb_color;
out vec4 nb_data0;
out vec4 nb_data1;
out vec4 nb_data2;
out vec4 nb_data3;
noperspective out vec2 nb_affineUV;
void main() {
    mat4 skin = nb_skinMatrix();
    vec4 p = skin * vec4(vertexPosition, 1.0);
    vec4 clip = fyrox_instanceData.worldViewProjection * p;
    vec2 grid = vec2(snap, snap * 0.75);
    clip.xy = floor(clip.xy / clip.w * grid + 0.5) / grid * clip.w;
    gl_Position = clip;
    nb_worldPos = (fyrox_instanceData.worldMatrix * p).xyz;
    nb_normal = normalize(mat3(fyrox_instanceData.worldMatrix) * mat3(skin) * vertexNormal);
    nb_tangent = vec3(1, 0, 0);
    nb_binormal = vec3(0, 0, 1);
    nb_uv = vertexTexCoord;
    nb_affineUV = vertexTexCoord;
    nb_uv2 = vertexSecondTexCoord;
    nb_color = vertexColor;
    nb_data0 = vertexData0; nb_data1 = vertexData1; nb_data2 = vertexData2; nb_data3 = vertexData3;
}
//! fragment
#define NB_VARYINGS 1
noperspective in vec2 nb_affineUV;
in vec3 nb_worldPos;
in vec3 nb_normal;
in vec3 nb_tangent;
in vec3 nb_binormal;
in vec2 nb_uv;
in vec2 nb_uv2;
in vec4 nb_color;
void nb_surface(inout NbSurface s) {
    s.normal = normalize(nb_normal);
    vec4 t = texture(albedoMap, nb_affineUV) * albedo;
    s.albedo = floor(t.rgb * colorDepth) / colorDepth;
    s.alpha = t.a;
}
"
        .into(),
        "pixelart" => "// Crisp pixels: snaps texture coordinates to the texel grid and posterizes light.
//! lighting toon
uniform sampler2D albedoMap;
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform float pixelsPerUnit = 16.0;
uniform float toonSteps = 4.0;
//! fragment
void nb_surface(inout NbSurface s) {
    vec2 size = vec2(textureSize(albedoMap, 0));
    vec2 uv = (floor(nb_uv * size) + 0.5) / size;
    vec4 t = texture(albedoMap, uv) * albedo;
    s.albedo = t.rgb;
    s.alpha = t.a;
    s.alphaCutoff = 0.5;
    s.toonSteps = toonSteps;
    s.specularColor = vec3(0.0);
    s.shadeColor = vec3(0.55);
}
"
        .into(),
        "vertexlit" => "// Lighting worked out per vertex (Gouraud), the look of early 3D hardware.
uniform sampler2D albedoMap;
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
//! vertex
#include <neptune/attributes.glsl>
#include <neptune/skinning.glsl>
#include <neptune/lighting.glsl>
out vec2 uv;
out vec3 light;
void main() {
    mat4 skin = nb_skinMatrix();
    vec4 p = skin * vec4(vertexPosition, 1.0);
    vec3 w = (fyrox_instanceData.worldMatrix * p).xyz;
    vec3 n = normalize(mat3(fyrox_instanceData.worldMatrix) * mat3(skin) * vertexNormal);
    light = nb_linear(nb_ambient);
    for (int i = 0; i < nb_lightCount(); ++i) { NbLight l = nb_light(i, w); light += l.color * max(dot(n, l.L), 0.0) * l.atten; }
    uv = vertexTexCoord;
    gl_Position = fyrox_instanceData.worldViewProjection * p;
}
//! fragment
in vec2 uv;
in vec3 light;
void main() {
    vec4 t = texture(albedoMap, uv) * albedo;
    FragColor = vec4(nb_linear(t.rgb) * light, t.a);
}
"
        .into(),
        "wind" => "// Foliage swaying in the wind: tips move more than roots (vertex height or colour red).
//! lighting lambert
//! cull none
//! alpha_clip on
//! shadows on
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
uniform vec2 windDirection = vec2(1.0, 0.0);
uniform float windStrength = 0.2;
uniform float windSpeed = 1.5;
uniform float stiffness = 1.0;
//! vertex
void nb_displaceWorld(inout vec3 p, inout vec3 n, vec2 uv) {
    vec3 root = (fyrox_instanceData.worldMatrix * vec4(0.0, 0.0, 0.0, 1.0)).xyz;
    float h = max(p.y - root.y, 0.0) / max(stiffness, 0.01);
    float phase = dot(root.xz, vec2(0.37, 0.71));
    float sway = sin(nb_time * windSpeed + phase) * 0.7 + sin(nb_time * windSpeed * 2.3 + phase * 1.7) * 0.3;
    // Blitz +X is Fyrox -X
    vec2 d = normalize(vec2(-windDirection.x, windDirection.y) + vec2(1e-5));
    p.xz += d * sway * windStrength * h * h;
}
//! fragment
void nb_surface(inout NbSurface s) {
    vec4 t = texture(albedoMap, nb_uv) * albedo;
    s.albedo = t.rgb;
    s.alpha = t.a;
}
"
        .into(),
        "displace" => "// Vertex displacement along the normal from a height map, animated.
//! lighting pbr
//! shadows on
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
uniform sampler2D heightMap; //! fallback black
uniform float amount = 0.2;
uniform float speed = 1.0;
uniform float frequency = 6.0;
//! vertex
void nb_displace(inout vec3 p, inout vec3 n, vec2 uv) {
    float h = textureLod(heightMap, uv, 0.0).r + 0.5 * sin(nb_time * speed + uv.x * frequency) * sin(uv.y * frequency);
    p += normalize(n) * h * amount;
}
//! fragment
void nb_surface(inout NbSurface s) {
    vec4 t = texture(albedoMap, nb_uv) * albedo;
    s.albedo = t.rgb;
    // derivative normal so lighting follows the displaced shape
    s.normal = normalize(cross(dFdx(nb_worldPos), dFdy(nb_worldPos)));
    if (dot(s.normal, nb_normal) < 0.0) s.normal = -s.normal;
}
"
        .into(),
        "posterize" => "//! lighting lambert
#include <neptune/color.glsl>
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
uniform float levels = 4.0;
//! fragment
void nb_surface(inout NbSurface s) {
    vec4 t = texture(albedoMap, nb_uv) * albedo;
    s.albedo = nb_posterize(t.rgb, levels);
    s.alpha = t.a;
}
"
        .into(),
        "handpainted" => "// Hand-painted look: soft wrapped light, warm/cool shading and brush-like noise.
//! lighting toon
#include <neptune/noise.glsl>
uniform vec4 albedo = vec4(1.0, 1.0, 1.0, 1.0);
uniform sampler2D albedoMap;
uniform vec3 shadeColor = vec3(0.55, 0.5, 0.75);
uniform float strokes = 0.12;
uniform float strokeScale = 18.0;
//! fragment
void nb_surface(inout NbSurface s) {
    vec4 t = texture(albedoMap, nb_uv) * albedo;
    float n = nb_fbmEx(2, nb_uv * strokeScale * vec2(1.0, 0.3), 3, 0.5, 2.0) - 0.5;
    s.albedo = clamp(t.rgb * (1.0 + n * strokes * 2.0), 0.0, 1.0);
    s.alpha = t.a;
    s.toonSteps = 6.0;
    s.shadeColor = shadeColor;
    s.specularColor = vec3(0.0);
}
"
        .into(),
        _ => return None,
    })
}

const WATER: &str = "// Animated water: Gerstner-like waves, depth colour, foam, reflections and refraction.
//! blend alpha
//! cull none
//! lighting blinn
#include <neptune/noise.glsl>
uniform vec3 shallowColor = vec3(0.10, 0.55, 0.60);
uniform vec3 deepColor = vec3(0.02, 0.12, 0.25);
uniform float waveHeight = 0.25;
uniform float waveLength = 6.0;
uniform float waveSpeed = 1.0;
uniform vec2 waveDirection = vec2(1.0, 0.3);
uniform float depthFade = 4.0;
uniform float reflection = 0.6;
uniform float refraction = 0.5;
uniform float foamAmount = 0.5;
uniform vec3 foamColor = vec3(1.0, 1.0, 1.0);
uniform samplerCube envMap; //! fallback black
uniform float opacity = 0.85;
//! vertex
vec2 nb_rotate2(vec2 v, float a) { float c = cos(a), s = sin(a); return vec2(c * v.x - s * v.y, s * v.x + c * v.y); }
vec3 nb_wave(vec2 p, vec2 dir, float len, float h, float t) {
    float k = 6.2831853 / max(len, 0.01);
    float c = sqrt(9.8 / k);
    vec2 d = normalize(dir + vec2(1e-5));
    float f = k * (dot(d, p) - c * t);
    float q = h * k * 0.5;
    return vec3(d.x * q * cos(f) / k, h * sin(f), d.y * q * cos(f) / k);
}
void nb_displaceWorld(inout vec3 p, inout vec3 n, vec2 uv) {
    float t = nb_time * waveSpeed;
    // Blitz +X is Fyrox -X
    vec2 dir = vec2(-waveDirection.x, waveDirection.y);
    vec3 o = nb_wave(p.xz, dir, waveLength, waveHeight, t)
           + nb_wave(p.xz, nb_rotate2(dir, 0.6), waveLength * 0.57, waveHeight * 0.5, t * 1.1)
           + nb_wave(p.xz, nb_rotate2(dir, -0.9), waveLength * 0.31, waveHeight * 0.25, t * 1.3);
    // normal from neighbouring offsets
    float e = 0.1;
    vec3 ox = nb_wave(p.xz + vec2(e, 0.0), dir, waveLength, waveHeight, t) + nb_wave(p.xz + vec2(e, 0.0), nb_rotate2(dir, 0.6), waveLength * 0.57, waveHeight * 0.5, t * 1.1);
    vec3 oz = nb_wave(p.xz + vec2(0.0, e), dir, waveLength, waveHeight, t) + nb_wave(p.xz + vec2(0.0, e), nb_rotate2(dir, 0.6), waveLength * 0.57, waveHeight * 0.5, t * 1.1);
    vec3 tx = vec3(e, 0.0, 0.0) + ox - o;
    vec3 tz = vec3(0.0, 0.0, e) + oz - o;
    n = normalize(cross(tz, tx));
    p += o;
}
//! fragment
void nb_surface(inout NbSurface s) {
    vec3 n = normalize(s.normal);
    // small ripples
    vec2 r = nb_worldPos.xz * 1.5 + nb_time * waveSpeed * vec2(0.3, 0.2);
    n = normalize(n + vec3(nb_perlin(r) - 0.5, 0.0, nb_perlin(r + 7.1) - 0.5) * 0.3);
    s.normal = n;
    // depth behind the surface from the scene depth buffer
    vec2 suv = nb_screenUV() + n.xz * 0.02 * refraction;
    float sceneZ = texture(fyrox_sceneDepth, suv).r;
    float zn = fyrox_cameraData.zNear, zf = fyrox_cameraData.zFar;
    float lin = (2.0 * zn * zf) / (zf + zn - (sceneZ * 2.0 - 1.0) * (zf - zn));
    float here = (2.0 * zn * zf) / (zf + zn - (gl_FragCoord.z * 2.0 - 1.0) * (zf - zn));
    float depth = max(lin - here, 0.0);
    float t = clamp(depth / max(depthFade, 0.01), 0.0, 1.0);
    vec3 col = mix(shallowColor, deepColor, t);
    float fr = nb_fresnel(n, 4.0);
    vec3 refl = texture(envMap, reflect(-nb_viewDir(), n)).rgb;
    float foam = (1.0 - smoothstep(0.0, 0.6 * foamAmount, depth)) * step(0.001, foamAmount);
    foam = max(foam, smoothstep(0.75, 1.0, nb_perlin(nb_worldPos.xz * 0.8 + nb_time * 0.2)) * foamAmount * 0.4);
    s.albedo = mix(col, foamColor, foam);
    s.alpha = clamp(mix(opacity * mix(0.6, 1.0, t), 1.0, max(fr * 0.5, foam)), 0.0, 1.0);
    s.specularColor = vec3(1.0);
    s.shininess = 128.0;
    s.emission = refl * reflection * fr;
}
";

const PROCEDURAL: &str = "// Procedural material: gradients, noise and patterns mixed without textures.
//! lighting pbr
//! shadows on
#include <neptune/procedural.glsl>
uniform int gradientType = 0;         // 0 linear, 1 radial, 2 angular, 3 none
uniform vec4 gradientColorA = vec4(0.1, 0.2, 0.6, 1.0);
uniform vec4 gradientColorB = vec4(0.9, 0.6, 0.2, 1.0);
uniform float gradientDirection = 90.0;
uniform int noiseType = 1;            // 0 value, 1 Perlin, 2 simplex, 3 Voronoi, 4 cellular
uniform float noiseScale = 4.0;
uniform float noiseSpeed = 0.0;
uniform int noiseOctaves = 4;
uniform float noisePersistence = 0.5;
uniform float noiseLacunarity = 2.0;
uniform float noiseStrength = 0.0;
uniform int patternType = 0;          // 0 none, 1 checker, 2 grid, 3 stripes, 4 dots, 5 Voronoi cells, 6 noise
uniform float patternScale = 8.0;
uniform vec4 patternColor = vec4(1.0, 1.0, 1.0, 1.0);
uniform float patternRotation = 0.0;
uniform float distortion = 0.0;
uniform float seed = 0.0;
uniform float roughness = 0.6;
uniform float metallic = 0.0;
uniform float bumpStrength = 0.0;
//! fragment
float nb_procNoise(vec2 p) {
    return nb_fbmEx(noiseType, p * noiseScale + vec2(seed * 13.17, seed * 7.31) + nb_time * noiseSpeed, noiseOctaves, noisePersistence, noiseLacunarity);
}
float nb_pattern(vec2 uv) {
    vec2 p = nb_rotate(uv - 0.5, patternRotation) * patternScale;
    if (patternType == 1) return nb_checker(p);
    if (patternType == 2) return nb_grid(p, 0.5);
    if (patternType == 3) return nb_stripes(p.x, 0.5);
    if (patternType == 4) return nb_dots(p, 0.3);
    if (patternType == 5) { vec3 v = nb_voronoi(p + seed); return v.z; }
    if (patternType == 6) return nb_procNoise(uv);
    return 0.0;
}
void nb_surface(inout NbSurface s) {
    vec2 uv = nb_uv;
    if (distortion != 0.0) uv += (vec2(nb_procNoise(uv), nb_procNoise(uv + 3.7)) - 0.5) * distortion;
    float g = gradientType == 0 ? nb_linearGradient(uv, gradientDirection)
            : gradientType == 1 ? nb_radialGradient(uv)
            : gradientType == 2 ? nb_angularGradient(uv, gradientDirection) : 0.0;
    g = clamp(g + (nb_procNoise(uv) - 0.5) * noiseStrength, 0.0, 1.0);
    vec4 c = mix(gradientColorA, gradientColorB, g);
    float p = nb_pattern(uv);
    c = mix(c, patternColor, p * patternColor.a);
    s.albedo = c.rgb;
    s.alpha = c.a;
    s.roughness = roughness;
    s.metallic = metallic;
    if (bumpStrength > 0.0) {
        vec3 tn = NB_HEIGHT_NORMAL(nb_procNoise, uv, 0.002, bumpStrength * 40.0);
        s.normal = nb_normalFromMap(tn * 0.5 + 0.5, 1.0);
    }
}
";
