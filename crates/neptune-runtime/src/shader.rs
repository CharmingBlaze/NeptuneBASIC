//! Shaders (ext): the shader manager behind `LoadShader`, `CreateShader`,
//! `CompileShader`, the `ShaderSet*` parameters, `ShaderApply` /
//! `EntityShader`, hot reloading and the debugging commands.
//!
//! A shader object holds source (GLSL, WGSL or a Fyrox `.shader` file),
//! keywords (variants), parameter values and the last shader that compiled.
//! Compiling processes the source (`shadersrc.rs`, `wgsl.rs`), has Fyrox
//! parse it, and, once the window exists, builds the GPU programs of every
//! pass, so driver errors come back with source file and line. A failed
//! compile keeps the previous working shader.
//!
//! Compiled shaders are cached by their final text: two objects with the
//! same source and keywords (every `CreatePBRMaterial`, say) share one
//! Fyrox shader, so Fyrox builds one GPU pipeline for all of them. Entities
//! that use a shader without per-entity overrides share one material, so
//! changing a parameter is one write, not one per entity.
//!
//! `VertexData` gives the vertices of a surface up to four extra `vec4`
//! values, read in a shader at locations 8 to 11 (`vertexData0`-`3`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use fyrox::{
    asset::untyped::ResourceKind,
    core::{
        algebra::{Matrix3, Matrix4, Vector2, Vector3, Vector4},
        pool::Handle,
        uuid::Uuid,
    },
    graph::SceneGraph,
    material::{shader::{Shader, ShaderResource}, Material, MaterialProperty, MaterialResource},
    resource::texture::{Texture, TextureKind, TexturePixelKind, TextureResource},
    scene::{
        base::BaseBuilder,
        mesh::{surface::Surface, Mesh, MeshBuilder, RenderPath},
        node::Node,
    },
};
use neptune_vm::value::Value;

use crate::commands::Cmd;
use crate::shadersrc::{self, Lang, PassKind, Processed, Ty};
use crate::world::Host3D;

const VOID: Result<Value, String> = Ok(Value::Null);

/// Name of the inverted-hull child meshes made for toon outlines.
pub const OUTLINE_NODE: &str = "nb_outline";

#[derive(Debug, Clone)]
pub enum Prop {
    Float(f32),
    Vector([f32; 4]),
    Texture(i32),
}

/// An entity's use of a shader, with its own parameter overrides
/// (`ShaderFloat`, `ShaderVector`, `ShaderTexture`).
#[derive(Debug, Clone)]
pub struct CustomMaterial {
    pub shader: i32,
    pub props: BTreeMap<String, Prop>,
    /// The entity's own material when it has overrides.
    pub resource: Option<MaterialResource>,
}

/// Number of extra `vec4` values a vertex can carry.
pub const SLOTS: usize = 4;

/// A parameter value set by the program.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Floats(Vec<f32>),
    Texture(i32),
    /// Several textures stacked into a 3D texture (`ShaderSetTextureArray`).
    Textures(Vec<i32>),
    /// A texture made by the runtime (ramps, generated maps).
    Resource(TextureResource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    NotCompiled,
    /// Parsed and checked, waiting for the GPU (before `Graphics3D`).
    Validated,
    Compiled,
    Failed,
}

impl Status {
    pub fn code(self) -> i32 {
        match self {
            Status::NotCompiled => 0,
            Status::Compiled => 1,
            Status::Failed => 2,
            Status::Validated => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShaderObj {
    pub name: String,
    pub lang: Lang,
    pub path: Option<PathBuf>,
    /// Built-in shader this object was made from ("builtin:<name>").
    pub preset: Option<String>,
    pub source: String,
    pub vertex_part: Option<String>,
    pub fragment_part: Option<String>,
    pub keywords: BTreeSet<String>,
    pub processed: Option<Arc<Processed>>,
    pub resource: Option<ShaderResource>,
    pub key: u64,
    pub status: Status,
    pub error: String,
    pub log: Vec<String>,
    pub values: BTreeMap<String, Val>,
    pub enabled: bool,
    pub hot: bool,
    pub stamps: Vec<(PathBuf, Option<SystemTime>)>,
    pub material: Option<MaterialResource>,
    pub users: BTreeSet<i32>,
    pub manual_time: Option<f32>,
    pub manual_resolution: Option<(f32, f32)>,
    pub compiles: u32,
    pub last_compile_ms: f32,
    /// Toon outline: a shader drawn as an inverted hull around users.
    pub outline: i32,
    /// Parameters set by name before the shader declared them.
    pub pending: BTreeSet<String>,
    /// Built-in volume textures made for `ShaderSetTextureArray`.
    pub arrays: BTreeMap<String, TextureResource>,
}

impl ShaderObj {
    pub fn new(name: &str, lang: Lang, source: String) -> ShaderObj {
        ShaderObj {
            name: name.to_string(),
            lang,
            path: None,
            preset: None,
            source,
            vertex_part: None,
            fragment_part: None,
            keywords: BTreeSet::new(),
            processed: None,
            resource: None,
            key: 0,
            status: Status::NotCompiled,
            error: String::new(),
            log: Vec::new(),
            values: BTreeMap::new(),
            enabled: true,
            hot: false,
            stamps: Vec::new(),
            material: None,
            users: BTreeSet::new(),
            manual_time: None,
            manual_resolution: None,
            compiles: 0,
            last_compile_ms: 0.0,
            outline: 0,
            pending: BTreeSet::new(),
            arrays: BTreeMap::new(),
        }
    }

    /// The text that is compiled: the source plus any stage set on its own.
    pub fn effective_source(&self) -> String {
        if self.vertex_part.is_none() && self.fragment_part.is_none() {
            return self.source.clone();
        }
        let mut s = self.source.clone();
        if let Some(v) = &self.vertex_part {
            s.push_str("\n//! vertex\n");
            s.push_str(v);
        }
        if let Some(f) = &self.fragment_part {
            s.push_str("\n//! fragment\n");
            s.push_str(f);
        }
        s
    }

    pub fn param(&self, name: &str) -> Option<&shadersrc::Param> {
        self.processed.as_ref().and_then(|p| p.param(name))
    }

    pub fn is_post(&self) -> bool {
        self.processed.as_ref().is_some_and(|p| p.options.pass == PassKind::Post)
    }

    fn logln(&mut self, msg: String) {
        if self.log.len() > 200 {
            self.log.remove(0);
        }
        self.log.push(msg);
    }
}

/// Shared compile results, by the hash of the final shader text.
#[derive(Debug, Default)]
pub struct ShaderCache {
    pub entries: HashMap<u64, (ShaderResource, Arc<Processed>)>,
    pub hits: u64,
    pub compiles: u64,
}

fn hash_text(t: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

/// The material property for a value of a declared type.
pub fn property(ty: Ty, v: &[f32]) -> MaterialProperty {
    let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
    let g1 = |i: usize| v.get(i).copied().unwrap_or(if v.len() == 1 { v[0] } else { 0.0 });
    let chunks = |n: usize, max: u16| -> Vec<&[f32]> { v.chunks(n).take(max as usize).collect() };
    match ty {
        Ty::Float => MaterialProperty::Float(g(0)),
        Ty::Int => MaterialProperty::Int(g(0).round() as i32),
        Ty::UInt => MaterialProperty::UInt(g(0).max(0.0).round() as u32),
        Ty::Bool => MaterialProperty::Bool(g(0) != 0.0),
        Ty::Vec2 => MaterialProperty::Vector2(Vector2::new(g1(0), g1(1))),
        Ty::Vec3 => MaterialProperty::Vector3(Vector3::new(g1(0), g1(1), g1(2))),
        Ty::Vec4 => MaterialProperty::Vector4(Vector4::new(g1(0), g1(1), g1(2), g1(3))),
        Ty::Mat3 => MaterialProperty::Matrix3(if v.len() >= 9 { Matrix3::from_column_slice(&v[..9]) } else { Matrix3::identity() }),
        Ty::Mat4 => MaterialProperty::Matrix4(if v.len() >= 16 { Matrix4::from_column_slice(&v[..16]) } else { Matrix4::identity() }),
        Ty::FloatArray(m) => MaterialProperty::FloatArray(v.iter().take(m as usize).copied().collect()),
        Ty::IntArray(m) => MaterialProperty::IntArray(v.iter().take(m as usize).map(|x| x.round() as i32).collect()),
        Ty::Vec2Array(m) => MaterialProperty::Vector2Array(chunks(2, m).iter().map(|c| Vector2::new(c[0], *c.get(1).unwrap_or(&0.0))).collect()),
        Ty::Vec3Array(m) => MaterialProperty::Vector3Array(chunks(3, m).iter().map(|c| Vector3::from_iterator(c.iter().copied().chain(std::iter::repeat(0.0)).take(3))).collect()),
        Ty::Vec4Array(m) => MaterialProperty::Vector4Array(chunks(4, m).iter().map(|c| Vector4::from_iterator(c.iter().copied().chain(std::iter::repeat(0.0)).take(4))).collect()),
        Ty::Mat4Array(m) => MaterialProperty::Matrix4Array(chunks(16, m).iter().filter(|c| c.len() == 16).map(|c| Matrix4::from_column_slice(c)).collect()),
        _ => MaterialProperty::Float(0.0),
    }
}

/// Parses "1, 2.5, 3" (or spaces) into numbers.
pub fn parse_numbers(s: &str) -> Result<Vec<f32>, String> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| match p.to_ascii_lowercase().as_str() {
            "true" => Ok(1.0),
            "false" => Ok(0.0),
            _ => p.parse::<f32>().map_err(|_| format!("'{p}' is not a number")),
        })
        .collect()
}

fn read_source(path: &str) -> Result<(Lang, String, Option<String>), String> {
    if let Some(name) = path.strip_prefix("builtin:") {
        let src = crate::shaderlib::preset(name).ok_or_else(|| format!("there is no built-in shader '{name}' (see the Shader commands in docs/COMMANDS.md)"))?;
        return Ok((Lang::Glsl, src, Some(name.to_ascii_lowercase())));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
    Ok((Lang::from_path(path), text, None))
}

fn stamp(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl Host3D<'_> {
    pub fn shader_obj(&self, s: i32) -> Result<&ShaderObj, String> {
        usize::try_from(s - 1)
            .ok()
            .and_then(|i| self.world.shaders.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Shader {s} does not exist"))
    }

    pub fn shader_obj_mut(&mut self, s: i32) -> Result<&mut ShaderObj, String> {
        usize::try_from(s - 1)
            .ok()
            .and_then(|i| self.world.shaders.get_mut(i))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("Shader {s} does not exist"))
    }

    pub fn add_shader(&mut self, obj: ShaderObj) -> i32 {
        if let Some(i) = self.world.shaders.iter().position(Option::is_none) {
            self.world.shaders[i] = Some(obj);
            return i as i32 + 1;
        }
        self.world.shaders.push(Some(obj));
        self.world.shaders.len() as i32
    }

    /// `LoadShader`: a file (`.glsl`, `.wgsl`, `.shader`) or `builtin:<name>`.
    pub fn load_shader(&mut self, path: &str) -> Result<i32, String> {
        let (lang, text, preset) = read_source(path)?;
        let mut obj = ShaderObj::new(preset.as_deref().unwrap_or(path), lang, text);
        obj.preset = preset;
        if obj.preset.is_none() {
            obj.path = Some(PathBuf::from(path));
        }
        let s = self.add_shader(obj);
        if !self.compile_shader(s)? {
            eprintln!("LoadShader: {}", self.shader_obj(s)?.error);
        }
        Ok(s)
    }

    /// A new object from a built-in shader, compiled (shared via the cache).
    pub fn preset_shader(&mut self, name: &str) -> Result<i32, String> {
        let s = self.load_shader(&format!("builtin:{name}"))?;
        let o = self.shader_obj(s)?;
        if o.resource.is_none() {
            return Err(format!("built-in shader '{name}' failed to compile: {}", o.error));
        }
        Ok(s)
    }

    /// Processes the shader's source (no GPU work).
    fn process_obj(&self, o: &ShaderObj) -> Result<Processed, String> {
        let src = o.effective_source();
        let file_name = o.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| o.preset.as_ref().map(|p| format!("builtin:{p}")).unwrap_or_else(|| "<source>".into()));
        let input = shadersrc::Input { source: &src, file_name: &file_name, path: o.path.as_deref(), keywords: &o.keywords, includes: &shadersrc::DiskIncludes };
        match o.lang {
            Lang::Glsl => shadersrc::process(&input),
            Lang::Wgsl => crate::wgsl::process(&input),
            Lang::Fyrox => {
                let def = Shader::from_string(&src).map_err(|e| format!("{file_name}: {e}"))?.definition;
                let params = shadersrc::params_of_definition(&def);
                let passes = def
                    .passes
                    .iter()
                    .filter_map(|p| {
                        let name: &'static str = match p.name.as_str() {
                            "GBuffer" => "GBuffer",
                            "Forward" => "Forward",
                            "DirectionalShadow" => "DirectionalShadow",
                            "SpotShadow" => "SpotShadow",
                            "PointShadow" => "PointShadow",
                            "Primary" => "Primary",
                            _ => return None,
                        };
                        Some(shadersrc::PassSource { name, vertex: p.vertex_shader.0.clone(), fragment: p.fragment_shader.0.clone(), vertex_map: Vec::new(), fragment_map: Vec::new() })
                    })
                    .collect::<Vec<_>>();
                let deferred = passes.iter().any(|p| p.name == "GBuffer") && !def.disabled_passes.iter().any(|d| d == "GBuffer");
                let mut options = shadersrc::Options::default();
                if deferred {
                    options.pass = PassKind::GBuffer;
                } else if passes.iter().any(|p| p.name == "Primary") {
                    options.pass = PassKind::Post;
                }
                options.shadows = passes.iter().any(|p| p.name.ends_with("Shadow"));
                let attributes = crate::shaderlib::ATTRIBUTES.iter().filter(|(n, _)| src.contains(n)).map(|(n, l)| (n.to_string(), *l)).collect();
                let builtins = def.resources.iter().filter_map(|r| ["fyrox_instanceData", "fyrox_boneMatrices", "fyrox_cameraData", "fyrox_lightData", "fyrox_lightsBlock", "fyrox_graphicsSettings", "fyrox_sceneDepth"].iter().find(|b| **b == r.name.as_str()).copied()).collect();
                Ok(Processed {
                    options,
                    params,
                    builtins,
                    attributes,
                    passes,
                    ron: src.clone(),
                    files: vec!["<generated>".into(), file_name.clone()],
                    deps: o.path.iter().cloned().collect(),
                    warnings: Vec::new(),
                })
            }
        }
    }

    /// Builds GPU programs for every pass (needs the window); errors are
    /// mapped back to source lines.
    fn gpu_check(&self, shader: &Shader, p: &Processed) -> Result<(), String> {
        let Some(server) = self.gpu else { return Ok(()) };
        match fyrox::renderer::cache::shader::RenderPassContainer::new(server, shader) {
            Ok(_) => Ok(()),
            Err(e) => Err(match e {
                fyrox::graphics::error::FrameworkError::ShaderCompilationFailed { shader_name, error_message } => p.map_error(&shader_name, &error_message),
                // RenderPassContainer wraps the driver's message
                fyrox::graphics::error::FrameworkError::Custom(msg) if msg.contains("Compilation of \"") => {
                    let after = &msg[msg.find("Compilation of \"").unwrap_or(0) + 16..];
                    match after.split_once("\" shader has failed: ") {
                        Some((program, message)) => p.map_error(program, message),
                        None => msg,
                    }
                }
                fyrox::graphics::error::FrameworkError::ShaderLinkingFailed { shader_name, error_message } => format!("linking {shader_name} failed: {}", error_message.trim()),
                other => other.to_string(),
            }),
        }
    }

    /// Processes, validates and (with a window) GPU-compiles; returns the
    /// result without changing the object.
    fn build_shader(&mut self, s: i32) -> Result<(u64, ShaderResource, Arc<Processed>, bool), String> {
        let o = self.shader_obj(s)?;
        let processed = self.process_obj(o)?;
        let key = hash_text(&processed.ron);
        if let Some((res, p)) = self.world.shader_cache.entries.get(&key) {
            let (res, p) = (res.clone(), p.clone());
            self.world.shader_cache.hits += 1;
            return Ok((key, res, p, true));
        }
        let shader = Shader::from_string(&processed.ron).map_err(|e| format!("Fyrox rejected the shader: {e}"))?;
        self.gpu_check(&shader, &processed)?;
        let res = ShaderResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, shader);
        let p = Arc::new(processed);
        self.world.shader_cache.compiles += 1;
        self.world.shader_cache.entries.insert(key, (res.clone(), p.clone()));
        Ok((key, res, p, false))
    }

    /// `CompileShader`: true on success. On failure the previous working
    /// shader stays in use and `ShaderError` tells why.
    pub fn compile_shader(&mut self, s: i32) -> Result<bool, String> {
        let t0 = Instant::now();
        let r = self.build_shader(s);
        let gpu = self.gpu.is_some();
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        let o = self.shader_obj_mut(s)?;
        o.compiles += 1;
        o.last_compile_ms = ms;
        o.stamps = Vec::new();
        match r {
            Ok((key, res, p, cached)) => {
                o.stamps = p.deps.iter().map(|d| (d.clone(), stamp(d))).collect();
                for w in &p.warnings {
                    o.logln(format!("warning: {w}"));
                }
                o.logln(format!("compiled{} in {ms:.1} ms ({} passes){}", if cached { " (cached)" } else { "" }, p.passes.len(), if gpu { "" } else { "; GPU check when the window opens" }));
                let changed = o.key != key;
                o.key = key;
                o.processed = Some(p);
                o.resource = Some(res);
                o.status = if gpu || cached && o.status == Status::Compiled { Status::Compiled } else { Status::Validated };
                if cached && o.status == Status::Validated && gpu {
                    o.status = Status::Compiled;
                }
                o.error.clear();
                if changed {
                    o.material = None;
                    self.refresh_shader_users(s)?;
                }
                Ok(true)
            }
            Err(e) => {
                // keep the last shader that worked
                if o.path.is_some() {
                    o.stamps = o.processed.as_ref().map(|p| p.deps.iter().map(|d| (d.clone(), stamp(d))).collect()).unwrap_or_default();
                    if let Some(path) = &o.path {
                        if o.stamps.is_empty() {
                            o.stamps.push((path.clone(), stamp(path)));
                        }
                    }
                }
                o.logln(format!("error: {e}"));
                o.error = e;
                o.status = Status::Failed;
                Ok(false)
            }
        }
    }

    /// `ValidateShader`: checks the current source without using it.
    pub fn validate_shader(&mut self, s: i32) -> Result<bool, String> {
        let r = self.build_shader(s);
        let o = self.shader_obj_mut(s)?;
        Ok(match r {
            Ok(_) => {
                o.logln("validated".into());
                o.error.clear();
                true
            }
            Err(e) => {
                o.logln(format!("error: {e}"));
                o.error = e;
                false
            }
        })
    }

    /// `ReloadShader`: reads the file again and compiles.
    pub fn reload_shader(&mut self, s: i32) -> Result<bool, String> {
        let o = self.shader_obj(s)?;
        let Some(path) = o.path.clone() else { return self.compile_shader(s) };
        match std::fs::read_to_string(&path) {
            Ok(text) => self.shader_obj_mut(s)?.source = text,
            Err(e) => {
                let o = self.shader_obj_mut(s)?;
                o.error = format!("cannot read '{}': {e}", path.display());
                o.logln(format!("error: {}", o.error));
                o.status = Status::Failed;
                return Ok(false);
            }
        }
        self.compile_shader(s)
    }

    /// Makes sure a shader has been compiled once.
    pub fn ensure_compiled(&mut self, s: i32) -> Result<(), String> {
        if self.shader_obj(s)?.status == Status::NotCompiled {
            self.compile_shader(s)?;
        }
        Ok(())
    }

    pub fn free_shader(&mut self, s: i32) -> Result<(), String> {
        let users: Vec<i32> = self.shader_obj(s)?.users.iter().copied().collect();
        let outline = self.shader_obj(s)?.outline;
        self.world.shaders[(s - 1) as usize] = None;
        self.world.postfx.forget_shader(s);
        for e in users {
            if let Some(ent) = usize::try_from(e - 1).ok().and_then(|i| self.world.entities.get_mut(i)).and_then(Option::as_mut) {
                if ent.shader.as_ref().is_some_and(|c| c.shader == s) {
                    ent.shader = None;
                    let _ = self.refresh_material(e);
                }
            }
        }
        if outline != 0 && self.shader_obj(outline).is_ok() {
            self.free_shader(outline)?;
        }
        self.purge_shader_cache();
        Ok(())
    }

    /// Drops compiled shaders no object uses any more.
    fn purge_shader_cache(&mut self) {
        let live: BTreeSet<u64> = self.world.shaders.iter().flatten().map(|o| o.key).collect();
        self.world.shader_cache.entries.retain(|k, _| live.contains(k));
    }

    pub fn clone_shader(&mut self, s: i32) -> Result<i32, String> {
        let mut o = self.shader_obj(s)?.clone();
        o.users.clear();
        o.material = None;
        o.outline = 0;
        let outline = self.shader_obj(s)?.outline;
        let n = self.add_shader(o);
        if outline != 0 {
            let c = self.clone_shader(outline)?;
            self.shader_obj_mut(n)?.outline = c;
        }
        Ok(n)
    }

    /// `ShaderSave`: the Fyrox shader (`.shader`/`.ron`) or the source.
    pub fn save_shader(&mut self, s: i32, file: &str) -> Result<bool, String> {
        self.ensure_compiled(s)?;
        let o = self.shader_obj(s)?;
        let lower = file.to_ascii_lowercase();
        let text = if lower.ends_with(".shader") || lower.ends_with(".ron") {
            match &o.processed {
                Some(p) => p.ron.clone(),
                None => return Err(format!("Shader {s} has not compiled: {}", o.error)),
            }
        } else {
            o.effective_source()
        };
        Ok(std::fs::write(file, text).is_ok())
    }

    pub fn set_shader_keyword(&mut self, s: i32, keyword: &str, on: bool) -> Result<(), String> {
        let k = keyword.trim().to_string();
        if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("'{keyword}' is not a valid keyword name"));
        }
        let o = self.shader_obj_mut(s)?;
        let changed = if on { o.keywords.insert(k) } else { o.keywords.remove(&k) };
        if changed && o.status != Status::NotCompiled {
            self.compile_shader(s)?;
        }
        Ok(())
    }

    // ---- parameters

    /// Sets a parameter by name; checks it against the declaration.
    pub fn shader_set(&mut self, s: i32, name: &str, v: Val) -> Result<(), String> {
        self.ensure_compiled(s)?;
        let o = self.shader_obj(s)?;
        match o.param(name) {
            Some(p) => match (&v, p.ty.is_texture()) {
                (Val::Floats(_), true) => return Err(format!("parameter '{name}' of shader {s} is a {}; use ShaderSetTexture", p.ty.name())),
                (Val::Texture(_) | Val::Textures(_) | Val::Resource(_), false) => return Err(format!("parameter '{name}' of shader {s} is a {}, not a texture", p.ty.name())),
                (Val::Texture(t), true) if *t != 0 => {
                    let cube = matches!(self.texture(*t)?.resource.data_ref().kind(), TextureKind::Cube { .. });
                    if cube != (p.ty == Ty::SamplerCube) {
                        return Err(format!("parameter '{name}' is a {} but texture {t} is {}a cube map", p.ty.name(), if cube { "" } else { "not " }));
                    }
                }
                _ => {}
            },
            None if o.resource.is_some() => return Err(format!("Shader {s} has no parameter '{name}' (see ShaderGetUniforms)")),
            None => {}
        }
        if let Val::Textures(list) = &v {
            let r = self.texture_array(list)?;
            self.shader_obj_mut(s)?.arrays.insert(name.to_string(), r);
        }
        let o = self.shader_obj_mut(s)?;
        if o.param(name).is_none() {
            o.pending.insert(name.to_string());
        }
        o.values.insert(name.to_string(), v);
        self.push_param(s, name)
    }

    /// The value programs see for a parameter (set value or default).
    pub fn shader_get(&self, s: i32, name: &str) -> Result<String, String> {
        let o = self.shader_obj(s)?;
        let nums = |v: &[f32]| v.iter().map(|x| format!("{x}")).collect::<Vec<_>>().join(",");
        if let Some(v) = o.values.get(name) {
            return Ok(match v {
                Val::Floats(f) => nums(f),
                Val::Texture(t) => t.to_string(),
                Val::Textures(t) => t.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","),
                Val::Resource(_) => "texture".into(),
            });
        }
        match o.param(name) {
            Some(p) if p.ty.is_texture() => Ok("0".into()),
            Some(p) => Ok(nums(&p.default)),
            None => Err(format!("Shader {s} has no parameter '{name}'")),
        }
    }

    fn texture_array(&self, list: &[i32]) -> Result<TextureResource, String> {
        if list.is_empty() {
            return Err("ShaderSetTextureArray needs at least one texture".into());
        }
        let mut bytes = Vec::new();
        let mut size = None;
        for t in list {
            let info = self.texture(*t)?;
            let data = info.resource.data_ref();
            let (w, h) = match data.kind() {
                TextureKind::Rectangle { width, height } => (width, height),
                _ => return Err(format!("texture {t} is not a 2D texture")),
            };
            if *size.get_or_insert((w, h)) != (w, h) {
                return Err("all textures of a texture array must have the same size".into());
            }
            if data.pixel_kind() != TexturePixelKind::RGBA8 {
                return Err(format!("texture {t} is not 8-bit RGBA"));
            }
            bytes.extend_from_slice(&data.data()[..(w * h * 4) as usize]);
        }
        let (w, h) = size.unwrap_or((1, 1));
        let tex = Texture::from_bytes(TextureKind::Volume { width: w, height: h, depth: list.len() as u32 }, TexturePixelKind::RGBA8, bytes).ok_or("cannot make the texture array")?;
        Ok(TextureResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, tex))
    }

    fn resolve_texture(&self, o: &ShaderObj, name: &str, v: &Val) -> Option<TextureResource> {
        match v {
            Val::Texture(0) => None,
            Val::Texture(t) => self.texture(*t).ok().map(|t| t.resource.clone()),
            Val::Textures(_) => o.arrays.get(name).cloned(),
            Val::Resource(r) => Some(r.clone()),
            Val::Floats(_) => None,
        }
    }

    /// Writes one parameter into a material.
    fn write_param(&self, o: &ShaderObj, m: &mut Material, name: &str, v: &Val) {
        let Some(p) = o.param(name) else { return };
        if p.ty.is_texture() {
            if let Some(t) = self.resolve_texture(o, name, v) {
                m.bind(name, t);
            }
        } else if let Val::Floats(f) = v {
            m.set_property(name, property(p.ty, f));
        }
    }

    /// Builds a material with the shader's parameter values.
    pub fn shader_material(&self, s: i32, extra: &BTreeMap<String, Prop>) -> Result<Option<MaterialResource>, String> {
        let o = self.shader_obj(s)?;
        let Some(res) = o.resource.clone() else { return Ok(None) };
        let mut m = Material::from_shader(res);
        for (name, v) in &o.values {
            self.write_param(o, &mut m, name, v);
        }
        self.write_auto(o, &mut m);
        for (name, p) in extra {
            let v = match p {
                Prop::Float(f) => Val::Floats(vec![*f]),
                Prop::Vector(v) => Val::Floats(v.to_vec()),
                Prop::Texture(t) => Val::Texture(*t),
            };
            if o.param(name).is_some() {
                self.write_param(o, &mut m, name, &v);
            } else {
                // Fyrox .shader files and older programs: write it anyway
                match p {
                    Prop::Float(f) => m.set_property(name.as_str(), *f),
                    Prop::Vector(v) => m.set_property(name.as_str(), Vector4::new(v[0], v[1], v[2], v[3])),
                    Prop::Texture(t) => {
                        if let Ok(t) = self.texture(*t) {
                            m.bind(name.as_str(), t.resource.clone());
                        }
                    }
                }
            }
        }
        Ok(Some(MaterialResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, m)))
    }

    /// The material shared by entities without overrides.
    pub fn shared_material(&mut self, s: i32) -> Result<Option<MaterialResource>, String> {
        if let Some(m) = &self.shader_obj(s)?.material {
            return Ok(Some(m.clone()));
        }
        let m = self.shader_material(s, &BTreeMap::new())?;
        self.shader_obj_mut(s)?.material = m.clone();
        Ok(m)
    }

    /// Materials currently drawn with shader `s`.
    fn shader_materials(&self, s: i32) -> Result<Vec<MaterialResource>, String> {
        let o = self.shader_obj(s)?;
        let mut out: Vec<MaterialResource> = o.material.iter().cloned().collect();
        for e in &o.users {
            if let Some(cm) = self.ent_any(*e).and_then(|e| e.shader.as_deref()) {
                if cm.shader == s {
                    out.extend(cm.resource.iter().cloned());
                }
            }
        }
        Ok(out)
    }

    /// Updates a changed parameter in every material of the shader.
    fn push_param(&mut self, s: i32, name: &str) -> Result<(), String> {
        let o = self.shader_obj(s)?;
        let Some(v) = o.values.get(name).cloned() else { return Ok(()) };
        for m in self.shader_materials(s)? {
            let o = self.shader_obj(s)?;
            // entity overrides win
            let mut data = m.data_ref();
            self.write_param(o, &mut data, name, &v);
        }
        // the outline shader follows the material's outline settings
        Ok(())
    }

    /// Fills the parameters the runtime provides (`nb_time`, ...).
    fn write_auto(&self, o: &ShaderObj, m: &mut Material) {
        let Some(p) = &o.processed else { return };
        for (name, v) in self.auto_values(o) {
            if let Some(par) = p.param(name) {
                if !o.values.contains_key(name) {
                    m.set_property(name, property(par.ty, &v));
                }
            }
        }
    }

    fn auto_values(&self, o: &ShaderObj) -> Vec<(&'static str, Vec<f32>)> {
        let w = &self.world;
        let res = o.manual_resolution.unwrap_or((w.width as f32 * w.dpi, w.height as f32 * w.dpi));
        let fog = &w.classic;
        let fog_on = fog.fog_mode != 0;
        vec![
            ("nb_time", vec![o.manual_time.unwrap_or(w.shader_time)]),
            ("nb_deltaTime", vec![w.update_dt]),
            ("nb_resolution", vec![res.0, res.1]),
            ("nb_ambient", vec![w.ambient.r as f32 / 255.0, w.ambient.g as f32 / 255.0, w.ambient.b as f32 / 255.0]),
            ("nb_fogColor", vec![fog.fog_color.r as f32 / 255.0, fog.fog_color.g as f32 / 255.0, fog.fog_color.b as f32 / 255.0]),
            ("nb_fogRange", if fog_on { vec![fog.fog_range.0, fog.fog_range.1] } else { vec![0.0, 0.0] }),
        ]
    }

    /// Every frame: automatic parameters, hot reloading, and the GPU check
    /// of shaders compiled before the window existed.
    pub fn update_shaders(&mut self) -> Result<(), String> {
        self.world.shader_time += self.world.update_dt;
        let n = self.world.shaders.len() as i32;
        let now = Instant::now();
        let check_files = self.world.shader_poll.is_none_or(|t| now.duration_since(t).as_secs_f32() > 0.25);
        if check_files {
            self.world.shader_poll = Some(now);
        }
        for s in 1..=n {
            let Ok(o) = self.shader_obj(s) else { continue };
            // hot reloading
            if o.hot && check_files && o.stamps.iter().any(|(p, t)| stamp(p) != *t) {
                // remember what was seen, so a missing or broken file is
                // reported once, not every check
                let seen: Vec<(PathBuf, Option<SystemTime>)> = o.stamps.iter().map(|(p, _)| (p.clone(), stamp(p))).collect();
                let ok = self.reload_shader(s)?;
                if !ok {
                    self.shader_obj_mut(s)?.stamps = seen;
                }
                let o = self.shader_obj_mut(s)?;
                let msg = if ok { "hot reload: recompiled".to_string() } else { format!("hot reload failed, keeping the previous shader: {}", o.error) };
                o.logln(msg.clone());
                eprintln!("Shader {s} ({}): {msg}", o.name);
                continue;
            }
            // shaders compiled before Graphics3D get their GPU check now
            if o.status == Status::Validated && self.gpu.is_some() {
                let Some(res) = o.resource.clone() else { continue };
                let Some(p) = o.processed.clone() else { continue };
                let r = {
                    let shader = res.data_ref();
                    self.gpu_check(&shader, &p)
                };
                let o = self.shader_obj_mut(s)?;
                match r {
                    Ok(()) => o.status = Status::Compiled,
                    Err(e) => {
                        eprintln!("Shader {s} ({}): {e}", o.name);
                        o.logln(format!("error: {e}"));
                        o.error = e;
                        o.status = Status::Failed;
                    }
                }
            }
            let o = self.shader_obj(s)?;
            let Some(p) = o.processed.clone() else { continue };
            let auto: Vec<(&str, Vec<f32>)> = self.auto_values(o).into_iter().filter(|(n, _)| p.param(n).is_some() && !o.values.contains_key(*n)).collect();
            if auto.is_empty() {
                continue;
            }
            for m in self.shader_materials(s)? {
                let mut data = m.data_ref();
                for (name, v) in &auto {
                    if let Some(par) = p.param(name) {
                        data.set_property(*name, property(par.ty, v));
                    }
                }
            }
        }
        Ok(())
    }

    /// Gives the users of a recompiled shader its new materials.
    fn refresh_shader_users(&mut self, s: i32) -> Result<(), String> {
        let users: Vec<i32> = self.shader_obj(s)?.users.iter().copied().collect();
        for e in users {
            let uses = self.ent_any(e).and_then(|e| e.shader.as_deref()).is_some_and(|c| c.shader == s);
            let here = self.ent_any(e).is_some_and(|en| en.scene == self.world.current_scene);
            if uses && here {
                if let Some(cm) = self.ent_mut(e)?.shader.as_mut() {
                    cm.resource = None;
                }
                self.refresh_material(e)?;
            } else if !uses {
                self.shader_obj_mut(s)?.users.remove(&e);
            }
        }
        // shaders that use this one as their outline
        let owners: Vec<i32> = self.world.shaders.iter().enumerate().filter_map(|(i, o)| o.as_ref().filter(|o| o.outline == s).map(|_| i as i32 + 1)).collect();
        for o in owners {
            self.refresh_shader_users(o)?;
        }
        Ok(())
    }

    pub fn set_shader_enabled(&mut self, s: i32, on: bool) -> Result<(), String> {
        let o = self.shader_obj_mut(s)?;
        if o.enabled == on {
            return Ok(());
        }
        o.enabled = on;
        self.refresh_shader_users(s)
    }

    // ---- entities

    /// `EntityShader` / `ShaderApply`: draws the entity with a shader
    /// (0 = back to the normal materials).
    pub fn entity_shader(&mut self, e: i32, s: i32) -> Result<(), String> {
        if s != 0 {
            self.shader_obj(s)?;
            if self.shader_obj(s)?.is_post() {
                return Err(format!("Shader {s} is a post-processing shader; use AddPostProcess"));
            }
            self.ensure_compiled(s)?;
        }
        let old = self.ent(e)?.shader.as_ref().map(|c| c.shader);
        if let Some(old) = old.filter(|o| *o != s) {
            if let Ok(o) = self.shader_obj_mut(old) {
                o.users.remove(&e);
            }
        }
        let ent = self.ent_mut(e)?;
        ent.shader = (s != 0).then(|| Box::new(CustomMaterial { shader: s, props: BTreeMap::new(), resource: None }));
        if s != 0 {
            self.shader_obj_mut(s)?.users.insert(e);
        }
        // the materials that were replaced must not be reused
        self.refresh_material(e)?;
        if s == 0 || old.is_some() && old != Some(s) {
            self.remove_outline(e)?;
        }
        Ok(())
    }

    /// `ShaderFloat` / `ShaderVector` / `ShaderTexture`: an override for one
    /// entity.
    pub fn set_shader_prop(&mut self, e: i32, name: &str, p: Prop) -> Result<(), String> {
        let m = self.ent_mut(e)?.shader.as_mut().ok_or_else(|| format!("Entity {e} has no shader (use EntityShader)"))?;
        m.props.insert(name.to_string(), p.clone());
        let Some(res) = m.resource.clone() else { return self.refresh_material(e) };
        let s = m.shader;
        let o = self.shader_obj(s)?;
        let v = match &p {
            Prop::Float(f) => Val::Floats(vec![*f]),
            Prop::Vector(v) => Val::Floats(v.to_vec()),
            Prop::Texture(t) => Val::Texture(*t),
        };
        let mut mat = res.data_ref();
        if o.param(name).is_some() {
            self.write_param(o, &mut mat, name, &v);
        } else {
            match p {
                Prop::Float(v) => mat.set_property(name, v),
                Prop::Vector(v) => mat.set_property(name, Vector4::new(v[0], v[1], v[2], v[3])),
                Prop::Texture(t) => mat.bind(name, self.texture(t)?.resource.clone()),
            }
        }
        Ok(())
    }

    /// Gives an entity with a custom shader its material. False if the
    /// entity has none (or the shader is disabled or has never compiled).
    pub fn refresh_custom(&mut self, id: i32) -> Result<bool, String> {
        let Some(cm) = self.ent(id)?.shader.as_deref().cloned() else { return Ok(false) };
        let Ok(o) = self.shader_obj(cm.shader) else {
            self.ent_mut(id)?.shader = None;
            return Ok(false);
        };
        if !o.enabled || o.resource.is_none() {
            self.remove_outline(id)?;
            return Ok(false);
        }
        let deferred = o.processed.as_ref().is_some_and(|p| p.options.pass == PassKind::GBuffer);
        let outline = o.outline;
        let material = if cm.props.is_empty() {
            self.shared_material(cm.shader)?
        } else {
            match cm.resource.clone() {
                Some(r) => Some(r),
                None => self.shader_material(cm.shader, &cm.props)?,
            }
        };
        let Some(material) = material else { return Ok(false) };
        if let Some(c) = self.ent_mut(id)?.shader.as_mut() {
            c.resource = (!cm.props.is_empty()).then(|| material.clone());
        }
        let nodes = self.entity_mesh_nodes(id)?;
        let graph = self.graph()?;
        for n in &nodes {
            if let Some(mesh) = graph[*n].cast_mut::<Mesh>() {
                for s in mesh.surfaces_mut() {
                    s.set_material(material.clone());
                }
                mesh.set_render_path(if deferred { RenderPath::Deferred } else { RenderPath::Forward });
            }
        }
        // a later normal material must not reuse these
        self.ent_mut(id)?.own_materials = true;
        if outline != 0 && self.shader_obj(outline).is_ok_and(|o| o.enabled) {
            self.add_outline(id, outline, &nodes)?;
        } else {
            self.remove_outline(id)?;
        }
        Ok(true)
    }

    /// Removes the toon-outline hulls of an entity.
    pub fn remove_outline(&mut self, id: i32) -> Result<(), String> {
        let Ok(node) = self.ent(id).map(|e| e.node) else { return Ok(()) };
        let graph = self.graph()?;
        let mut stack = vec![node];
        let mut doomed = Vec::new();
        while let Some(n) = stack.pop() {
            for c in graph[n].children() {
                if graph[*c].name() == OUTLINE_NODE {
                    doomed.push(*c);
                } else {
                    stack.push(*c);
                }
            }
        }
        for d in doomed {
            graph.remove_node(d);
        }
        Ok(())
    }

    /// Inverted-hull outline: a child of each mesh with the same geometry
    /// drawn with the outline shader.
    fn add_outline(&mut self, id: i32, outline: i32, nodes: &[Handle<Node>]) -> Result<(), String> {
        self.remove_outline(id)?;
        self.ensure_compiled(outline)?;
        let Some(mat) = self.shared_material(outline)? else { return Ok(()) };
        let graph = self.graph()?;
        for n in nodes {
            let Some(mesh) = graph[*n].cast::<Mesh>() else { continue };
            let surfaces: Vec<Surface> = mesh
                .surfaces()
                .iter()
                .map(|s| {
                    let mut ns = Surface::new(s.data());
                    ns.set_material(mat.clone());
                    ns.bones.set_value_and_mark_modified(s.bones.to_vec());
                    ns
                })
                .collect();
            let child = MeshBuilder::new(BaseBuilder::new().with_name(OUTLINE_NODE).with_cast_shadows(false))
                .with_surfaces(surfaces)
                .with_render_path(RenderPath::Forward)
                .build(graph);
            graph.link_nodes(child, *n);
        }
        Ok(())
    }

    // ---- reflection and debugging

    pub fn shader_report(&self, s: i32, what: &str) -> Result<String, String> {
        let o = self.shader_obj(s)?;
        let p = o.processed.as_deref();
        let list = |f: &dyn Fn(&Processed) -> Vec<String>| p.map(f).unwrap_or_default().join(",");
        Ok(match what {
            "uniforms" => list(&|p| p.params.iter().filter(|q| !q.ty.is_texture()).map(|q| format!("{}:{}", q.name, q.ty.name())).collect()),
            "attributes" => list(&|p| p.attributes.iter().map(|(n, l)| format!("{n}@{l}")).collect()),
            "bindings" => list(&|p| {
                let mut v: Vec<String> = Vec::new();
                for (i, q) in p.params.iter().filter(|q| q.ty.is_texture()).enumerate() {
                    v.push(format!("{}:{}@{i}", q.name, q.ty.name()));
                }
                let mut groups: Vec<&str> = Vec::new();
                for q in p.params.iter().filter(|q| !q.ty.is_texture()) {
                    if !groups.contains(&q.group.as_str()) {
                        groups.push(&q.group);
                    }
                }
                let n = groups.len();
                for (i, g) in groups.into_iter().enumerate() {
                    v.push(format!("{g}:block@{i}"));
                }
                for (i, b) in p.builtins.iter().filter(|b| **b != "fyrox_sceneDepth").enumerate() {
                    v.push(format!("{b}:block@{}", n + i));
                }
                v
            }),
            "resources" => {
                let users = o.users.iter().filter(|e| self.ent_any(**e).and_then(|e| e.shader.as_deref()).is_some_and(|c| c.shader == s)).count();
                let own = self.shader_materials(s).map(|m| m.len()).unwrap_or(0);
                format!(
                    "entities={users},materials={own},passes={},parameters={},keywords={},compiles={},last_compile_ms={:.1},cache_entries={},cache_hits={},source_bytes={}",
                    p.map(|p| p.passes.len()).unwrap_or(0),
                    p.map(|p| p.params.len()).unwrap_or(0),
                    o.keywords.iter().cloned().collect::<Vec<_>>().join("+"),
                    o.compiles,
                    o.last_compile_ms,
                    self.world.shader_cache.entries.len(),
                    self.world.shader_cache.hits,
                    o.effective_source().len()
                )
            }
            _ => {
                // full report
                let status = ["not compiled", "compiled", "failed", "validated (GPU check pending)"][o.status.code() as usize];
                let mut r = format!("Shader {s} '{}'\nlanguage: {}\nbackend: {}\nstatus: {status}\n", o.name, o.lang.name(), self.backend_name());
                if let Some(p) = p {
                    r += &format!(
                        "pass: {:?}, lighting: {:?}, blend: {:?}, cull: {:?}, shadows: {}\npasses: {}\n",
                        p.options.pass,
                        p.options.lighting,
                        p.options.blend,
                        p.options.cull,
                        p.options.shadows,
                        p.passes.iter().map(|x| x.name).collect::<Vec<_>>().join(", ")
                    );
                    r += &format!("uniforms: {}\nbindings: {}\nattributes: {}\nfiles: {}\n", self.shader_report(s, "uniforms")?, self.shader_report(s, "bindings")?, self.shader_report(s, "attributes")?, p.files[1..].join(", "));
                }
                r += &format!("keywords: {}\nenabled: {}\nhot reload: {}\n{}\n", o.keywords.iter().cloned().collect::<Vec<_>>().join(" "), o.enabled, o.hot, self.shader_report(s, "resources")?);
                if !o.error.is_empty() {
                    r += &format!("error: {}\n", o.error);
                }
                r
            }
        })
    }

    pub fn backend_name(&self) -> String {
        match self.gpu {
            Some(_) => "OpenGL 3.3 core / GLES 3.0 (Fyrox); GLSL 330, WGSL via naga".into(),
            None => "OpenGL 3.3 core / GLES 3.0 (Fyrox; window not open yet); GLSL 330, WGSL via naga".into(),
        }
    }

    // ---- camera and light parameters

    /// `ShaderSetCamera`: camera position and matrices (Fyrox space).
    pub fn shader_set_camera(&mut self, s: i32, cam: i32) -> Result<(), String> {
        let node = self.ent(cam)?.node;
        let graph = self.graph()?;
        let n = &graph[node];
        let pos = n.global_position();
        let (view, proj) = match n.cast::<fyrox::scene::camera::Camera>() {
            Some(c) => (c.view_matrix(), c.projection_matrix()),
            None => return Err(format!("Entity {cam} is not a camera")),
        };
        let vp = proj * view;
        let mut any = false;
        for (name, v) in [
            ("nb_cameraPosition", pos.as_slice().to_vec()),
            ("nb_viewMatrix", view.as_slice().to_vec()),
            ("nb_projectionMatrix", proj.as_slice().to_vec()),
            ("nb_viewProjection", vp.as_slice().to_vec()),
        ] {
            if self.shader_obj(s)?.param(name).is_some() {
                self.shader_set(s, name, Val::Floats(v))?;
                any = true;
            }
        }
        if !any {
            return Err(format!("Shader {s} declares none of nb_cameraPosition, nb_viewMatrix, nb_projectionMatrix, nb_viewProjection"));
        }
        Ok(())
    }

    /// `ShaderSetLight`: a light's position, direction (towards the light),
    /// colour and range, into `nb_light*` parameters (arrays use `index`).
    pub fn shader_set_light(&mut self, s: i32, light: i32, index: i32) -> Result<(), String> {
        let node = self.ent(light)?.node;
        let graph = self.graph()?;
        let mut stack = vec![node];
        let mut found = None;
        while let Some(n) = stack.pop() {
            use fyrox::scene::light::{directional::DirectionalLight, point::PointLight, spot::SpotLight};
            let node = &graph[n];
            let found_light = if let Some(p) = node.cast::<PointLight>() {
                Some((p.base_light_ref(), p.radius()))
            } else if let Some(p) = node.cast::<SpotLight>() {
                Some((p.base_light_ref(), p.distance()))
            } else {
                node.cast::<DirectionalLight>().map(|p| (p.base_light_ref(), 1.0e9))
            };
            if let Some((l, range)) = found_light {
                let c = l.color();
                found = Some((graph[n].global_position(), graph[n].up_vector(), [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0], range));
                break;
            }
            stack.extend(graph[n].children().iter().copied());
        }
        let Some((pos, dir, color, range)) = found else { return Err(format!("Entity {light} is not a light")) };
        let mut any = false;
        for (name, v) in [("nb_lightPosition", pos.as_slice().to_vec()), ("nb_lightDirection", dir.normalize().as_slice().to_vec()), ("nb_lightColor", color.to_vec()), ("nb_lightRange", vec![range])] {
            let Some(p) = self.shader_obj(s)?.param(name).cloned() else { continue };
            any = true;
            let v = match p.ty.array_len() {
                Some(len) => {
                    if !(0..len as i32).contains(&index) {
                        return Err(format!("light index {index} is outside {name} (size {len})"));
                    }
                    let comp = p.ty.components();
                    let mut cur = match self.shader_obj(s)?.values.get(name) {
                        Some(Val::Floats(f)) => f.clone(),
                        _ => p.default.clone(),
                    };
                    cur.resize(len as usize * comp, 0.0);
                    cur[index as usize * comp..index as usize * comp + comp].copy_from_slice(&v[..comp]);
                    cur
                }
                None if index != 0 => return Err(format!("{name} is not an array; index must be 0")),
                None => v,
            };
            self.shader_set(s, name, Val::Floats(v))?;
        }
        if !any {
            return Err(format!("Shader {s} declares none of nb_lightPosition, nb_lightDirection, nb_lightColor, nb_lightRange"));
        }
        Ok(())
    }

    /// `ShaderSetMaterial`: copies the parameters two shaders share.
    pub fn shader_copy_params(&mut self, s: i32, from: i32) -> Result<i32, String> {
        self.ensure_compiled(s)?;
        self.ensure_compiled(from)?;
        let src: Vec<(String, Val)> = self.shader_obj(from)?.values.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut n = 0;
        for (k, v) in src {
            let ok = match (self.shader_obj(s)?.param(&k), self.shader_obj(from)?.param(&k)) {
                (Some(a), Some(b)) => a.ty == b.ty || a.ty.is_texture() && b.ty.is_texture(),
                _ => false,
            };
            if ok {
                self.shader_set(s, &k, v)?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// `VertexData`: sets extra value `slot` (0-3) of a vertex.
    pub fn vertex_data(&mut self, surface: i32, index: i32, slot: i32, v: [f32; 4]) -> Result<(), String> {
        if !(0..SLOTS as i32).contains(&slot) {
            return Err(format!("VertexData: slot must be 0 to 3, not {slot}"));
        }
        let s = self.surface_mut(surface)?;
        let i = usize::try_from(index).ok().filter(|&i| i < s.verts.len()).ok_or_else(|| format!("Vertex {index} does not exist"))?;
        let n = s.verts.len();
        s.custom.resize(n, [[0.0; 4]; SLOTS]);
        s.custom[i][slot as usize] = v;
        s.dirty = true;
        Ok(())
    }

    /// Component (0-3 = x, y, z, w) of a vertex's extra value.
    pub fn vertex_data_get(&mut self, surface: i32, index: i32, slot: i32, component: i32) -> Result<f32, String> {
        let s = self.surface_mut(surface)?;
        Ok(usize::try_from(index)
            .ok()
            .and_then(|i| s.custom.get(i))
            .and_then(|c| c.get(slot as usize))
            .and_then(|v| v.get(component as usize))
            .copied()
            .unwrap_or(0.0))
    }
}

fn int(v: i32) -> Result<Value, String> {
    Ok(Value::Int(v))
}

fn text(s: String) -> Result<Value, String> {
    Ok(Value::Str(s.as_str().into()))
}

fn set(h: &mut Host3D, a: &[Value], v: Vec<f32>) -> Result<Value, String> {
    h.shader_set(a[0].int(), &a[1].str(), Val::Floats(v))?;
    VOID
}

/// Colour 0-255 to 0-1 (sRGB, as shaders expect for colours).
fn rgba(a: &[Value], i: usize) -> Vec<f32> {
    (i..i + 4).map(|k| a[k].float() / 255.0).collect()
}

fn source_stage(h: &mut Host3D, a: &[Value], stage: u8) -> Result<Value, String> {
    let s = a[0].int();
    let src = a[1].str().to_string();
    let o = h.shader_obj_mut(s)?;
    match stage {
        0 => {
            o.source = src;
            o.vertex_part = None;
            o.fragment_part = None;
        }
        1 => o.vertex_part = Some(src),
        _ => o.fragment_part = Some(src),
    }
    o.status = Status::NotCompiled;
    VOID
}

/// The shader commands; see the module comment and docs/COMMANDS.md.
pub static COMMANDS: &[(&str, Cmd)] = &[
    // ---- creating and compiling
    ("%CreateShader$source=\"\"$language=\"glsl\"", |h, a, _| {
        let lang = Lang::parse(&a[1].str()).ok_or_else(|| format!("CreateShader: language must be glsl, wgsl or fyrox, not '{}'", a[1].str()))?;
        int(h.add_shader(ShaderObj::new("<source>", lang, a[0].str().to_string())))
    }),
    ("%CompileShader%shader", |h, a, _| int(h.compile_shader(a[0].int())? as i32)),
    ("%ReloadShader%shader", |h, a, _| int(h.reload_shader(a[0].int())? as i32)),
    ("FreeShader%shader", |h, a, _| {
        h.free_shader(a[0].int())?;
        VOID
    }),
    ("%ValidateShader%shader", |h, a, _| int(h.validate_shader(a[0].int())? as i32)),
    ("%ShaderValidate%shader", |h, a, _| int(h.validate_shader(a[0].int())? as i32)),
    ("%ShaderExists%shader", |h, a, _| int(h.shader_obj(a[0].int()).is_ok() as i32)),
    ("%ShaderCompiled%shader", |h, a, _| int(h.shader_obj(a[0].int()).is_ok_and(|o| o.status == Status::Compiled || o.status == Status::Validated) as i32)),
    ("$ShaderError%shader", |h, a, _| text(h.shader_obj(a[0].int())?.error.clone())),
    ("$ShaderCompileError%shader", |h, a, _| text(h.shader_obj(a[0].int())?.error.clone())),
    ("$ShaderLog%shader", |h, a, _| text(h.shader_obj(a[0].int())?.log.join("\n"))),
    ("ShaderSource%shader$source", |h, a, _| source_stage(h, a, 0)),
    ("ShaderVertexSource%shader$source", |h, a, _| source_stage(h, a, 1)),
    ("ShaderFragmentSource%shader$source", |h, a, _| source_stage(h, a, 2)),
    ("ShaderGeometrySource%shader$source", |h, a, _| {
        h.shader_obj(a[0].int())?;
        Err(shadersrc::GEOMETRY_UNSUPPORTED.into())
    }),
    ("ShaderComputeSource%shader$source", |h, a, _| {
        h.shader_obj(a[0].int())?;
        Err(shadersrc::COMPUTE_UNSUPPORTED.into())
    }),
    ("$ShaderGetSource%shader%stage=0", |h, a, _| {
        let o = h.shader_obj(a[0].int())?;
        text(match a[1].int() {
            0 => o.effective_source(),
            1 => o.processed.as_ref().map(|p| p.passes[0].vertex.clone()).unwrap_or_default(),
            2 => o.processed.as_ref().map(|p| p.passes[0].fragment.clone()).unwrap_or_default(),
            3 => o.processed.as_ref().map(|p| p.ron.clone()).unwrap_or_default(),
            s => return Err(format!("ShaderGetSource: stage must be 0-3, not {s}")),
        })
    }),
    // ---- parameters
    ("ShaderSetParameter%shader$name$value", |h, a, _| {
        let s = a[0].int();
        let name = a[1].str();
        h.ensure_compiled(s)?;
        let is_tex = h.shader_obj(s)?.param(&name).is_some_and(|p| p.ty.is_texture());
        let v = if is_tex { Val::Texture(a[2].str().trim().parse().map_err(|_| format!("'{}' is not a texture handle", a[2].str()))?) } else { Val::Floats(parse_numbers(&a[2].str())?) };
        h.shader_set(s, &name, v)?;
        VOID
    }),
    ("$ShaderGetParameter%shader$name", |h, a, _| text(h.shader_get(a[0].int(), &a[1].str())?)),
    ("ShaderSetFloat%shader$name#value", |h, a, _| set(h, a, vec![a[2].float()])),
    ("ShaderSetInt%shader$name%value", |h, a, _| set(h, a, vec![a[2].int() as f32])),
    ("ShaderSetBool%shader$name%value", |h, a, _| set(h, a, vec![(a[2].int() != 0) as i32 as f32])),
    ("ShaderSetVector2%shader$name#x#y", |h, a, _| set(h, a, vec![a[2].float(), a[3].float()])),
    ("ShaderSetVector3%shader$name#x#y#z", |h, a, _| set(h, a, vec![a[2].float(), a[3].float(), a[4].float()])),
    ("ShaderSetVector4%shader$name#x#y#z#w", |h, a, _| set(h, a, vec![a[2].float(), a[3].float(), a[4].float(), a[5].float()])),
    ("ShaderSetMatrix3%shader$name$values", |h, a, _| {
        let v = parse_numbers(&a[2].str())?;
        if v.len() != 9 {
            return Err(format!("ShaderSetMatrix3 needs 9 numbers, got {}", v.len()));
        }
        set(h, a, v)
    }),
    ("ShaderSetMatrix4%shader$name$values", |h, a, _| {
        let v = parse_numbers(&a[2].str())?;
        if v.len() != 16 {
            return Err(format!("ShaderSetMatrix4 needs 16 numbers, got {}", v.len()));
        }
        set(h, a, v)
    }),
    ("ShaderSetColor%shader$name#red#green#blue#alpha=255", |h, a, _| set(h, a, rgba(a, 2))),
    ("ShaderSetTexture%shader$name%texture", |h, a, _| {
        h.shader_set(a[0].int(), &a[1].str(), Val::Texture(a[2].int()))?;
        VOID
    }),
    ("ShaderSetTextureArray%shader$name$textures", |h, a, _| {
        let list: Result<Vec<i32>, String> = a[2].str().split(',').map(|t| t.trim().parse::<i32>().map_err(|_| format!("'{t}' is not a texture handle"))).collect();
        h.shader_set(a[0].int(), &a[1].str(), Val::Textures(list?))?;
        VOID
    }),
    ("ShaderSetCubeMap%shader$name%texture", |h, a, _| {
        let t = a[2].int();
        if t != 0 && !matches!(h.texture(t)?.resource.data_ref().kind(), TextureKind::Cube { .. }) {
            return Err(format!("texture {t} is not a cube map (load it with flag 128)"));
        }
        h.shader_set(a[0].int(), &a[1].str(), Val::Texture(t))?;
        VOID
    }),
    ("ShaderSetTime%shader#time=-1", |h, a, _| {
        let t = a[1].float();
        h.shader_obj_mut(a[0].int())?.manual_time = (t >= 0.0).then_some(t);
        VOID
    }),
    ("ShaderSetResolution%shader#width=0#height=0", |h, a, _| {
        let (w, ht) = (a[1].float(), a[2].float());
        h.shader_obj_mut(a[0].int())?.manual_resolution = (w > 0.0 && ht > 0.0).then_some((w, ht));
        VOID
    }),
    ("ShaderSetCamera%shader%camera", |h, a, _| {
        h.shader_set_camera(a[0].int(), a[1].int())?;
        VOID
    }),
    ("ShaderSetLight%shader%light%index=0", |h, a, _| {
        h.shader_set_light(a[0].int(), a[1].int(), a[2].int())?;
        VOID
    }),
    ("%ShaderSetMaterial%shader%material", |h, a, _| int(h.shader_copy_params(a[0].int(), a[1].int())?)),
    ("ShaderSetBuffer%shader$name$values", |h, a, _| set(h, a, parse_numbers(&a[2].str())?)),
    ("ShaderSetKeyword%shader$keyword%enable=1", |h, a, _| {
        h.set_shader_keyword(a[0].int(), &a[1].str(), a[2].int() != 0)?;
        VOID
    }),
    ("ShaderEnable%shader", |h, a, _| {
        h.set_shader_enabled(a[0].int(), true)?;
        VOID
    }),
    ("ShaderDisable%shader", |h, a, _| {
        h.set_shader_enabled(a[0].int(), false)?;
        VOID
    }),
    ("ShaderApply%shader%entity", |h, a, _| {
        h.entity_shader(a[1].int(), a[0].int())?;
        VOID
    }),
    ("EntityMaterial%entity%material", |h, a, _| {
        h.entity_shader(a[0].int(), a[1].int())?;
        VOID
    }),
    ("%ShaderClone%shader", |h, a, _| int(h.clone_shader(a[0].int())?)),
    ("%ShaderSave%shader$file", |h, a, _| int(h.save_shader(a[0].int(), &a[1].str())? as i32)),
    ("ShaderHotReload%shader%enable=1", |h, a, _| {
        let s = a[0].int();
        let on = a[1].int() != 0;
        let o = h.shader_obj_mut(s)?;
        if on && o.path.is_none() {
            return Err(format!("Shader {s} was not loaded from a file, so it cannot be hot reloaded"));
        }
        o.hot = on;
        h.ensure_compiled(s)?;
        VOID
    }),
    ("%ShaderParameterExists%shader$name", |h, a, _| {
        let s = a[0].int();
        h.ensure_compiled(s)?;
        int(h.shader_obj(s)?.param(&a[1].str()).is_some() as i32)
    }),
    ("%ShaderParameterCount%shader", |h, a, _| {
        let s = a[0].int();
        h.ensure_compiled(s)?;
        int(h.shader_obj(s)?.processed.as_ref().map_or(0, |p| p.params.len() as i32))
    }),
    ("$ShaderParameterName%shader%index", |h, a, _| {
        let s = a[0].int();
        h.ensure_compiled(s)?;
        let i = a[1].int();
        let o = h.shader_obj(s)?;
        let p = o.processed.as_ref().and_then(|p| usize::try_from(i).ok().and_then(|i| p.params.get(i)));
        text(p.map(|p| p.name.clone()).ok_or_else(|| format!("Shader {s} has no parameter {i}"))?)
    }),
    ("$ShaderParameterType%shader$name", |h, a, _| {
        let s = a[0].int();
        h.ensure_compiled(s)?;
        let name = a[1].str();
        text(h.shader_obj(s)?.param(&name).map(|p| p.ty.name()).ok_or_else(|| format!("Shader {s} has no parameter '{name}'"))?)
    }),
    // ---- debugging
    ("$ShaderDebug%shader", |h, a, _| text(h.shader_report(a[0].int(), "all")?)),
    ("$ShaderGetUniforms%shader", |h, a, _| text(h.shader_report(a[0].int(), "uniforms")?)),
    ("$ShaderGetAttributes%shader", |h, a, _| text(h.shader_report(a[0].int(), "attributes")?)),
    ("$ShaderGetBindings%shader", |h, a, _| text(h.shader_report(a[0].int(), "bindings")?)),
    ("$ShaderGetResourceInfo%shader", |h, a, _| text(h.shader_report(a[0].int(), "resources")?)),
    ("%ShaderGetCompilationStatus%shader", |h, a, _| int(h.shader_obj(a[0].int())?.status.code())),
    ("$ShaderGetBackend%shader=0", |h, a, _| {
        let s = a[0].int();
        let lang = if s != 0 { format!(" ({} source)", h.shader_obj(s)?.lang.name()) } else { String::new() };
        text(format!("{}{lang}", h.backend_name()))
    }),
];
