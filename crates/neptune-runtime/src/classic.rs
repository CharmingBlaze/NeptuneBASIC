//! The classic material: a forward-rendered shader with Blitz3D's fixed
//! function features that Fyrox's PBR material does not have.
//!
//! Entities use Fyrox's standard (deferred, PBR) material by default. An
//! entity switches to this material as soon as it needs something only the
//! classic pipeline can do: `EntityBlend` multiply/add, `EntityFX`
//! (fullbright, vertex colours, flat shading, two-sided), `EntityShininess`,
//! more than one texture layer, texture transforms, sphere mapping, alpha,
//! or camera fog. The shader is cheap (one pass, up to 16 per-pixel lights,
//! GLSL 3.30), so it also suits older GPUs.
//!
//! Draw state (blending, culling, depth writes) is fixed per shader in
//! Fyrox, so one shader variant is built per combination and cached.

use std::cell::RefCell;
use std::collections::HashMap;

use fyrox::{
    asset::untyped::ResourceKind,
    core::{
        algebra::{Vector2, Vector3, Vector4},
        color::Color,
        uuid::Uuid,
    },
    material::{
        shader::{Shader, ShaderResource},
        Material, MaterialResource,
    },
    resource::texture::TextureResource,
};

/// `EntityBlend` modes.
pub const BLEND_ALPHA: i32 = 1;
pub const BLEND_MULTIPLY: i32 = 2;
pub const BLEND_ADD: i32 = 3;

/// `EntityFX` flags.
pub mod fx {
    pub const FULLBRIGHT: i32 = 1;
    pub const VERTEX_COLOR: i32 = 2;
    pub const FLAT: i32 = 4;
    pub const NO_FOG: i32 = 8;
    pub const TWO_SIDED: i32 = 16;
    pub const FORCE_ALPHA: i32 = 32;
}

/// Number of texture layers the classic shader blends (Blitz3D has 8;
/// four covers base, lightmap, detail and environment maps).
pub const MAX_LAYERS: usize = 4;

/// One texture layer of a classic material.
#[derive(Clone, Debug)]
pub struct Layer {
    pub texture: TextureResource,
    /// `TextureBlend`: 0 off, 1 alpha, 2 multiply, 3 add, 4 dot3, 5 multiply x2.
    pub blend: i32,
    /// `ScaleTexture` (u, v), `PositionTexture` (u, v).
    pub scale: (f32, f32),
    pub offset: (f32, f32),
    /// `RotateTexture`, degrees.
    pub rotation: f32,
    /// `TextureCoords`: 0 first UV set, 1 second.
    pub coords: i32,
    /// Texture flag 64: spherical environment map.
    pub sphere_map: bool,
    /// Cube map mode (0 none, 1 reflection, 2 normal, 3 refraction), or 4:
    /// a mirror's reflection, projected with the `mirrorMatrix` property.
    pub cube: i32,
    /// Terrain painting (ext): 1-3 blends this layer over the ones below by
    /// the vertex colour's red, green or blue (the painted weight); 0 off.
    pub splat: i32,
}

/// Everything the classic material shows for one entity.
#[derive(Clone, Debug)]
pub struct ClassicState {
    pub color: Color,
    pub shininess: f32,
    pub fx: i32,
    pub blend: i32,
    pub layers: Vec<Layer>,
    pub normal_map: Option<TextureResource>,
    /// Alpha-tested ("masked") textures discard transparent pixels.
    pub alpha_test: bool,
}

/// Scene-wide settings that every classic material reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Globals {
    /// 0 off, 1 linear (`CameraFogMode`).
    pub fog_mode: i32,
    pub fog_color: Color,
    pub fog_range: (f32, f32),
}

impl Default for Globals {
    fn default() -> Self {
        Globals { fog_mode: 0, fog_color: Color::BLACK, fog_range: (1.0, 1000.0) }
    }
}

impl ClassicState {
    /// Whether the entity's surfaces must be drawn in the forward, blended pass.
    pub fn transparent(&self) -> bool {
        self.color.a < 255 || self.blend != BLEND_ALPHA || self.fx & fx::FORCE_ALPHA != 0
    }

    /// Builds the material for these settings.
    pub fn material(&self, globals: &Globals) -> MaterialResource {
        let two_sided = self.fx & fx::TWO_SIDED != 0;
        let mut m = Material::from_shader(shader(self.blend, two_sided, self.transparent()));
        self.apply(&mut m, globals);
        MaterialResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, m)
    }

    /// The material for these settings, shared with every other surface
    /// that has exactly the same settings, so copies of an entity (forests,
    /// rocks) use one material and can be drawn together (`EntityBatch`).
    /// Mirrors are never shared: their material changes every frame.
    pub fn shared_material(&self, globals: &Globals) -> MaterialResource {
        if self.layers.iter().any(|l| l.cube == 4) {
            return self.material(globals);
        }
        let mut key = format!(
            "{:?}|{}|{}|{}|{}|{:?}|{:?}",
            self.color,
            self.shininess.to_bits(),
            self.fx,
            self.blend,
            self.alpha_test,
            self.normal_map.as_ref().map(|t| t.key()),
            globals
        );
        for l in &self.layers {
            key += &format!(
                "|{:?},{},{:?},{:?},{},{},{},{},{}",
                l.texture.key(),
                l.blend,
                (l.scale.0.to_bits(), l.scale.1.to_bits()),
                (l.offset.0.to_bits(), l.offset.1.to_bits()),
                l.rotation.to_bits(),
                l.coords,
                l.sphere_map,
                l.cube,
                l.splat
            );
        }
        SHARED.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() > 4096 {
                cache.clear();
            }
            cache.entry(key).or_insert_with(|| self.material(globals)).clone()
        })
    }

    /// Writes every property into an existing classic material.
    pub fn apply(&self, m: &mut Material, globals: &Globals) {
        m.set_property("diffuseColor", self.color);
        m.set_property("shininess", self.shininess.clamp(0.0, 1.0));
        m.set_property("fx", self.fx);
        m.set_property("alphaTest", self.alpha_test as i32);
        m.set_property("layerCount", self.layers.len().min(MAX_LAYERS) as i32);
        for i in 0..MAX_LAYERS {
            let name = |s: &str| format!("{s}{i}");
            match self.layers.get(i) {
                Some(l) => {
                    m.bind(name("layerTexture"), l.texture.clone());
                    m.set_property(name("layerBlend"), l.blend);
                    m.set_property(
                        name("layerTransform"),
                        Vector4::new(1.0 / nonzero(l.scale.0), 1.0 / nonzero(l.scale.1), l.offset.0, l.offset.1),
                    );
                    m.set_property(
                        name("layerOptions"),
                        // z: 1 sphere map, 2-4 cube map reflection/normal/refraction
                        Vector4::new(
                            l.rotation.to_radians(),
                            l.coords as f32,
                            if l.cube > 0 { (l.cube + 1) as f32 } else { l.sphere_map as i32 as f32 },
                            l.splat as f32,
                        ),
                    );
                }
                None => {
                    m.unbind(name("layerTexture"));
                    m.set_property(name("layerBlend"), 0);
                }
            }
        }
        match &self.normal_map {
            Some(t) => {
                m.bind("normalTexture", t.clone());
                m.set_property("useNormalMap", 1);
            }
            None => {
                m.unbind("normalTexture");
                m.set_property("useNormalMap", 0);
            }
        }
        apply_globals(m, globals);
    }
}

/// Updates the scene-wide properties (fog) of a classic material.
pub fn apply_globals(m: &mut Material, g: &Globals) {
    m.set_property("fogMode", g.fog_mode);
    m.set_property("fogColor", Vector3::new(g.fog_color.r as f32, g.fog_color.g as f32, g.fog_color.b as f32) / 255.0);
    m.set_property("fogRange", Vector2::new(g.fog_range.0, g.fog_range.1));
}

fn nonzero(v: f32) -> f32 {
    if v.abs() < 1e-6 {
        1e-6
    } else {
        v
    }
}

/// Is this material one of ours?
pub fn is_classic(m: &Material) -> bool {
    m.shader().data_ref().as_loaded_ref().is_some_and(|s| s.definition.name.starts_with("NeptuneClassic"))
}

thread_local! {
    static SHARED: RefCell<HashMap<String, MaterialResource>> = RefCell::new(HashMap::new());
    static SHADERS: RefCell<HashMap<(i32, bool, bool), ShaderResource>> = RefCell::new(HashMap::new());
}

/// The cached shader variant for a blend mode, culling and depth-write setting.
pub fn shader(blend: i32, two_sided: bool, transparent: bool) -> ShaderResource {
    let key = (blend, two_sided, transparent);
    SHADERS.with(|cache| {
        cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let src = source(blend, two_sided, transparent);
                let shader = Shader::from_string(&src).expect("built-in classic shader is valid");
                ShaderResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, shader)
            })
            .clone()
    })
}

/// RON blend parameters for a blend mode.
fn blend_params(blend: i32, transparent: bool) -> &'static str {
    if !transparent {
        return "None";
    }
    match blend {
        BLEND_MULTIPLY => {
            "Some(BlendParameters(func: BlendFunc(sfactor: DstColor, dfactor: Zero, alpha_sfactor: Zero, alpha_dfactor: One), equation: BlendEquation(rgb: Add, alpha: Add)))"
        }
        BLEND_ADD => {
            "Some(BlendParameters(func: BlendFunc(sfactor: SrcAlpha, dfactor: One, alpha_sfactor: Zero, alpha_dfactor: One), equation: BlendEquation(rgb: Add, alpha: Add)))"
        }
        _ => {
            "Some(BlendParameters(func: BlendFunc(sfactor: SrcAlpha, dfactor: OneMinusSrcAlpha, alpha_sfactor: SrcAlpha, alpha_dfactor: OneMinusSrcAlpha), equation: BlendEquation(rgb: Add, alpha: Add)))"
        }
    }
}

const SKINNING: &str = r#"
    vec4 skin(vec4 p) {
        if (!fyrox_instanceData.useSkeletalAnimation) {
            return p;
        }
        mat4 m0 = fyrox_boneMatrices.matrices[int(boneIndices.x)];
        mat4 m1 = fyrox_boneMatrices.matrices[int(boneIndices.y)];
        mat4 m2 = fyrox_boneMatrices.matrices[int(boneIndices.z)];
        mat4 m3 = fyrox_boneMatrices.matrices[int(boneIndices.w)];
        return m0 * p * boneWeights.x + m1 * p * boneWeights.y + m2 * p * boneWeights.z + m3 * p * boneWeights.w;
    }
    vec3 skinDir(vec3 d) {
        if (!fyrox_instanceData.useSkeletalAnimation) {
            return d;
        }
        mat4 m0 = fyrox_boneMatrices.matrices[int(boneIndices.x)];
        mat4 m1 = fyrox_boneMatrices.matrices[int(boneIndices.y)];
        mat4 m2 = fyrox_boneMatrices.matrices[int(boneIndices.z)];
        mat4 m3 = fyrox_boneMatrices.matrices[int(boneIndices.w)];
        return mat3(m0) * d * boneWeights.x + mat3(m1) * d * boneWeights.y + mat3(m2) * d * boneWeights.z + mat3(m3) * d * boneWeights.w;
    }
"#;

fn shadow_pass(name: &str, cull: &str, point: bool) -> String {
    let (out_decl, out_body, world_out, world_in) = if point {
        (
            "layout(location = 0) out float depth;",
            "depth = length(fyrox_lightData.lightPosition - worldPosition);",
            "worldPosition = (fyrox_instanceData.worldMatrix * p).xyz;",
            "in vec3 worldPosition;",
        )
    } else {
        ("", "", "", "")
    };
    let color = if point { "true" } else { "false" };
    let world_decl = if point { "out vec3 worldPosition;" } else { "" };
    format!(
        r##"(
            name: "{name}",
            draw_parameters: DrawParameters(
                cull_face: {cull},
                color_write: ColorMask(red: {color}, green: {color}, blue: {color}, alpha: {color}),
                depth_write: true,
                stencil_test: None,
                depth_test: Some(Less),
                blend: None,
                stencil_op: StencilOp(fail: Keep, zfail: Keep, zpass: Keep, write_mask: 0xFFFF_FFFF),
                scissor_box: None
            ),
            vertex_shader: r#"
                layout(location = 0) in vec3 vertexPosition;
                layout(location = 1) in vec2 vertexTexCoord;
                layout(location = 4) in vec4 boneWeights;
                layout(location = 5) in vec4 boneIndices;
                out vec2 texCoord;
                {world_decl}
                {SKINNING}
                void main() {{
                    vec4 p = skin(vec4(vertexPosition, 1.0));
                    gl_Position = fyrox_instanceData.worldViewProjection * p;
                    texCoord = vertexTexCoord;
                    {world_out}
                }}
            "#,
            fragment_shader: r#"
                in vec2 texCoord;
                {world_in}
                {out_decl}
                void main() {{
                    if (properties.alphaTest != 0 && texture(layerTexture0, texCoord).a < 0.5) discard;
                    {out_body}
                }}
            "#,
        )"##
    )
}

/// Generates the shader source for one variant.
pub fn source(blend: i32, two_sided: bool, transparent: bool) -> String {
    let cull = if two_sided { "None" } else { "Some(Back)" };
    let blend_ron = blend_params(blend, transparent);
    // additive and multiplied surfaces should not hide what is behind them
    let depth_write = !transparent || blend == BLEND_ALPHA;
    let mut layer_res = String::new();
    let mut layer_props = String::new();
    for i in 0..MAX_LAYERS {
        layer_res += &format!(
            "(name: \"layerTexture{i}\", kind: Texture(kind: Sampler2D, fallback: White), binding: {i}),\n"
        );
        layer_props += &format!(
            "(name: \"layerBlend{i}\", kind: Int(value: 0)),\n\
             (name: \"layerTransform{i}\", kind: Vector4(value: (1.0, 1.0, 0.0, 0.0))),\n\
             (name: \"layerOptions{i}\", kind: Vector4(value: (0.0, 0.0, 0.0, 0.0))),\n"
        );
    }
    let normal_binding = MAX_LAYERS;
    let shadows = [
        shadow_pass("DirectionalShadow", cull, false),
        shadow_pass("SpotShadow", cull, false),
        shadow_pass("PointShadow", cull, true),
    ]
    .join(",\n");
    let disabled = if transparent {
        // transparent surfaces do not cast shadows
        r#"disabled_passes: ["GBuffer", "DirectionalShadow", "SpotShadow", "PointShadow"],"#
    } else {
        r#"disabled_passes: ["GBuffer"],"#
    };
    let shadow_list = if transparent { String::new() } else { format!(",\n{shadows}") };
    format!(
        r##"(
    name: "NeptuneClassic_{blend}_{two_sided}_{transparent}",
    resources: [
        {layer_res}
        (name: "normalTexture", kind: Texture(kind: Sampler2D, fallback: Normal), binding: {normal_binding}),
        (
            name: "properties",
            kind: PropertyGroup([
                (name: "diffuseColor", kind: Color(r: 255, g: 255, b: 255, a: 255)),
                (name: "shininess", kind: Float(value: 0.0)),
                (name: "fx", kind: Int(value: 0)),
                (name: "alphaTest", kind: Int(value: 0)),
                (name: "useNormalMap", kind: Int(value: 0)),
                (name: "layerCount", kind: Int(value: 0)),
                {layer_props}
                (name: "mirrorMatrix", kind: Matrix4(value: (1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0))),
                (name: "fogMode", kind: Int(value: 0)),
                (name: "fogColor", kind: Vector3(value: (0.0, 0.0, 0.0))),
                (name: "fogRange", kind: Vector2(value: (1.0, 1000.0))),
            ]),
            binding: 0
        ),
        (name: "fyrox_instanceData", kind: PropertyGroup([]), binding: 1),
        (name: "fyrox_boneMatrices", kind: PropertyGroup([]), binding: 2),
        (name: "fyrox_cameraData", kind: PropertyGroup([]), binding: 3),
        (name: "fyrox_lightData", kind: PropertyGroup([]), binding: 4),
        (name: "fyrox_lightsBlock", kind: PropertyGroup([]), binding: 5),
    ],
    {disabled}
    passes: [
        (
            name: "Forward",
            draw_parameters: DrawParameters(
                cull_face: {cull},
                color_write: ColorMask(red: true, green: true, blue: true, alpha: true),
                depth_write: {depth_write},
                stencil_test: None,
                depth_test: Some(LessOrEqual),
                blend: {blend_ron},
                stencil_op: StencilOp(fail: Keep, zfail: Keep, zpass: Keep, write_mask: 0xFFFF_FFFF),
                scissor_box: None
            ),
            vertex_shader: r#"
                layout(location = 0) in vec3 vertexPosition;
                layout(location = 1) in vec2 vertexTexCoord;
                layout(location = 2) in vec3 vertexNormal;
                layout(location = 3) in vec4 vertexTangent;
                layout(location = 4) in vec4 boneWeights;
                layout(location = 5) in vec4 boneIndices;
                layout(location = 6) in vec2 vertexSecondTexCoord;
                layout(location = 7) in vec4 vertexColor;

                out vec3 worldPosition;
                out vec3 worldNormal;
                out vec3 worldTangent;
                out vec3 worldBinormal;
                out vec2 uv0;
                out vec2 uv1;
                out vec4 vcolor;
                {SKINNING}
                void main() {{
                    vec4 p = skin(vec4(vertexPosition, 1.0));
                    mat3 nm = mat3(fyrox_instanceData.worldMatrix);
                    worldPosition = (fyrox_instanceData.worldMatrix * p).xyz;
                    worldNormal = normalize(nm * skinDir(vertexNormal));
                    worldTangent = normalize(nm * skinDir(vertexTangent.xyz));
                    worldBinormal = vertexTangent.w * cross(worldNormal, worldTangent);
                    uv0 = vertexTexCoord;
                    uv1 = vertexSecondTexCoord;
                    vcolor = vertexColor;
                    gl_Position = fyrox_instanceData.worldViewProjection * p;
                }}
            "#,
            fragment_shader: r#"
                out vec4 FragColor;

                in vec3 worldPosition;
                in vec3 worldNormal;
                in vec3 worldTangent;
                in vec3 worldBinormal;
                in vec2 uv0;
                in vec2 uv1;
                in vec4 vcolor;

                // Blitz cube map stored as a strip of six faces: left, front,
                // right, back, up, down. `d` is a Blitz-space direction.
                vec2 cubeUV(vec3 d) {{
                    vec3 a = abs(d);
                    float face;
                    vec2 st;
                    if (a.x >= a.y && a.x >= a.z) {{
                        if (d.x < 0.0) {{ face = 0.0; st = vec2(d.z, -d.y) / a.x; }}
                        else {{ face = 2.0; st = vec2(-d.z, -d.y) / a.x; }}
                    }} else if (a.z >= a.y) {{
                        if (d.z > 0.0) {{ face = 1.0; st = vec2(d.x, -d.y) / a.z; }}
                        else {{ face = 3.0; st = vec2(-d.x, -d.y) / a.z; }}
                    }} else {{
                        if (d.y > 0.0) {{ face = 4.0; st = vec2(d.x, d.z) / a.y; }}
                        else {{ face = 5.0; st = vec2(d.x, -d.z) / a.y; }}
                    }}
                    // stay a little inside the face so filtering does not bleed
                    vec2 local = clamp(st * 0.5 + 0.5, 0.003, 0.997);
                    return vec2((face + local.x) / 6.0, local.y);
                }}

                vec2 layerUV(vec4 xf, vec4 opt, vec3 n) {{
                    vec2 uv;
                    if (opt.z > 4.5) {{
                        // mirror: where this point lands in the reflected camera's image
                        vec4 c = properties.mirrorMatrix * vec4(worldPosition, 1.0);
                        return c.xy / c.w * 0.5 + 0.5;
                    }}
                    if (opt.z > 1.5) {{
                        vec3 v = normalize(worldPosition - fyrox_cameraData.position);
                        vec3 d = opt.z < 2.5 ? reflect(v, n) : (opt.z < 3.5 ? n : refract(v, n, 0.9));
                        // Fyrox space is Blitz space mirrored on X
                        return cubeUV(vec3(-d.x, d.y, d.z));
                    }}
                    if (opt.z > 0.5) {{
                        // spherical environment map from the view-space reflection
                        vec3 v = normalize(worldPosition - fyrox_cameraData.position);
                        vec3 r = reflect(v, n);
                        float m = 2.0 * sqrt(r.x * r.x + r.y * r.y + (r.z + 1.0) * (r.z + 1.0));
                        uv = r.xy / max(m, 1e-4) + 0.5;
                    }} else {{
                        uv = opt.y > 0.5 ? uv1 : uv0;
                    }}
                    float c = cos(opt.x);
                    float s = sin(opt.x);
                    uv = mat2(c, -s, s, c) * uv;
                    return uv * xf.xy + xf.zw;
                }}

                vec4 blendLayer(vec4 dst, vec4 src, int mode) {{
                    if (mode == 1) return vec4(mix(dst.rgb, src.rgb, src.a), dst.a);
                    if (mode == 2) return dst * src;
                    if (mode == 3) return vec4(dst.rgb + src.rgb, dst.a * src.a);
                    if (mode == 4) {{
                        float d = clamp(dot(dst.rgb * 2.0 - 1.0, src.rgb * 2.0 - 1.0), 0.0, 1.0);
                        return vec4(vec3(d), dst.a);
                    }}
                    if (mode == 5) return vec4(dst.rgb * src.rgb * 2.0, dst.a * src.a);
                    return dst;
                }}

                // a painted terrain layer: mixed in by the vertex colour channel it was painted into
                vec4 splatLayer(vec4 dst, vec4 src, int mode, float channel) {{
                    if (channel < 0.5) return blendLayer(dst, src, mode);
                    float w = channel < 1.5 ? vcolor.r : (channel < 2.5 ? vcolor.g : vcolor.b);
                    return vec4(mix(dst.rgb, src.rgb, clamp(w, 0.0, 1.0)), dst.a);
                }}

                void main() {{
                    vec3 n = normalize(worldNormal);
                    if ((properties.fx & 4) != 0) {{
                        // flat shading: the face normal from screen-space derivatives
                        n = normalize(cross(dFdx(worldPosition), dFdy(worldPosition)));
                    }}
                    if (!gl_FrontFacing) n = -n;
                    if (properties.useNormalMap != 0) {{
                        vec3 tn = texture(normalTexture, uv0).xyz * 2.0 - 1.0;
                        n = normalize(mat3(normalize(worldTangent), normalize(worldBinormal), n) * tn);
                    }}

                    vec4 base = S_SRGBToLinear(properties.diffuseColor);
                    if ((properties.fx & 2) != 0) base *= S_SRGBToLinear(vcolor);

                    vec4 tex = vec4(1.0);
                    if (properties.layerCount > 0) tex = S_SRGBToLinear(texture(layerTexture0, layerUV(properties.layerTransform0, properties.layerOptions0, n)));
                    if (properties.layerCount > 1) tex = splatLayer(tex, S_SRGBToLinear(texture(layerTexture1, layerUV(properties.layerTransform1, properties.layerOptions1, n))), properties.layerBlend1, properties.layerOptions1.w);
                    if (properties.layerCount > 2) tex = splatLayer(tex, S_SRGBToLinear(texture(layerTexture2, layerUV(properties.layerTransform2, properties.layerOptions2, n))), properties.layerBlend2, properties.layerOptions2.w);
                    if (properties.layerCount > 3) tex = splatLayer(tex, S_SRGBToLinear(texture(layerTexture3, layerUV(properties.layerTransform3, properties.layerOptions3, n))), properties.layerBlend3, properties.layerOptions3.w);
                    // layer 0 uses its own blend mode against the vertex/entity colour
                    vec4 albedo = properties.layerCount > 0 && properties.layerBlend0 != 2
                        ? blendLayer(base, tex, properties.layerBlend0)
                        : base * tex;

                    if (properties.alphaTest != 0 && albedo.a < 0.5) discard;

                    vec3 color;
                    if ((properties.fx & 1) != 0) {{
                        color = albedo.rgb;
                    }} else {{
                        vec3 diffuse = fyrox_lightData.ambientLightColor.rgb;
                        vec3 specular = vec3(0.0);
                        vec3 toEye = normalize(fyrox_cameraData.position - worldPosition);
                        for (int i = 0; i < min(fyrox_lightsBlock.lightCount, 16); ++i) {{
                            vec3 lightColor = fyrox_lightsBlock.lightsColorRadius[i].xyz;
                            float radius = fyrox_lightsBlock.lightsColorRadius[i].w;
                            vec3 dir = fyrox_lightsBlock.lightsDirection[i];
                            vec3 l;
                            float atten = 1.0;
                            if (isinf(radius)) {{
                                l = normalize(dir);
                            }} else {{
                                vec3 toLight = fyrox_lightsBlock.lightsPosition[i] - worldPosition;
                                float d = length(toLight);
                                l = toLight / max(d, 1e-4);
                                atten = S_LightDistanceAttenuation(d, radius);
                                float hot = fyrox_lightsBlock.lightsParameters[i].x;
                                float cone = fyrox_lightsBlock.lightsParameters[i].y;
                                if (cone > -0.999) {{
                                    atten *= smoothstep(cone, hot, dot(normalize(dir), l));
                                }}
                            }}
                            float ndl = max(dot(n, l), 0.0);
                            diffuse += lightColor * ndl * atten;
                            if (properties.shininess > 0.0 && ndl > 0.0) {{
                                vec3 h = normalize(l + toEye);
                                specular += lightColor * properties.shininess * pow(max(dot(n, h), 0.0), 64.0) * atten;
                            }}
                        }}
                        color = albedo.rgb * diffuse + specular;
                    }}

                    if (properties.fogMode != 0 && (properties.fx & 8) == 0) {{
                        float dist = length(worldPosition - fyrox_cameraData.position);
                        float f = clamp((dist - properties.fogRange.x) / max(properties.fogRange.y - properties.fogRange.x, 1e-4), 0.0, 1.0);
                        vec3 fogLinear = S_SRGBToLinear(vec4(properties.fogColor, 1.0)).rgb;
                        color = mix(color, fogLinear, f);
                    }}

                    FragColor = vec4(color, albedo.a);
                }}
            "#,
        ){shadow_list}
    ],
)"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_parses() {
        for blend in [BLEND_ALPHA, BLEND_MULTIPLY, BLEND_ADD] {
            for two_sided in [false, true] {
                for transparent in [false, true] {
                    let src = source(blend, two_sided, transparent);
                    if let Err(e) = Shader::from_string(&src) {
                        panic!("variant ({blend}, {two_sided}, {transparent}) failed: {e:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn transparency_rules() {
        let s = |color: Color, blend: i32, fx: i32| ClassicState {
            color,
            shininess: 0.0,
            fx,
            blend,
            layers: Vec::new(),
            normal_map: None,
            alpha_test: false,
        };
        assert!(!s(Color::WHITE, BLEND_ALPHA, 0).transparent());
        assert!(s(Color::from_rgba(255, 255, 255, 128), BLEND_ALPHA, 0).transparent());
        assert!(s(Color::WHITE, BLEND_ADD, 0).transparent());
        assert!(s(Color::WHITE, BLEND_ALPHA, fx::FORCE_ALPHA).transparent());
    }
}
