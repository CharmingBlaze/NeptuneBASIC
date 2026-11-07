//! Ready-made materials (ext): PBR, toon/stylized, water, foliage and
//! procedural materials, built on the shader library (`shaderlib.rs`).
//!
//! Each `Create*Material` returns a shader object (use it with
//! `EntityMaterial`, `ShaderApply` or `EntityShader`, and any `ShaderSet*`
//! command). Objects made from the same built-in shader share one compiled
//! shader and GPU pipeline; each has its own parameters. The `Set*`
//! commands below set the parameters by their friendly names and work on
//! any shader that declares them.
//!
//! `BakeProceduralTexture` evaluates a procedural material on the CPU into
//! a texture, so the pattern can feed any texture input (an albedo, a
//! roughness or a normal map of another material, a sprite, a 2D image).

use neptune_vm::value::Value;

use crate::commands::Cmd;
use crate::shader::Val;
use crate::world::Host3D;

const VOID: Result<Value, String> = Ok(Value::Null);

pub const SHADING_TOON: i32 = 0;
pub const SHADING_CEL: i32 = 1;
pub const SHADING_FLAT: i32 = 2;
pub const SHADING_UNLIT: i32 = 3;
pub const SHADING_VERTEXLIT: i32 = 4;
pub const SHADING_PIXEL: i32 = 5;
pub const SHADING_LAMBERT: i32 = 6;
pub const SHADING_PHONG: i32 = 7;
pub const SHADING_PBR: i32 = 8;
pub const SHADING_MATCAP: i32 = 9;
pub const SHADING_PS1: i32 = 10;
pub const SHADING_POSTERIZE: i32 = 11;
pub const SHADING_HANDPAINTED: i32 = 12;
pub const SHADING_RIM: i32 = 13;

const SHADING: &[&str] = &["toon", "cel", "flat", "unlit", "vertexlit", "pixelart", "diffuse", "phong", "textured", "matcap", "ps1", "posterize", "handpainted", "rim"];

fn shading_name(mode: i32) -> Result<&'static str, String> {
    usize::try_from(mode).ok().and_then(|m| SHADING.get(m)).copied().ok_or_else(|| format!("shading mode must be 0-{}, not {mode}", SHADING.len() - 1))
}

/// Sets a named parameter, with an error naming the material.
fn mset(h: &mut Host3D, m: i32, name: &str, v: Vec<f32>) -> Result<(), String> {
    h.ensure_compiled(m)?;
    if h.shader_obj(m)?.param(name).is_none() {
        return Err(format!("Material {m} ({}) has no '{name}' parameter", h.shader_obj(m)?.name));
    }
    h.shader_set(m, name, Val::Floats(v))
}

fn mtex(h: &mut Host3D, m: i32, name: &str, t: i32) -> Result<(), String> {
    h.ensure_compiled(m)?;
    if h.shader_obj(m)?.param(name).is_none() {
        return Err(format!("Material {m} ({}) has no '{name}' texture", h.shader_obj(m)?.name));
    }
    h.shader_set(m, name, Val::Texture(t))
}

fn rgb(a: &[Value], i: usize) -> Vec<f32> {
    (i..i + 3).map(|k| a[k].float() / 255.0).collect()
}

fn rgba(a: &[Value], i: usize) -> Vec<f32> {
    (i..i + 4).map(|k| a[k].float() / 255.0).collect()
}

fn int(v: i32) -> Result<Value, String> {
    Ok(Value::Int(v))
}

impl Host3D<'_> {
    /// Makes a material of a built-in shader and gives it the global wind.
    pub fn new_material(&mut self, preset: &str) -> Result<i32, String> {
        let m = self.preset_shader(preset)?;
        self.apply_wind(m)?;
        Ok(m)
    }

    /// Switches a material to another built-in shader, keeping the
    /// parameters both have (`SetShadingMode`).
    pub fn switch_preset(&mut self, m: i32, preset: &str) -> Result<(), String> {
        let src = crate::shaderlib::preset(preset).ok_or_else(|| format!("there is no built-in shader '{preset}'"))?;
        let o = self.shader_obj_mut(m)?;
        if o.preset.as_deref() == Some(preset) {
            return Ok(());
        }
        o.source = src;
        o.preset = Some(preset.to_string());
        o.name = preset.to_string();
        o.path = None;
        if !self.compile_shader(m)? {
            return Err(self.shader_obj(m)?.error.clone());
        }
        // values the new shader does not declare are kept for switching back
        Ok(())
    }

    /// The outline shader of a toon material (made on first use).
    fn outline_of(&mut self, m: i32) -> Result<i32, String> {
        let o = self.shader_obj(m)?.outline;
        if o != 0 && self.shader_obj(o).is_ok() {
            return Ok(o);
        }
        let o = self.preset_shader("outline")?;
        self.shader_obj_mut(o)?.enabled = false;
        self.shader_obj_mut(m)?.outline = o;
        Ok(o)
    }

    fn outline_target(&mut self, m: i32) -> Result<i32, String> {
        if self.shader_obj(m)?.preset.as_deref() == Some("outline") {
            Ok(m)
        } else {
            self.outline_of(m)
        }
    }

    /// Gives a material the global wind unless it set its own.
    fn apply_wind(&mut self, m: i32) -> Result<(), String> {
        let (dir, strength) = (self.world.wind_dir, self.world.wind_strength);
        let o = self.shader_obj(m)?;
        let has_dir = o.param("windDirection").is_some() && !o.values.contains_key("windDirection") || self.world.wind_owned.contains(&(m, 0));
        let has_str = o.param("windStrength").is_some() && !o.values.contains_key("windStrength") || self.world.wind_owned.contains(&(m, 1));
        if has_dir {
            self.shader_set(m, "windDirection", Val::Floats(vec![dir.0, dir.1]))?;
            self.world.wind_owned.insert((m, 0));
        }
        if has_str {
            self.shader_set(m, "windStrength", Val::Floats(vec![strength]))?;
            self.world.wind_owned.insert((m, 1));
        }
        Ok(())
    }

    fn apply_wind_all(&mut self) -> Result<(), String> {
        let n = self.world.shaders.len() as i32;
        for m in 1..=n {
            if self.shader_obj(m).is_ok() {
                self.apply_wind(m)?;
            }
        }
        Ok(())
    }

    /// The value of a float parameter (set or default).
    fn fparam(&self, m: i32, name: &str, n: usize) -> Vec<f32> {
        let o = match self.shader_obj(m) {
            Ok(o) => o,
            Err(_) => return vec![0.0; n],
        };
        let mut v = match o.values.get(name) {
            Some(Val::Floats(f)) => f.clone(),
            _ => o.param(name).map(|p| p.default.clone()).unwrap_or_default(),
        };
        if v.len() == 1 && n > 1 {
            v = vec![v[0]; n];
        }
        v.resize(n, 0.0);
        v
    }

    /// `BakeProceduralTexture`: the material's colour (or, with `normal`,
    /// a normal map of its noise) evaluated on the CPU.
    pub fn bake_procedural(&mut self, m: i32, w: u32, h: u32, normal: bool) -> Result<i32, String> {
        self.ensure_compiled(m)?;
        if self.shader_obj(m)?.param("patternType").is_none() {
            return Err(format!("Material {m} is not a procedural material (CreateProceduralShader)"));
        }
        let p = Proc {
            gradient: self.fparam(m, "gradientType", 1)[0] as i32,
            a: self.fparam(m, "gradientColorA", 4),
            b: self.fparam(m, "gradientColorB", 4),
            dir: self.fparam(m, "gradientDirection", 1)[0],
            noise: self.fparam(m, "noiseType", 1)[0] as i32,
            scale: self.fparam(m, "noiseScale", 1)[0],
            octaves: self.fparam(m, "noiseOctaves", 1)[0] as i32,
            persistence: self.fparam(m, "noisePersistence", 1)[0],
            lacunarity: self.fparam(m, "noiseLacunarity", 1)[0],
            strength: self.fparam(m, "noiseStrength", 1)[0],
            pattern: self.fparam(m, "patternType", 1)[0] as i32,
            pscale: self.fparam(m, "patternScale", 1)[0],
            pcolor: self.fparam(m, "patternColor", 4),
            prot: self.fparam(m, "patternRotation", 1)[0],
            distortion: self.fparam(m, "distortion", 1)[0],
            seed: self.fparam(m, "seed", 1)[0],
            time: self.world.shader_time * self.fparam(m, "noiseSpeed", 1)[0],
        };
        let mut t = crate::texture::TextureInfo::blank(w, h, 1 | 8)?;
        let px = t.pixels.as_mut().ok_or("no pixels")?;
        for y in 0..h {
            for x in 0..w {
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                let c = if normal {
                    let e = 1.0 / w.max(h) as f32;
                    let hx = p.noise_at([uv[0] - e, uv[1]]) - p.noise_at([uv[0] + e, uv[1]]);
                    let hy = p.noise_at([uv[0], uv[1] - e]) - p.noise_at([uv[0], uv[1] + e]);
                    let s = 4.0;
                    let n = [hx * s, hy * s, 2.0 * e * w as f32 / 64.0];
                    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
                    [n[0] / l * 0.5 + 0.5, n[1] / l * 0.5 + 0.5, n[2] / l * 0.5 + 0.5, 1.0]
                } else {
                    p.color(uv)
                };
                px.put_pixel(x, y, image::Rgba(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)));
            }
        }
        t.commit();
        self.world.textures.push(Some(t));
        Ok(self.world.textures.len() as i32)
    }
}

// ---- CPU versions of the library's procedural functions (shaderlib.rs)

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn hash12(p: [f32; 2]) -> f32 {
    let mut p3 = [fract(p[0] * 0.1031), fract(p[1] * 0.1031), fract(p[0] * 0.1031)];
    let d = p3[0] * (p3[1] + 33.33) + p3[1] * (p3[2] + 33.33) + p3[2] * (p3[0] + 33.33);
    p3 = [p3[0] + d, p3[1] + d, p3[2] + d];
    fract((p3[0] + p3[1]) * p3[2])
}

fn hash22(p: [f32; 2]) -> [f32; 2] {
    let mut p3 = [fract(p[0] * 0.1031), fract(p[1] * 0.1030), fract(p[0] * 0.0973)];
    let d = p3[0] * (p3[1] + 33.33) + p3[1] * (p3[2] + 33.33) + p3[2] * (p3[0] + 33.33);
    p3 = [p3[0] + d, p3[1] + d, p3[2] + d];
    [fract((p3[0] + p3[0]) * p3[2]), fract((p3[0] + p3[1]) * p3[1])]
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn value_noise(p: [f32; 2]) -> f32 {
    let (i, f) = ([p[0].floor(), p[1].floor()], [fract(p[0]), fract(p[1])]);
    let u = [f[0] * f[0] * (3.0 - 2.0 * f[0]), f[1] * f[1] * (3.0 - 2.0 * f[1])];
    let h = |dx: f32, dy: f32| hash12([i[0] + dx, i[1] + dy]);
    mix(mix(h(0.0, 0.0), h(1.0, 0.0), u[0]), mix(h(0.0, 1.0), h(1.0, 1.0), u[0]), u[1])
}

fn perlin(p: [f32; 2]) -> f32 {
    let (i, f) = ([p[0].floor(), p[1].floor()], [fract(p[0]), fract(p[1])]);
    let fade = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let u = [fade(f[0]), fade(f[1])];
    let g = |dx: f32, dy: f32| {
        let h = hash22([i[0] + dx, i[1] + dy]);
        (h[0] * 2.0 - 1.0) * (f[0] - dx) + (h[1] * 2.0 - 1.0) * (f[1] - dy)
    };
    mix(mix(g(0.0, 0.0), g(1.0, 0.0), u[0]), mix(g(0.0, 1.0), g(1.0, 1.0), u[0]), u[1]) * std::f32::consts::FRAC_1_SQRT_2 + 0.5
}

fn simplex(p: [f32; 2]) -> f32 {
    const K1: f32 = 0.366_025_42;
    const K2: f32 = 0.211_324_87;
    let s = (p[0] + p[1]) * K1;
    let i = [(p[0] + s).floor(), (p[1] + s).floor()];
    let t = (i[0] + i[1]) * K2;
    let a = [p[0] - i[0] + t, p[1] - i[1] + t];
    let m = if a[0] > a[1] { 1.0 } else { 0.0 };
    let o = [m, 1.0 - m];
    let b = [a[0] - o[0] + K2, a[1] - o[1] + K2];
    let c = [a[0] - 1.0 + 2.0 * K2, a[1] - 1.0 + 2.0 * K2];
    let corner = |d: [f32; 2], off: [f32; 2]| {
        let h = (0.5 - (d[0] * d[0] + d[1] * d[1])).max(0.0);
        let g = hash22([i[0] + off[0], i[1] + off[1]]);
        h * h * h * h * (d[0] * (g[0] * 2.0 - 1.0) + d[1] * (g[1] * 2.0 - 1.0))
    };
    (corner(a, [0.0, 0.0]) + corner(b, o) + corner(c, [1.0, 1.0])) * 70.0 * 0.5 + 0.5
}

fn voronoi(p: [f32; 2]) -> [f32; 3] {
    let (i, f) = ([p[0].floor(), p[1].floor()], [fract(p[0]), fract(p[1])]);
    let (mut d1, mut d2, mut id) = (8.0f32, 8.0f32, 0.0f32);
    for y in -1..=1 {
        for x in -1..=1 {
            let g = [x as f32, y as f32];
            let o = hash22([i[0] + g[0], i[1] + g[1]]);
            let v = [g[0] + o[0] - f[0], g[1] + o[1] - f[1]];
            let d = (v[0] * v[0] + v[1] * v[1]).sqrt();
            if d < d1 {
                d2 = d1;
                d1 = d;
                id = hash12([i[0] + g[0], i[1] + g[1]]);
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    [d1, d2, id]
}

fn noise(kind: i32, p: [f32; 2]) -> f32 {
    match kind {
        1 => perlin(p),
        2 => simplex(p),
        3 => voronoi(p)[0].clamp(0.0, 1.0),
        4 => {
            let v = voronoi(p);
            (v[1] - v[0]).clamp(0.0, 1.0)
        }
        _ => value_noise(p),
    }
}

struct Proc {
    gradient: i32,
    a: Vec<f32>,
    b: Vec<f32>,
    dir: f32,
    noise: i32,
    scale: f32,
    octaves: i32,
    persistence: f32,
    lacunarity: f32,
    strength: f32,
    pattern: i32,
    pscale: f32,
    pcolor: Vec<f32>,
    prot: f32,
    distortion: f32,
    seed: f32,
    time: f32,
}

fn rotate(p: [f32; 2], deg: f32) -> [f32; 2] {
    let (s, c) = deg.to_radians().sin_cos();
    [c * p[0] + s * p[1], -s * p[0] + c * p[1]]
}

impl Proc {
    fn noise_at(&self, uv: [f32; 2]) -> f32 {
        let mut p = [uv[0] * self.scale + self.seed * 13.17 + self.time, uv[1] * self.scale + self.seed * 7.31 + self.time];
        let (mut sum, mut amp, mut norm) = (0.0, 0.5, 0.0);
        for _ in 0..self.octaves.clamp(1, 12) {
            sum += noise(self.noise, p) * amp;
            norm += amp;
            p = [p[0] * self.lacunarity + 17.3, p[1] * self.lacunarity + 9.1];
            amp *= self.persistence;
        }
        if norm > 0.0 {
            sum / norm
        } else {
            0.0
        }
    }

    fn pattern_at(&self, uv: [f32; 2]) -> f32 {
        let q = rotate([uv[0] - 0.5, uv[1] - 0.5], self.prot);
        let p = [q[0] * self.pscale, q[1] * self.pscale];
        match self.pattern {
            1 => (p[0].floor() + p[1].floor()).rem_euclid(2.0),
            2 => {
                let gx = (fract(p[0] - 0.5) - 0.5).abs();
                let gy = (fract(p[1] - 0.5) - 0.5).abs();
                if gx.min(gy) < 0.03 {
                    1.0
                } else {
                    0.0
                }
            }
            3 => (fract(p[0]) < 0.5) as i32 as f32,
            4 => {
                let d = ((fract(p[0]) - 0.5).powi(2) + (fract(p[1]) - 0.5).powi(2)).sqrt();
                (d < 0.3) as i32 as f32
            }
            5 => voronoi([p[0] + self.seed, p[1] + self.seed])[2],
            6 => self.noise_at(uv),
            _ => 0.0,
        }
    }

    fn color(&self, uv0: [f32; 2]) -> [f32; 4] {
        let mut uv = uv0;
        if self.distortion != 0.0 {
            uv = [uv[0] + (self.noise_at(uv0) - 0.5) * self.distortion, uv[1] + (self.noise_at([uv0[0] + 3.7, uv0[1] + 3.7]) - 0.5) * self.distortion];
        }
        let g = match self.gradient {
            0 => {
                let (s, c) = self.dir.to_radians().sin_cos();
                ((uv[0] - 0.5) * c + (uv[1] - 0.5) * s + 0.5).clamp(0.0, 1.0)
            }
            1 => (((uv[0] - 0.5).powi(2) + (uv[1] - 0.5).powi(2)).sqrt() * 2.0).clamp(0.0, 1.0),
            2 => {
                let q = rotate([uv[0] - 0.5, uv[1] - 0.5], self.dir);
                q[1].atan2(q[0]) / std::f32::consts::TAU + 0.5
            }
            _ => 0.0,
        };
        let g = (g + (self.noise_at(uv) - 0.5) * self.strength).clamp(0.0, 1.0);
        let p = self.pattern_at(uv) * self.pcolor[3];
        std::array::from_fn(|k| mix(mix(self.a[k], self.b[k], g), self.pcolor[k], p))
    }
}

/// Material commands; see the module comment.
pub static COMMANDS: &[(&str, Cmd)] = &[
    // ---- PBR
    ("%CreatePBRMaterial", |h, _, _| int(h.new_material("pbr")?)),
    ("SetAlbedo%material#red#green#blue#alpha=255", |h, a, _| {
        mset(h, a[0].int(), "albedo", rgba(a, 1))?;
        VOID
    }),
    ("SetAlbedoMap%material%texture", |h, a, _| {
        mtex(h, a[0].int(), "albedoMap", a[1].int())?;
        VOID
    }),
    ("SetMetallic%material#metallic", |h, a, _| {
        mset(h, a[0].int(), "metallic", vec![a[1].float().clamp(0.0, 1.0)])?;
        VOID
    }),
    ("SetRoughness%material#roughness", |h, a, _| {
        mset(h, a[0].int(), "roughness", vec![a[1].float().clamp(0.0, 1.0)])?;
        VOID
    }),
    ("SetEmissive%material#red#green#blue#strength=1", |h, a, _| {
        let m = a[0].int();
        mset(h, m, "emissive", rgb(a, 1))?;
        mset(h, m, "emissiveStrength", vec![a[4].float()])?;
        VOID
    }),
    ("SetNormalMap%material%texture#strength=1", |h, a, _| {
        let m = a[0].int();
        mtex(h, m, "normalMap", a[1].int())?;
        if h.shader_obj(m)?.param("normalStrength").is_some() {
            mset(h, m, "normalStrength", vec![a[2].float()])?;
        }
        VOID
    }),
    ("SetMetallicMap%material%texture", |h, a, _| {
        mtex(h, a[0].int(), "metallicMap", a[1].int())?;
        VOID
    }),
    ("SetRoughnessMap%material%texture", |h, a, _| {
        mtex(h, a[0].int(), "roughnessMap", a[1].int())?;
        VOID
    }),
    ("SetAOMap%material%texture", |h, a, _| {
        mtex(h, a[0].int(), "aoMap", a[1].int())?;
        VOID
    }),
    ("SetEmissiveMap%material%texture", |h, a, _| {
        mtex(h, a[0].int(), "emissiveMap", a[1].int())?;
        VOID
    }),
    // mode: 0 opaque, 1 cut-out (alpha test, keeps shadows), 2 blended
    ("SetOpacityMap%material%texture%mode=1", |h, a, _| {
        let m = a[0].int();
        mtex(h, m, "opacityMap", a[1].int())?;
        let mode = a[2].int();
        if !(0..=2).contains(&mode) {
            return Err(format!("SetOpacityMap: mode must be 0-2, not {mode}"));
        }
        h.set_shader_keyword(m, "NB_CUTOUT", mode == 1)?;
        h.set_shader_keyword(m, "NB_TRANSPARENT", mode == 2)?;
        VOID
    }),
    ("SetOpacity%material#opacity%mode=2", |h, a, _| {
        let m = a[0].int();
        mset(h, m, "opacity", vec![a[1].float().clamp(0.0, 1.0)])?;
        let mode = a[2].int();
        h.set_shader_keyword(m, "NB_CUTOUT", mode == 1)?;
        h.set_shader_keyword(m, "NB_TRANSPARENT", mode == 2 && a[1].float() < 1.0)?;
        VOID
    }),
    ("SetHeightMap%material%texture#scale=0.05", |h, a, _| {
        let m = a[0].int();
        mtex(h, m, "heightMap", a[1].int())?;
        mset(h, m, "parallaxScale", vec![if a[1].int() == 0 { 0.0 } else { a[2].float() }])?;
        VOID
    }),
    ("SetEnvironmentMap%material%cubemap#intensity=1", |h, a, _| {
        let m = a[0].int();
        let t = a[1].int();
        h.ensure_compiled(m)?;
        h.shader_set(m, "envMap", Val::Texture(t))?;
        if h.shader_obj(m)?.param("reflection").is_some() {
            mset(h, m, "reflection", vec![if t == 0 { 0.0 } else { a[2].float() }])?;
        }
        VOID
    }),
    ("SetReflectionIntensity%material#intensity", |h, a, _| {
        mset(h, a[0].int(), "reflection", vec![a[1].float().max(0.0)])?;
        VOID
    }),
    ("SetFresnel%material#amount", |h, a, _| {
        mset(h, a[0].int(), "fresnel", vec![a[1].float().max(0.0)])?;
        VOID
    }),
    ("SetParallaxScale%material#scale", |h, a, _| {
        mset(h, a[0].int(), "parallaxScale", vec![a[1].float()])?;
        VOID
    }),
    // 0 metallic/roughness, 1 specular (specular colour from ShaderSetColor "specularColor")
    ("SetMaterialWorkflow%material%workflow#red=10#green=10#blue=10", |h, a, _| {
        let m = a[0].int();
        let w = a[1].int();
        if !(0..=1).contains(&w) {
            return Err(format!("SetMaterialWorkflow: workflow must be 0 (metallic) or 1 (specular), not {w}"));
        }
        mset(h, m, "workflow", vec![w as f32])?;
        if w == 1 {
            mset(h, m, "specularColor", rgb(a, 2))?;
        }
        VOID
    }),
    // ---- toon and stylized
    ("%CreateToonMaterial%mode=0", |h, a, _| int(h.new_material(shading_name(a[0].int())?)?)),
    ("SetToonRamp%material%texture", |h, a, _| {
        let m = a[0].int();
        let t = a[1].int();
        mtex(h, m, "toonRamp", t)?;
        mset(h, m, "useRamp", vec![(t != 0) as i32 as f32])?;
        VOID
    }),
    ("SetLightingRamp%material%texture", |h, a, _| {
        let m = a[0].int();
        let t = a[1].int();
        mtex(h, m, "toonRamp", t)?;
        mset(h, m, "useRamp", vec![(t != 0) as i32 as f32])?;
        VOID
    }),
    ("SetToonSteps%material#steps", |h, a, _| {
        mset(h, a[0].int(), "toonSteps", vec![a[1].float().max(1.0)])?;
        VOID
    }),
    ("SetToonOutline%material%enable#width=0.03", |h, a, _| {
        let m = a[0].int();
        let o = h.outline_of(m)?;
        mset(h, o, "outlineWidth", vec![a[2].float()])?;
        h.set_shader_enabled(o, a[1].int() != 0)?;
        // users get or lose their hulls
        let users: Vec<i32> = h.shader_obj(m)?.users.iter().copied().collect();
        for e in users {
            if h.ent(e).is_ok() {
                h.refresh_material(e)?;
            }
        }
        VOID
    }),
    ("SetOutlineColor%material#red#green#blue#alpha=255", |h, a, _| {
        let o = h.outline_target(a[0].int())?;
        mset(h, o, "outlineColor", rgba(a, 1))?;
        VOID
    }),
    ("SetOutlineWidth%material#width", |h, a, _| {
        let o = h.outline_target(a[0].int())?;
        mset(h, o, "outlineWidth", vec![a[1].float().max(0.0)])?;
        VOID
    }),
    ("SetRimLighting%material%enable#strength=0.6", |h, a, _| {
        mset(h, a[0].int(), "rimStrength", vec![if a[1].int() != 0 { a[2].float() } else { 0.0 }])?;
        VOID
    }),
    ("SetRimColor%material#red#green#blue", |h, a, _| {
        mset(h, a[0].int(), "rimColor", rgb(a, 1))?;
        VOID
    }),
    ("SetRimPower%material#power", |h, a, _| {
        mset(h, a[0].int(), "rimPower", vec![a[1].float().max(0.01)])?;
        VOID
    }),
    ("SetShadingMode%material%mode", |h, a, _| {
        let name = shading_name(a[1].int())?;
        h.switch_preset(a[0].int(), name)?;
        VOID
    }),
    ("SetVertexLighting%material%enable", |h, a, _| {
        h.switch_preset(a[0].int(), if a[1].int() != 0 { "vertexlit" } else { "toon" })?;
        VOID
    }),
    ("SetFlatShading%material%enable", |h, a, _| {
        h.switch_preset(a[0].int(), if a[1].int() != 0 { "flat" } else { "toon" })?;
        VOID
    }),
    ("SetPixelShading%material%enable", |h, a, _| {
        h.switch_preset(a[0].int(), if a[1].int() != 0 { "pixelart" } else { "toon" })?;
        VOID
    }),
    // ---- water, wind and foliage
    ("%CreateWaterMaterial", |h, _, _| int(h.new_material("water")?)),
    ("SetWaterWaves%material#height#length=6#dir_x=1#dir_z=0.3", |h, a, _| {
        let m = a[0].int();
        mset(h, m, "waveHeight", vec![a[1].float()])?;
        mset(h, m, "waveLength", vec![a[2].float().max(0.01)])?;
        mset(h, m, "waveDirection", vec![a[3].float(), a[4].float()])?;
        VOID
    }),
    ("SetWaterSpeed%material#speed", |h, a, _| {
        mset(h, a[0].int(), "waveSpeed", vec![a[1].float()])?;
        VOID
    }),
    ("SetWaterDepth%material#depth", |h, a, _| {
        mset(h, a[0].int(), "depthFade", vec![a[1].float().max(0.01)])?;
        VOID
    }),
    ("SetWaterReflection%material#amount%cubemap=0", |h, a, _| {
        let m = a[0].int();
        mset(h, m, "reflection", vec![a[1].float().max(0.0)])?;
        if a[2].int() != 0 {
            h.shader_set(m, "envMap", Val::Texture(a[2].int()))?;
        }
        VOID
    }),
    ("SetWaterRefraction%material#amount", |h, a, _| {
        mset(h, a[0].int(), "refraction", vec![a[1].float()])?;
        VOID
    }),
    ("SetWaterFoam%material#amount#red=255#green=255#blue=255", |h, a, _| {
        let m = a[0].int();
        mset(h, m, "foamAmount", vec![a[1].float().max(0.0)])?;
        mset(h, m, "foamColor", rgb(a, 2))?;
        VOID
    }),
    ("SetWaterColor%material#shallow_red#shallow_green#shallow_blue#deep_red#deep_green#deep_blue", |h, a, _| {
        let m = a[0].int();
        mset(h, m, "shallowColor", rgb(a, 1))?;
        mset(h, m, "deepColor", rgb(a, 4))?;
        VOID
    }),
    ("SetWindDirection#x#z", |h, a, _| {
        h.world.wind_dir = (a[0].float(), a[1].float());
        h.apply_wind_all()?;
        VOID
    }),
    ("SetWindStrength#strength", |h, a, _| {
        h.world.wind_strength = a[0].float();
        h.apply_wind_all()?;
        VOID
    }),
    ("%CreateFoliageMaterial", |h, _, _| int(h.new_material("wind")?)),
    ("SetFoliageWind%material#strength#speed=1.5#stiffness=1", |h, a, _| {
        let m = a[0].int();
        h.world.wind_owned.remove(&(m, 1));
        mset(h, m, "windStrength", vec![a[1].float()])?;
        mset(h, m, "windSpeed", vec![a[2].float()])?;
        mset(h, m, "stiffness", vec![a[3].float().max(0.01)])?;
        VOID
    }),
    // ---- procedural
    // kind: 0 gradient, 1 noise, 2 pattern
    ("%CreateProceduralShader%kind=0", |h, a, _| {
        let m = h.new_material("procedural")?;
        match a[0].int() {
            0 => {}
            1 => {
                mset(h, m, "gradientType", vec![3.0])?;
                mset(h, m, "patternType", vec![6.0])?;
                mset(h, m, "patternColor", vec![1.0, 1.0, 1.0, 1.0])?;
                mset(h, m, "gradientColorA", vec![0.0, 0.0, 0.0, 1.0])?;
            }
            2 => mset(h, m, "patternType", vec![1.0])?,
            k => return Err(format!("CreateProceduralShader: kind must be 0 (gradient), 1 (noise) or 2 (pattern), not {k}")),
        }
        int(m)
    }),
    ("SetGradient%material%type", |h, a, _| {
        let t = a[1].int();
        if !(0..=3).contains(&t) {
            return Err(format!("SetGradient: type must be GRADIENT_LINEAR, GRADIENT_RADIAL, GRADIENT_ANGULAR or GRADIENT_NONE, not {t}"));
        }
        mset(h, a[0].int(), "gradientType", vec![t as f32])?;
        VOID
    }),
    ("SetGradientColor%material%index#red#green#blue#alpha=255", |h, a, _| {
        let name = match a[1].int() {
            0 => "gradientColorA",
            1 => "gradientColorB",
            i => return Err(format!("SetGradientColor: index must be 0 or 1, not {i}")),
        };
        mset(h, a[0].int(), name, rgba(a, 2))?;
        VOID
    }),
    ("SetGradientDirection%material#angle", |h, a, _| {
        mset(h, a[0].int(), "gradientDirection", vec![a[1].float()])?;
        VOID
    }),
    ("SetNoiseType%material%type", |h, a, _| {
        let t = a[1].int();
        if !(0..=4).contains(&t) {
            return Err(format!("SetNoiseType: type must be 0-4 (NOISE_VALUE, NOISE_PERLIN, NOISE_SIMPLEX, NOISE_VORONOI, NOISE_CELLULAR), not {t}"));
        }
        mset(h, a[0].int(), "noiseType", vec![t as f32])?;
        VOID
    }),
    ("SetNoiseScale%material#scale", |h, a, _| {
        mset(h, a[0].int(), "noiseScale", vec![a[1].float()])?;
        VOID
    }),
    ("SetNoiseSpeed%material#speed", |h, a, _| {
        mset(h, a[0].int(), "noiseSpeed", vec![a[1].float()])?;
        VOID
    }),
    ("SetNoiseOctaves%material%octaves", |h, a, _| {
        mset(h, a[0].int(), "noiseOctaves", vec![a[1].int().clamp(1, 12) as f32])?;
        VOID
    }),
    ("SetNoisePersistence%material#persistence", |h, a, _| {
        mset(h, a[0].int(), "noisePersistence", vec![a[1].float()])?;
        VOID
    }),
    ("SetNoiseLacunarity%material#lacunarity", |h, a, _| {
        mset(h, a[0].int(), "noiseLacunarity", vec![a[1].float()])?;
        VOID
    }),
    ("SetNoiseStrength%material#strength", |h, a, _| {
        mset(h, a[0].int(), "noiseStrength", vec![a[1].float()])?;
        VOID
    }),
    ("SetPattern%material%type", |h, a, _| {
        let t = a[1].int();
        if !(0..=6).contains(&t) {
            return Err(format!("SetPattern: type must be 0-6 (PATTERN_NONE ... PATTERN_NOISE), not {t}"));
        }
        mset(h, a[0].int(), "patternType", vec![t as f32])?;
        VOID
    }),
    ("SetPatternScale%material#scale", |h, a, _| {
        mset(h, a[0].int(), "patternScale", vec![a[1].float()])?;
        VOID
    }),
    ("SetPatternColor%material#red#green#blue#alpha=255", |h, a, _| {
        mset(h, a[0].int(), "patternColor", rgba(a, 1))?;
        VOID
    }),
    ("SetPatternRotation%material#angle", |h, a, _| {
        mset(h, a[0].int(), "patternRotation", vec![a[1].float()])?;
        VOID
    }),
    ("SetDistortion%material#amount", |h, a, _| {
        mset(h, a[0].int(), "distortion", vec![a[1].float()])?;
        VOID
    }),
    ("SetProceduralSeed%material#seed", |h, a, _| {
        mset(h, a[0].int(), "seed", vec![a[1].float()])?;
        VOID
    }),
    ("%BakeProceduralTexture%material%width=256%height=256%normal_map=0", |h, a, _| {
        let (w, ht) = (a[1].int(), a[2].int());
        if !(1..=4096).contains(&w) || !(1..=4096).contains(&ht) {
            return Err(format!("BakeProceduralTexture: size {w}x{ht} is not allowed (1-4096)"));
        }
        int(h.bake_procedural(a[0].int(), w as u32, ht as u32, a[3].int() != 0)?)
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_noise_is_in_range_and_varies() {
        let mut lo: f32 = 1.0;
        let mut hi: f32 = 0.0;
        for k in 0..5 {
            for i in 0..200 {
                let v = noise(k, [i as f32 * 0.37, i as f32 * 0.11]);
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        assert!(lo >= -0.05 && hi <= 1.05 && hi - lo > 0.3, "{lo} {hi}");
        assert_eq!(shading_name(SHADING_PS1).unwrap(), "ps1");
        for n in SHADING {
            assert!(crate::shaderlib::preset(n).is_some(), "{n}");
        }
    }
}
