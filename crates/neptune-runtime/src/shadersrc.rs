//! Shader source processing (ext): turns NeptuneBASIC shader source into a
//! Fyrox shader definition. Pure text work, so it is tested without a GPU.
//!
//! Fyrox 1.0 renders through OpenGL 3.3 (GLES 3.0 on some systems) and
//! accepts GLSL vertex and fragment programs inside a RON description of
//! resources and passes. NeptuneBASIC shaders are plain GLSL (or WGSL,
//! translated by `wgsl.rs`) with a few additions, all optional:
//!
//! - `//! name value` directives: `pass forward|gbuffer|post`, `blend
//!   none|alpha|add|multiply|premultiplied`, `cull back|front|none`,
//!   `depth_write on|off`, `depth_test on|off`, `shadows on|off`,
//!   `alpha_clip on|off`, `lighting pbr|lambert|phong|blinn|toon|unlit`,
//!   `default <param> <values>`, `fallback <texture> white|black|normal`.
//!   A directive may end in `if KEYWORD` or `if !KEYWORD`.
//! - `//! vertex`, `//! fragment` and `//! common` start stage sections
//!   (text before any marker is common to both stages).
//! - `#include "file"` and `#include <neptune/...>` (the built-in library).
//! - `uniform <type> <name> [= default];` lines become material parameters
//!   (scalars go into a uniform block, `sampler*` into texture bindings).
//! - Shaders without a vertex section get a standard vertex program with
//!   skinning; a fragment that defines `nb_surface(inout NbSurface s)` and
//!   no `main` gets a generated `main` for the pass and lighting model.
//!
//! Geometry and compute stages do not exist in Fyrox's renderer and are
//! rejected with an error instead of being emulated.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Glsl,
    Wgsl,
    /// A Fyrox `.shader` file, used as it is.
    Fyrox,
}

impl Lang {
    pub fn from_path(path: &str) -> Lang {
        let p = path.to_ascii_lowercase();
        if p.ends_with(".wgsl") {
            Lang::Wgsl
        } else if p.ends_with(".shader") || p.ends_with(".ron") {
            Lang::Fyrox
        } else {
            Lang::Glsl
        }
    }

    pub fn parse(s: &str) -> Option<Lang> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "glsl" => Some(Lang::Glsl),
            "wgsl" => Some(Lang::Wgsl),
            "fyrox" | "shader" | "ron" => Some(Lang::Fyrox),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::Glsl => "GLSL",
            Lang::Wgsl => "WGSL",
            Lang::Fyrox => "Fyrox",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassKind {
    /// Drawn after the deferred lights, with its own lighting (or none).
    Forward,
    /// Writes Fyrox's G-buffer: full deferred lighting and received shadows.
    GBuffer,
    /// A full-screen post-processing pass (see `postfx.rs`).
    Post,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    None,
    Alpha,
    Add,
    Multiply,
    Premultiplied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cull {
    Back,
    Front,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lighting {
    Pbr,
    Lambert,
    Phong,
    Blinn,
    Toon,
    Unlit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub pass: PassKind,
    pub blend: Blend,
    pub cull: Cull,
    /// `None` = on unless blending.
    pub depth_write: Option<bool>,
    pub depth_test: bool,
    pub shadows: bool,
    pub alpha_clip: bool,
    pub lighting: Lighting,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            pass: PassKind::Forward,
            blend: Blend::None,
            cull: Cull::Back,
            depth_write: None,
            depth_test: true,
            shadows: false,
            alpha_clip: false,
            lighting: Lighting::Pbr,
        }
    }
}

/// Parameter types a shader can declare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Float,
    Int,
    UInt,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    Mat3,
    Mat4,
    FloatArray(u16),
    IntArray(u16),
    Vec2Array(u16),
    Vec3Array(u16),
    Vec4Array(u16),
    Mat4Array(u16),
    Sampler2D,
    Sampler3D,
    SamplerCube,
    /// `sampler2DArray` in source: stored as a 3D texture (Fyrox has no
    /// 2D-array sampler); read with `nb_textureArray`.
    Sampler2DArray,
    USampler2D,
}

impl Ty {
    pub fn parse(word: &str, len: Option<u16>) -> Option<Ty> {
        Some(match (word, len) {
            ("float", None) => Ty::Float,
            ("int", None) => Ty::Int,
            ("uint", None) => Ty::UInt,
            ("bool", None) => Ty::Bool,
            ("vec2", None) => Ty::Vec2,
            ("vec3", None) => Ty::Vec3,
            ("vec4", None) => Ty::Vec4,
            ("mat3", None) => Ty::Mat3,
            ("mat4", None) => Ty::Mat4,
            ("float", Some(n)) => Ty::FloatArray(n),
            ("int", Some(n)) => Ty::IntArray(n),
            ("vec2", Some(n)) => Ty::Vec2Array(n),
            ("vec3", Some(n)) => Ty::Vec3Array(n),
            ("vec4", Some(n)) => Ty::Vec4Array(n),
            ("mat4", Some(n)) => Ty::Mat4Array(n),
            ("sampler2D", None) => Ty::Sampler2D,
            ("sampler3D", None) => Ty::Sampler3D,
            ("samplerCube", None) => Ty::SamplerCube,
            ("sampler2DArray", None) => Ty::Sampler2DArray,
            ("usampler2D", None) => Ty::USampler2D,
            _ => return None,
        })
    }

    pub fn is_texture(self) -> bool {
        matches!(self, Ty::Sampler2D | Ty::Sampler3D | Ty::SamplerCube | Ty::Sampler2DArray | Ty::USampler2D)
    }

    /// Number of floats a value holds (arrays: per element).
    pub fn components(self) -> usize {
        match self {
            Ty::Float | Ty::Int | Ty::UInt | Ty::Bool | Ty::FloatArray(_) | Ty::IntArray(_) => 1,
            Ty::Vec2 | Ty::Vec2Array(_) => 2,
            Ty::Vec3 | Ty::Vec3Array(_) => 3,
            Ty::Vec4 | Ty::Vec4Array(_) => 4,
            Ty::Mat3 => 9,
            Ty::Mat4 | Ty::Mat4Array(_) => 16,
            _ => 0,
        }
    }

    pub fn array_len(self) -> Option<u16> {
        match self {
            Ty::FloatArray(n) | Ty::IntArray(n) | Ty::Vec2Array(n) | Ty::Vec3Array(n) | Ty::Vec4Array(n) | Ty::Mat4Array(n) => Some(n),
            _ => None,
        }
    }

    /// The name programs see (`ShaderParameterType`).
    pub fn name(self) -> String {
        match self {
            Ty::Float => "float".into(),
            Ty::Int => "int".into(),
            Ty::UInt => "uint".into(),
            Ty::Bool => "bool".into(),
            Ty::Vec2 => "vec2".into(),
            Ty::Vec3 => "vec3".into(),
            Ty::Vec4 => "vec4".into(),
            Ty::Mat3 => "mat3".into(),
            Ty::Mat4 => "mat4".into(),
            Ty::FloatArray(n) => format!("float[{n}]"),
            Ty::IntArray(n) => format!("int[{n}]"),
            Ty::Vec2Array(n) => format!("vec2[{n}]"),
            Ty::Vec3Array(n) => format!("vec3[{n}]"),
            Ty::Vec4Array(n) => format!("vec4[{n}]"),
            Ty::Mat4Array(n) => format!("mat4[{n}]"),
            Ty::Sampler2D => "sampler2D".into(),
            Ty::Sampler3D => "sampler3D".into(),
            Ty::SamplerCube => "samplerCube".into(),
            Ty::Sampler2DArray => "sampler2DArray".into(),
            Ty::USampler2D => "usampler2D".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    White,
    Black,
    Normal,
}

impl Fallback {
    fn parse(s: &str) -> Option<Fallback> {
        match s {
            "white" => Some(Fallback::White),
            "black" => Some(Fallback::Black),
            "normal" | "flat" => Some(Fallback::Normal),
            _ => None,
        }
    }
}

/// A material parameter declared by a shader.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
    /// Default value, `ty.components()` floats per element (empty = zero,
    /// or identity for matrices).
    pub default: Vec<f32>,
    pub fallback: Fallback,
    /// Uniform block of a scalar parameter ("properties" unless a WGSL
    /// shader names its own).
    pub group: String,
}

impl Param {
    pub fn new(name: &str, ty: Ty, default: Vec<f32>, fallback: Fallback) -> Param {
        Param { name: name.to_string(), ty, default, fallback, group: "properties".into() }
    }
}

/// Parameters the runtime fills in every frame when a shader uses them.
pub const AUTO_PARAMS: &[(&str, Ty)] = &[
    ("nb_time", Ty::Float),
    ("nb_deltaTime", Ty::Float),
    ("nb_resolution", Ty::Vec2),
    ("nb_ambient", Ty::Vec3),
    ("nb_fogColor", Ty::Vec3),
    ("nb_fogRange", Ty::Vec2),
];

/// Extra parameters of post-processing shaders, set by the post renderer.
pub const POST_PARAMS: &[(&str, Ty)] = &[
    ("nb_wvp", Ty::Mat4),
    ("nb_invViewProj", Ty::Mat4),
    ("nb_prevViewProj", Ty::Mat4),
    ("nb_cameraPos", Ty::Vec3),
    ("nb_clip", Ty::Vec2),
];

/// Textures of post-processing shaders, bound by the post renderer.
pub const POST_TEXTURES: &[&str] = &["nb_screenTexture", "nb_depthTexture", "nb_normalTexture", "nb_originalTexture"];

/// Fyrox's built-in resources, filled in by the renderer.
const BUILTINS: &[(&str, bool)] = &[
    ("fyrox_instanceData", false),
    ("fyrox_boneMatrices", false),
    ("fyrox_cameraData", false),
    ("fyrox_lightData", false),
    ("fyrox_lightsBlock", false),
    ("fyrox_graphicsSettings", false),
    ("fyrox_sceneDepth", true),
];

/// Where a line of generated stage text came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loc {
    /// Index into `Processed::files` (0 = generated code).
    pub file: u16,
    pub line: u32,
}

#[derive(Clone, Debug)]
struct Line {
    text: String,
    loc: Loc,
}

/// One render pass of the result, with its line maps for error reports.
#[derive(Clone, Debug)]
pub struct PassSource {
    pub name: &'static str,
    pub vertex: String,
    pub fragment: String,
    pub vertex_map: Vec<Loc>,
    pub fragment_map: Vec<Loc>,
}

/// The processed shader.
#[derive(Clone, Debug)]
pub struct Processed {
    pub options: Options,
    pub params: Vec<Param>,
    pub builtins: Vec<&'static str>,
    /// Vertex attributes the main pass reads (name, location).
    pub attributes: Vec<(String, u32)>,
    pub passes: Vec<PassSource>,
    /// The Fyrox shader text.
    pub ron: String,
    /// File names for `Loc::file` (index 0 is "<generated>").
    pub files: Vec<String>,
    /// Files on disk this shader was built from (for hot reloading).
    pub deps: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

impl Processed {
    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }

    /// Line of the shader text where a pass's stage code starts.
    pub fn stage_line(&self, pass: &str, fragment: bool) -> isize {
        fyrox::material::shader::Shader::from_string(&self.ron)
            .ok()
            .and_then(|s| s.definition.passes.iter().find(|q| q.name == pass).map(|q| if fragment { q.fragment_shader_line } else { q.vertex_shader_line }))
            .unwrap_or(0)
    }

    /// Rewrites `0(12)` / `0:12` line references in a driver message for
    /// `program` (Fyrox's "<shader>_<pass>_VertexShader") to `file:line`.
    pub fn map_error(&self, program: &str, message: &str) -> String {
        let (rest, fragment) = if let Some(r) = program.strip_suffix("_VertexShader") {
            (r, false)
        } else if let Some(r) = program.strip_suffix("_FragmentShader") {
            (r, true)
        } else {
            (program, false)
        };
        let pass = rest.rsplit('_').next().unwrap_or("");
        let Some(p) = self.passes.iter().find(|p| p.name == pass) else { return message.to_string() };
        let map = if fragment { &p.fragment_map } else { &p.vertex_map };
        let stage = if fragment { "fragment" } else { "vertex" };
        // Fyrox reports lines of the whole .shader text; the stage starts at
        // the line its definition records
        let base = self.stage_line(pass, fragment);
        let mut out = String::new();
        for line in message.lines() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            match find_line_ref(line) {
                Some((_, end, n)) => {
                    let body = n as isize - base - 1;
                    let place = match usize::try_from(body - 1).ok().and_then(|i| map.get(i)) {
                        _ if map.is_empty() => format!("{}:{n}", self.files.get(1).map_or("<shader>", |f| f.as_str())),
                        Some(l) if l.file != 0 => format!("{}:{}", self.files[l.file as usize], l.line),
                        Some(l) => format!("<generated {stage} code>:{}", l.line),
                        None => format!("<{stage}>:{body}"),
                    };
                    let msg = line[end..].trim_start_matches([':', ' ']);
                    let _ = writeln!(out, "{place} ({pass} {stage}): {msg}");
                }
                None => {
                    let _ = writeln!(out, "({pass} {stage}) {line}");
                }
            }
        }
        out.trim_end().to_string()
    }
}

/// Finds a driver line reference: `0(12)` (NVIDIA), `0:12` (AMD, Intel,
/// Mesa) or a leading `:12` / `12:`. Returns the byte range and the number.
fn find_line_ref(line: &str) -> Option<(usize, usize, usize)> {
    let b = line.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'0' && (i == 0 || !b[i - 1].is_ascii_digit()) && i + 1 < b.len() && (b[i + 1] == b'(' || b[i + 1] == b':') {
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 2 {
                let n: usize = line[i + 2..j].parse().ok()?;
                // skip a column "(5)" after the line number
                let mut end = j;
                if end < b.len() && b[end] == b')' {
                    end += 1;
                }
                if end < b.len() && b[end] == b'(' {
                    while end < b.len() && b[end] != b')' {
                        end += 1;
                    }
                    end = (end + 1).min(b.len());
                }
                return Some((i, end, n));
            }
        }
    }
    None
}

/// Source of included files: the built-in library or the disk.
pub trait Includes {
    /// Returns (display name, text, path on disk if any).
    fn load(&self, name: &str, from: Option<&Path>) -> Result<(String, String, Option<PathBuf>), String>;
}

/// The default include source: `neptune/...` from the built-in library,
/// anything else relative to the including file.
pub struct DiskIncludes;

impl Includes for DiskIncludes {
    fn load(&self, name: &str, from: Option<&Path>) -> Result<(String, String, Option<PathBuf>), String> {
        if let Some(text) = crate::shaderlib::include(name) {
            return Ok((format!("<{name}>"), text.to_string(), None));
        }
        let path = match from.and_then(Path::parent) {
            Some(dir) if !Path::new(name).is_absolute() => dir.join(name),
            _ => PathBuf::from(name),
        };
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot include '{name}': {e}"))?;
        Ok((name.to_string(), text, Some(path)))
    }
}

struct Pre<'a> {
    includes: &'a dyn Includes,
    files: Vec<String>,
    deps: Vec<PathBuf>,
    once: BTreeSet<String>,
    depth: usize,
}

impl Pre<'_> {
    fn expand(&mut self, text: &str, file: u16, path: Option<&Path>, out: &mut Vec<Line>) -> Result<(), String> {
        self.depth += 1;
        if self.depth > 32 {
            return Err("includes nested more than 32 deep (an include cycle?)".into());
        }
        for (i, raw) in text.lines().enumerate() {
            let t = raw.trim();
            let target = t
                .strip_prefix("#include")
                .or_else(|| t.strip_prefix("//!").map(str::trim_start).and_then(|d| d.strip_prefix("include ")))
                .map(|t| {
                    // a comment after the file name is not part of it
                    let t = t.trim();
                    let close = match t.chars().next() {
                        Some('<') => t.find('>'),
                        Some('"') => t[1..].find('"').map(|e| e + 1),
                        _ => t.find("//").map(|e| e.saturating_sub(1)),
                    };
                    close.map_or(t, |e| &t[..=e]).trim()
                });
            if let Some(target) = target {
                let name = target.trim_matches(|c| c == '"' || c == '<' || c == '>').trim();
                if name.is_empty() {
                    return Err(format!("{}:{}: #include needs a file name", self.files[file as usize], i + 1));
                }
                let (shown, body, dep) = self
                    .includes
                    .load(name, path)
                    .map_err(|e| format!("{}:{}: {e}", self.files[file as usize], i + 1))?;
                if !self.once.insert(shown.clone()) {
                    out.push(Line { text: String::new(), loc: Loc { file, line: i as u32 + 1 } });
                    continue;
                }
                self.files.push(shown);
                let idx = (self.files.len() - 1) as u16;
                if let Some(d) = &dep {
                    self.deps.push(d.clone());
                }
                self.expand(&body, idx, dep.as_deref().or(path), out)?;
                continue;
            }
            out.push(Line { text: raw.to_string(), loc: Loc { file, line: i as u32 + 1 } });
        }
        self.depth -= 1;
        Ok(())
    }
}

#[derive(Default)]
struct Sections {
    common: Vec<Line>,
    vertex: Option<Vec<Line>>,
    fragment: Option<Vec<Line>>,
}

fn parse_bool(v: &str, what: &str) -> Result<bool, String> {
    match v {
        "on" | "true" | "1" | "yes" => Ok(true),
        "off" | "false" | "0" | "no" => Ok(false),
        _ => Err(format!("{what} must be on or off, not '{v}'")),
    }
}

/// Parses `//!` directives into options; returns the remaining sections.
fn directives(
    lines: Vec<Line>,
    keywords: &BTreeSet<String>,
    files: &[String],
    opts: &mut Options,
    defaults: &mut BTreeMap<String, Vec<f32>>,
    fallbacks: &mut BTreeMap<String, Fallback>,
) -> Result<Sections, String> {
    let mut s = Sections::default();
    #[derive(Clone, Copy)]
    enum Cur {
        Common,
        Vertex,
        Fragment,
    }
    let mut cur = Cur::Common;
    for l in lines {
        let t = l.text.trim();
        let Some(d) = t.strip_prefix("//!") else {
            match cur {
                Cur::Common => s.common.push(l),
                Cur::Vertex => s.vertex.get_or_insert_with(Vec::new).push(l),
                Cur::Fragment => s.fragment.get_or_insert_with(Vec::new).push(l),
            }
            continue;
        };
        let at = format!("{}:{}", files[l.loc.file as usize], l.loc.line);
        let mut words: Vec<&str> = d.split_whitespace().collect();
        // trailing "if KEYWORD" / "if !KEYWORD"
        if words.len() >= 2 && words[words.len() - 2] == "if" {
            let k = words[words.len() - 1];
            let (neg, k) = match k.strip_prefix('!') {
                Some(k) => (true, k),
                None => (false, k),
            };
            words.truncate(words.len() - 2);
            if keywords.contains(k) == neg {
                continue;
            }
        }
        let Some((&key, args)) = words.split_first() else { continue };
        let arg = args.first().copied().unwrap_or("").to_ascii_lowercase();
        let err = |m: String| format!("{at}: {m}");
        match key.to_ascii_lowercase().as_str() {
            "vertex" => {
                cur = Cur::Vertex;
                s.vertex.get_or_insert_with(Vec::new);
            }
            "fragment" | "pixel" => {
                cur = Cur::Fragment;
                s.fragment.get_or_insert_with(Vec::new);
            }
            "common" => cur = Cur::Common,
            "geometry" => return Err(err(GEOMETRY_UNSUPPORTED.into())),
            "compute" => return Err(err(COMPUTE_UNSUPPORTED.into())),
            "name" => {}
            "pass" => {
                opts.pass = match arg.as_str() {
                    "forward" => PassKind::Forward,
                    "gbuffer" | "deferred" => PassKind::GBuffer,
                    "post" | "postprocess" => PassKind::Post,
                    _ => return Err(err(format!("pass must be forward, gbuffer or post, not '{arg}'"))),
                }
            }
            "blend" => {
                opts.blend = match arg.as_str() {
                    "none" | "opaque" | "off" => Blend::None,
                    "alpha" => Blend::Alpha,
                    "add" | "additive" => Blend::Add,
                    "multiply" => Blend::Multiply,
                    "premultiplied" => Blend::Premultiplied,
                    _ => return Err(err(format!("blend must be none, alpha, add, multiply or premultiplied, not '{arg}'"))),
                }
            }
            "cull" => {
                opts.cull = match arg.as_str() {
                    "back" => Cull::Back,
                    "front" => Cull::Front,
                    "none" | "off" => Cull::None,
                    _ => return Err(err(format!("cull must be back, front or none, not '{arg}'"))),
                }
            }
            "depth_write" => opts.depth_write = Some(parse_bool(&arg, "depth_write").map_err(err)?),
            "depth_test" => opts.depth_test = parse_bool(&arg, "depth_test").map_err(err)?,
            "shadows" => opts.shadows = parse_bool(&arg, "shadows").map_err(err)?,
            "alpha_clip" => opts.alpha_clip = parse_bool(&arg, "alpha_clip").map_err(err)?,
            "lighting" => {
                opts.lighting = match arg.as_str() {
                    "pbr" => Lighting::Pbr,
                    "lambert" | "diffuse" => Lighting::Lambert,
                    "phong" => Lighting::Phong,
                    "blinn" | "blinnphong" | "blinn-phong" => Lighting::Blinn,
                    "toon" | "cel" => Lighting::Toon,
                    "unlit" | "none" => Lighting::Unlit,
                    _ => return Err(err(format!("lighting must be pbr, lambert, phong, blinn, toon or unlit, not '{arg}'"))),
                }
            }
            "default" => {
                let Some((name, vals)) = args.split_first() else { return Err(err("default needs a parameter name".into())) };
                let v: Result<Vec<f32>, _> = vals.iter().flat_map(|v| v.split(',')).filter(|v| !v.is_empty()).map(|v| v.parse::<f32>()).collect();
                defaults.insert(name.to_string(), v.map_err(|_| err(format!("bad default for {name}")))?);
            }
            "fallback" => {
                let (Some(name), Some(f)) = (args.first(), args.get(1).and_then(|f| Fallback::parse(&f.to_ascii_lowercase()))) else {
                    return Err(err("fallback needs a texture name and white, black or normal".into()));
                };
                fallbacks.insert(name.to_string(), f);
            }
            "include" => {}
            other => return Err(err(format!("unknown directive '{other}'"))),
        }
    }
    Ok(s)
}

pub const GEOMETRY_UNSUPPORTED: &str = "geometry shaders are not supported: Fyrox's OpenGL 3.3 renderer has vertex and fragment stages only. Generate geometry on the CPU (CreateMesh, AddVertex) or displace vertices in the vertex shader";
pub const COMPUTE_UNSUPPORTED: &str = "compute shaders are not supported: Fyrox's OpenGL 3.3 renderer has no compute stage. Use a post-processing pass to compute into a texture (CreatePostProcess with PostProcessOutput) or do the work on the CPU";

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// True if `text` contains `word` as a whole identifier.
pub fn has_word(text: &str, word: &str) -> bool {
    let mut start = 0;
    while let Some(i) = text[start..].find(word) {
        let a = start + i;
        let b = a + word.len();
        let before = text[..a].chars().next_back().is_some_and(is_ident);
        let after = text[b..].chars().next().is_some_and(is_ident);
        if !before && !after {
            return true;
        }
        start = b;
    }
    false
}

/// Prefixes uses of parameters with their block (`speed` ->
/// `properties.speed`); member accesses (`s.speed`) and preprocessor
/// lines are left alone. `library`: only `nb_*` names.
fn qualify(line: &str, names: &[(&str, &str)], library: bool) -> String {
    if names.is_empty() || line.trim_start().starts_with('#') {
        return line.to_string();
    }
    let code_end = line.find("//").unwrap_or(line.len());
    let mut out = String::with_capacity(line.len() + 16);
    let b = line.as_bytes();
    let mut i = 0;
    while i < line.len() {
        let c = b[i] as char;
        if i < code_end && (c.is_ascii_alphabetic() || c == '_') && (i == 0 || !is_ident(b[i - 1] as char)) {
            let mut j = i;
            while j < line.len() && is_ident(b[j] as char) {
                j += 1;
            }
            let word = &line[i..j];
            let after_dot = line[..i].trim_end().ends_with('.');
            match names.iter().find(|(n, _)| *n == word) {
                Some((n, g)) if !after_dot && (!library || n.starts_with("nb_")) => {
                    out.push_str(g);
                    out.push('.');
                    out.push_str(word);
                }
                _ => out.push_str(word),
            }
            i = j;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Replaces whole-identifier `from` by `to`.
pub fn replace_word(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = 0;
    while let Some(i) = text[start..].find(from) {
        let a = start + i;
        let b = a + from.len();
        let before = text[..a].chars().next_back().is_some_and(is_ident);
        let after = text[b..].chars().next().is_some_and(is_ident);
        out.push_str(&text[start..a]);
        out.push_str(if !before && !after { to } else { from });
        start = b;
    }
    out.push_str(&text[start..]);
    out
}

/// Numbers in an initializer such as `vec4(1.0, 0.5, 0, 1)` or `true`.
fn numbers(s: &str) -> Vec<f32> {
    let s = s.trim();
    if s == "true" {
        return vec![1.0];
    }
    if s == "false" {
        return vec![0.0];
    }
    let inner = match (s.find('('), s.rfind(')')) {
        (Some(a), Some(b)) if b > a => &s[a + 1..b],
        _ => s,
    };
    inner.split(',').filter_map(|p| p.trim().trim_end_matches(['f', 'u']).parse::<f32>().ok()).collect()
}

/// Parses a `uniform` declaration line. `Ok(None)` if it is not one.
fn uniform_line(t: &str) -> Result<Option<Param>, String> {
    let Some(rest) = t.strip_prefix("uniform ") else { return Ok(None) };
    if rest.contains('{') {
        return Ok(None);
    }
    let (decl, comment) = match rest.find("//") {
        Some(i) => (&rest[..i], &rest[i + 2..]),
        None => (rest, ""),
    };
    let decl = decl.trim().trim_end_matches(';').trim();
    let (decl, init) = match decl.find('=') {
        Some(i) => (decl[..i].trim(), Some(decl[i + 1..].trim())),
        None => (decl, None),
    };
    // drop precision qualifiers
    let words: Vec<&str> = decl.split_whitespace().filter(|w| !matches!(*w, "highp" | "mediump" | "lowp")).collect();
    let [ty, name] = words.as_slice() else { return Err(format!("cannot read uniform declaration '{t}'")) };
    let (name, len) = match name.find('[') {
        Some(i) => {
            let n: u16 = name[i + 1..].trim_end_matches(']').parse().map_err(|_| format!("array size of '{name}' must be a number"))?;
            (&name[..i], Some(n))
        }
        None => (*name, None),
    };
    let ty = Ty::parse(ty, len).ok_or_else(|| format!("uniform type '{ty}' is not supported (use float, int, uint, bool, vec2-4, mat3, mat4, arrays of them, or sampler2D/3D/Cube/2DArray)"))?;
    let comment = comment.trim().trim_start_matches('!').trim();
    let fallback = comment
        .strip_prefix("fallback")
        .and_then(|f| Fallback::parse(f.trim()))
        .unwrap_or(if name.to_ascii_lowercase().contains("normal") { Fallback::Normal } else { Fallback::White });
    Ok(Some(Param::new(name, ty, init.map(numbers).unwrap_or_default(), fallback)))
}

fn stage_text(parts: &[&[Line]]) -> (String, Vec<Loc>) {
    let mut s = String::new();
    let mut map = Vec::new();
    for p in parts {
        for l in p.iter() {
            s.push_str(&l.text);
            s.push('\n');
            map.push(l.loc);
        }
    }
    (s, map)
}

fn generated(text: &str) -> Vec<Line> {
    text.lines().enumerate().map(|(i, t)| Line { text: t.to_string(), loc: Loc { file: 0, line: i as u32 + 1 } }).collect()
}

/// What `process` needs besides the source text.
pub struct Input<'a> {
    pub source: &'a str,
    /// Shown in errors (file name or "<source>").
    pub file_name: &'a str,
    /// File on disk, for relative includes and hot reloading.
    pub path: Option<&'a Path>,
    pub keywords: &'a BTreeSet<String>,
    pub includes: &'a dyn Includes,
}

/// Includes expanded and directives read: what the WGSL front end needs.
pub struct Prepared {
    pub text: String,
    pub map: Vec<Loc>,
    pub options: Options,
    pub defaults: BTreeMap<String, Vec<f32>>,
    pub fallbacks: BTreeMap<String, Fallback>,
    pub files: Vec<String>,
    pub deps: Vec<PathBuf>,
}

pub fn prepare(input: &Input) -> Result<Prepared, String> {
    let mut pre = Pre { includes: input.includes, files: vec!["<generated>".into(), input.file_name.to_string()], deps: Vec::new(), once: BTreeSet::new(), depth: 0 };
    if let Some(p) = input.path {
        pre.deps.push(p.to_path_buf());
    }
    let mut lines = Vec::new();
    pre.expand(input.source, 1, input.path, &mut lines)?;
    let mut options = Options::default();
    let mut defaults = BTreeMap::new();
    let mut fallbacks = BTreeMap::new();
    let s = directives(lines, input.keywords, &pre.files, &mut options, &mut defaults, &mut fallbacks)?;
    let all: Vec<Line> = s.common.into_iter().chain(s.vertex.unwrap_or_default()).chain(s.fragment.unwrap_or_default()).collect();
    let (text, map) = stage_text(&[&all]);
    Ok(Prepared { text, map, options, defaults, fallbacks, files: pre.files, deps: pre.deps })
}

/// The `#define` lines for keywords.
pub fn keyword_defines(keywords: &BTreeSet<String>) -> Result<String, String> {
    let mut head = String::new();
    for k in keywords {
        if !k.chars().all(is_ident) || k.starts_with(|c: char| c.is_ascii_digit()) || k.is_empty() {
            return Err(format!("keyword '{k}' is not a valid name"));
        }
        let _ = writeln!(head, "#define {k} 1");
    }
    Ok(head)
}

/// Shadow passes for a translated (WGSL) vertex program.
pub fn shadow_passes(vertex: &str) -> Vec<PassSource> {
    let mut out = Vec::new();
    for (name, point) in [("DirectionalShadow", false), ("SpotShadow", false), ("PointShadow", true)] {
        let v = if point { format!("#define main nb_userMain\n{vertex}{}", crate::shaderlib::SHADOW_WRAPPER) } else { vertex.to_string() };
        let f = crate::shaderlib::shadow_fragment(point, false).to_string();
        let vertex_map = vec![Loc { file: 0, line: 0 }; v.lines().count()];
        let fragment_map = vec![Loc { file: 0, line: 0 }; f.lines().count()];
        out.push(PassSource { name, vertex: v, fragment: f, vertex_map, fragment_map });
    }
    out
}

/// Processes NeptuneBASIC GLSL into a Fyrox shader.
pub fn process(input: &Input) -> Result<Processed, String> {
    let mut pre = Pre { includes: input.includes, files: vec!["<generated>".into(), input.file_name.to_string()], deps: Vec::new(), once: BTreeSet::new(), depth: 0 };
    if let Some(p) = input.path {
        pre.deps.push(p.to_path_buf());
    }
    let mut lines = Vec::new();
    pre.expand(input.source, 1, input.path, &mut lines)?;
    let mut opts = Options::default();
    let mut defaults = BTreeMap::new();
    let mut fallbacks = BTreeMap::new();
    let mut s = directives(lines, input.keywords, &pre.files, &mut opts, &mut defaults, &mut fallbacks)?;

    // uniforms become parameters
    let mut params: Vec<Param> = Vec::new();
    let mut warnings = Vec::new();
    let files = pre.files.clone();
    let mut take = |sec: &mut Vec<Line>| -> Result<(), String> {
        for l in sec.iter_mut() {
            let t = l.text.trim();
            if let Some(p) = uniform_line(t).map_err(|e| format!("{}:{}: {e}", files[l.loc.file as usize], l.loc.line))? {
                if p.name.starts_with("fyrox_") {
                    return Err(format!("{}:{}: '{}' is a Fyrox built-in; use it without declaring it", files[l.loc.file as usize], l.loc.line, p.name));
                }
                match params.iter().find(|q| q.name == p.name) {
                    Some(q) if q.ty != p.ty => {
                        return Err(format!("{}:{}: uniform '{}' declared again with another type", files[l.loc.file as usize], l.loc.line, p.name))
                    }
                    Some(_) => {}
                    None => params.push(p),
                }
                l.text.clear();
            }
        }
        Ok(())
    };
    take(&mut s.common)?;
    if let Some(v) = s.vertex.as_mut() {
        take(v)?;
    }
    if let Some(f) = s.fragment.as_mut() {
        take(f)?;
    }
    for p in params.iter_mut() {
        if let Some(d) = defaults.remove(&p.name) {
            p.default = d;
        }
        if let Some(f) = fallbacks.remove(&p.name) {
            p.fallback = f;
        }
    }
    for name in defaults.keys().chain(fallbacks.keys()) {
        warnings.push(format!("directive names '{name}', which is not a uniform of this shader"));
    }

    let mut user_vertex = s.vertex.take();
    // a vertex section without main() only adds hooks to the standard program
    let mut hooks: Vec<Line> = Vec::new();
    if let Some(v) = &user_vertex {
        if !has_word(&stage_text(&[v]).0, "main") {
            hooks = user_vertex.take().unwrap_or_default();
        }
    }
    let Some(user_fragment) = s.fragment.take() else {
        return Err(format!("{}: the shader has no fragment stage (start it with //! fragment)", input.file_name));
    };
    let (common_text, _) = stage_text(&[&s.common]);
    let (frag_text, _) = stage_text(&[&user_fragment]);
    let vert_text = user_vertex.as_ref().map(|v| stage_text(&[v]).0).unwrap_or_else(|| stage_text(&[&hooks]).0);
    let all_user = format!("{common_text}{vert_text}{frag_text}");
    let has_surface = has_word(&all_user, "nb_surface");
    let frag_main = has_word(&format!("{common_text}{frag_text}"), "main");
    if !frag_main && !(has_surface || opts.pass == PassKind::Post && has_word(&all_user, "nb_post")) {
        return Err(format!(
            "{}: the fragment stage needs a main() function{}",
            input.file_name,
            if opts.pass == PassKind::Post { " or vec4 nb_post(vec2 uv)" } else { " or void nb_surface(inout NbSurface s)" }
        ));
    }
    if opts.blend != Blend::None && opts.pass == PassKind::GBuffer {
        warnings.push("blending needs the forward pass; the shader draws in the forward pass".into());
        opts.pass = PassKind::Forward;
    }
    if opts.pass == PassKind::Post {
        // a full-screen quad: no depth, no culling
        opts.shadows = false;
        opts.depth_test = false;
        opts.depth_write = Some(false);
        opts.cull = Cull::None;
    }

    // keyword and parameter macros at the top of every stage
    let mut head = keyword_defines(input.keywords)?;
    let pass_define = match opts.pass {
        PassKind::Forward => "NB_PASS_FORWARD",
        PassKind::GBuffer => "NB_PASS_GBUFFER",
        PassKind::Post => "NB_PASS_POST",
    };
    let _ = writeln!(head, "#define {pass_define} 1");
    let _ = writeln!(head, "#define NB_LIGHTING_{} 1", format!("{:?}", opts.lighting).to_ascii_uppercase());
    if opts.alpha_clip {
        let _ = writeln!(head, "#define NB_ALPHA_CLIP 1");
    }
    if has_surface {
        let _ = writeln!(head, "#define NB_HAS_SURFACE 1");
    }
    let vertex_hook = has_word(&format!("{common_text}{vert_text}"), "nb_displace");
    let world_hook = has_word(&format!("{common_text}{vert_text}"), "nb_displaceWorld");
    if vertex_hook {
        let _ = writeln!(head, "#define NB_VERTEX_HOOK 1");
    }
    if world_hook {
        let _ = writeln!(head, "#define NB_WORLD_HOOK 1");
    }

    // passes
    let mut passes = Vec::new();
    let main_name: &'static str = match opts.pass {
        PassKind::Forward => "Forward",
        PassKind::GBuffer => "GBuffer",
        PassKind::Post => "Primary",
    };
    // outputs the program declares itself are not declared again
    let declares_out = frag_text.lines().chain(common_text.lines()).any(|l| {
        let t = l.trim_start();
        t.starts_with("out ") || (t.starts_with("layout") && t.contains(" out "))
    });
    let varyings = user_vertex.is_none();
    let frag_prelude = generated(&crate::shaderlib::fragment_prelude_ex(Some(opts.pass), varyings, !declares_out));
    let frag_main_gen = if frag_main { Vec::new() } else { generated(&crate::shaderlib::fragment_main(opts.pass, opts.lighting)) };
    let head_lines = generated(&head);
    let vertex_prelude = generated(&crate::shaderlib::vertex_prelude());
    let surface_lib = generated(&crate::shaderlib::surface_lib());

    let vertex_body = |shadow_point: bool| -> Vec<Line> {
        let mut out = Vec::new();
        match &user_vertex {
            Some(v) if shadow_point => {
                out.extend(generated("#define main nb_userMain"));
                out.extend(v.iter().cloned());
                out.extend(generated(crate::shaderlib::SHADOW_WRAPPER));
            }
            Some(v) => out.extend(v.iter().cloned()),
            None if opts.pass == PassKind::Post => {
                out.extend(hooks.iter().cloned());
                out.extend(generated(crate::shaderlib::POST_VERTEX));
            }
            None => {
                out.extend(generated(&crate::shaderlib::default_vertex_decls(shadow_point)));
                out.extend(hooks.iter().cloned());
                out.extend(generated(&crate::shaderlib::default_vertex_main(shadow_point)));
            }
        }
        out
    };
    {
        let (vertex, vertex_map) = stage_text(&[&head_lines, &vertex_prelude, &s.common, &vertex_body(false)]);
        let (fragment, fragment_map) = stage_text(&[&head_lines, &surface_lib, &frag_prelude, &s.common, &user_fragment, &frag_main_gen]);
        passes.push(PassSource { name: main_name, vertex, fragment, vertex_map, fragment_map });
    }
    if opts.shadows {
        let shadow_prelude = generated(&crate::shaderlib::fragment_prelude_ex(None, varyings, false));
        for (name, point) in [("DirectionalShadow", false), ("SpotShadow", false), ("PointShadow", true)] {
            let mut sh_head = head.clone();
            let _ = writeln!(sh_head, "#define NB_PASS_SHADOW 1");
            if point {
                let _ = writeln!(sh_head, "#define NB_PASS_POINT_SHADOW 1");
            }
            let sh_head = generated(&sh_head);
            let (vertex, vertex_map) = stage_text(&[&sh_head, &vertex_prelude, &s.common, &vertex_body(point)]);
            // the user's fragment is only needed for cut-out alpha
            let clip = opts.alpha_clip && has_surface && !frag_main;
            let user: &[Line] = if clip { &user_fragment } else { &[] };
            let prelude: &[Line] = if clip { &shadow_prelude } else { &[] };
            let common: &[Line] = if clip { &s.common } else { &[] };
            let lib: &[Line] = if clip { &surface_lib } else { &[] };
            let (fragment, fragment_map) = stage_text(&[&sh_head, lib, prelude, common, user, &generated(crate::shaderlib::shadow_fragment(point, clip))]);
            passes.push(PassSource { name, vertex, fragment, vertex_map, fragment_map });
        }
    }

    // parameters the runtime provides
    let all_text: String = passes.iter().flat_map(|p| [p.vertex.as_str(), p.fragment.as_str()]).collect();
    let mut auto: Vec<(&str, Ty)> = AUTO_PARAMS.to_vec();
    if opts.pass == PassKind::Post {
        auto.extend_from_slice(POST_PARAMS);
    }
    for (name, ty) in auto {
        if has_word(&all_text, name) && params.iter().all(|p| p.name != name) {
            params.push(Param::new(name, ty, Vec::new(), Fallback::White));
        }
    }
    if opts.pass == PassKind::Post {
        for name in POST_TEXTURES {
            if has_word(&all_text, name) && params.iter().all(|p| p.name != *name) {
                params.push(Param::new(name, Ty::Sampler2D, Vec::new(), Fallback::Black));
            }
        }
    }
    for p in &params {
        if p.ty.is_texture() {
            continue;
        }
        let n = p.ty.components() * p.ty.array_len().unwrap_or(1) as usize;
        if !p.default.is_empty() && p.default.len() != n && !(p.default.len() == 1 && p.ty.array_len().is_none()) && p.ty.array_len().is_none() {
            warnings.push(format!("default of '{}' has {} values, {} expected", p.name, p.default.len(), n));
        }
    }
    // scalars are read from their uniform block: `speed` becomes
    // `properties.speed` (in library code only the nb_* names, so struct
    // members such as NbSurface.roughness stay as they are)
    let scalars: Vec<(&str, &str)> = params.iter().filter(|p| !p.ty.is_texture()).map(|p| (p.name.as_str(), p.group.as_str())).collect();
    let library_files: Vec<bool> = pre.files.iter().enumerate().map(|(i, f)| i == 0 || f.starts_with("<neptune/")).collect();
    let mut qualified = Vec::new();
    for p in &passes {
        let mut out = p.clone();
        for (text, map) in [(&mut out.vertex, &out.vertex_map), (&mut out.fragment, &out.fragment_map)] {
            let mut t = String::with_capacity(text.len() + 256);
            for (i, line) in text.lines().enumerate() {
                let library = map.get(i).is_none_or(|l| library_files.get(l.file as usize).copied().unwrap_or(true));
                t.push_str(&qualify(line, &scalars, library));
                t.push('\n');
            }
            *text = t;
        }
        qualified.push(out);
    }
    let passes = qualified;
    let all_text: String = passes.iter().flat_map(|p| [p.vertex.as_str(), p.fragment.as_str()]).collect();
    let builtins: Vec<&'static str> = BUILTINS.iter().filter(|(b, _)| has_word(&all_text, b)).map(|(b, _)| *b).collect();
    let main_vertex = &passes[0].vertex;
    let attributes = crate::shaderlib::ATTRIBUTES
        .iter()
        .filter(|(n, _)| has_word(main_vertex, n))
        .map(|(n, l)| (n.to_string(), *l))
        .collect();

    let ron = to_ron("NeptuneShader", &opts, &params, &builtins, &passes);
    Ok(Processed { options: opts, params, builtins, attributes, passes, ron, files: pre.files, deps: pre.deps, warnings })
}

fn fmt_f(v: f32) -> String {
    if v.is_finite() {
        let s = format!("{v:?}");
        if s.contains('.') || s.contains('e') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        "0.0".into()
    }
}

fn values(p: &Param, n: usize) -> Vec<f32> {
    let mut v = p.default.clone();
    if v.len() == 1 && n > 1 && !matches!(p.ty, Ty::Mat3 | Ty::Mat4) {
        v = vec![v[0]; n];
    }
    if v.len() < n {
        // matrices default to identity
        let side = match p.ty {
            Ty::Mat3 => 3,
            Ty::Mat4 => 4,
            _ => 0,
        };
        if side > 0 && v.is_empty() {
            v = (0..n).map(|i| if i % (side + 1) == 0 { 1.0 } else { 0.0 }).collect();
        }
        v.resize(n, 0.0);
    }
    v.truncate(n);
    v
}

fn tuple(v: &[f32]) -> String {
    format!("({})", v.iter().map(|x| fmt_f(*x)).collect::<Vec<_>>().join(", "))
}

/// The RON text of a Fyrox property kind with the parameter's default.
fn property_kind(p: &Param) -> String {
    let n = p.ty.components();
    match p.ty {
        Ty::Float => format!("Float(value: {})", fmt_f(values(p, 1)[0])),
        Ty::Int => format!("Int(value: {})", values(p, 1)[0].round() as i32),
        Ty::UInt => format!("UInt(value: {})", values(p, 1)[0].max(0.0).round() as u32),
        Ty::Bool => format!("Bool(value: {})", values(p, 1)[0] != 0.0),
        Ty::Vec2 => format!("Vector2(value: {})", tuple(&values(p, 2))),
        Ty::Vec3 => format!("Vector3(value: {})", tuple(&values(p, 3))),
        Ty::Vec4 => format!("Vector4(value: {})", tuple(&values(p, 4))),
        Ty::Mat3 => format!("Matrix3(value: {})", tuple(&values(p, 9))),
        Ty::Mat4 => format!("Matrix4(value: {})", tuple(&values(p, 16))),
        Ty::FloatArray(m) => format!("FloatArray(value: [{}], max_len: {m})", p.default.iter().take(m as usize).map(|x| fmt_f(*x)).collect::<Vec<_>>().join(", ")),
        Ty::IntArray(m) => format!("IntArray(value: [{}], max_len: {m})", p.default.iter().take(m as usize).map(|x| (x.round() as i32).to_string()).collect::<Vec<_>>().join(", ")),
        Ty::Vec2Array(m) | Ty::Vec3Array(m) | Ty::Vec4Array(m) | Ty::Mat4Array(m) => {
            let kind = match p.ty {
                Ty::Vec2Array(_) => "Vector2Array",
                Ty::Vec3Array(_) => "Vector3Array",
                Ty::Vec4Array(_) => "Vector4Array",
                _ => "Matrix4Array",
            };
            let elems: Vec<String> = p.default.chunks_exact(n).take(m as usize).map(tuple).collect();
            format!("{kind}(value: [{}], max_len: {m})", elems.join(", "))
        }
        _ => unreachable!("textures are not properties"),
    }
}

fn sampler_kind(t: Ty) -> &'static str {
    match t {
        Ty::Sampler3D | Ty::Sampler2DArray => "Sampler3D",
        Ty::SamplerCube => "SamplerCube",
        Ty::USampler2D => "USampler2D",
        _ => "Sampler2D",
    }
}

fn draw_params(name: &str, o: &Options) -> String {
    let shadow = name.ends_with("Shadow");
    let color = !(shadow && name != "PointShadow");
    let cull = match (shadow, o.cull) {
        (_, Cull::Back) => "Some(Back)",
        (_, Cull::Front) => "Some(Front)",
        (_, Cull::None) => "None",
    };
    let blending = !shadow && o.pass != PassKind::GBuffer;
    let depth_write = if shadow { true } else { o.depth_write.unwrap_or(o.blend == Blend::None || !blending) };
    let depth_test = if o.depth_test || shadow { "Some(Less)" } else { "None" };
    let blend = match (blending, o.blend) {
        (false, _) | (_, Blend::None) => "None".to_string(),
        (_, b) => {
            let (s, d, sa, da) = match b {
                Blend::Alpha => ("SrcAlpha", "OneMinusSrcAlpha", "SrcAlpha", "OneMinusSrcAlpha"),
                Blend::Add => ("SrcAlpha", "One", "Zero", "One"),
                Blend::Multiply => ("DstColor", "Zero", "Zero", "One"),
                Blend::Premultiplied => ("One", "OneMinusSrcAlpha", "One", "OneMinusSrcAlpha"),
                Blend::None => unreachable!(),
            };
            format!("Some(BlendParameters(func: BlendFunc(sfactor: {s}, dfactor: {d}, alpha_sfactor: {sa}, alpha_dfactor: {da}), equation: BlendEquation(rgb: Add, alpha: Add)))")
        }
    };
    format!(
        "DrawParameters(cull_face: {cull}, color_write: ColorMask(red: {color}, green: {color}, blue: {color}, alpha: {color}), depth_write: {depth_write}, stencil_test: None, depth_test: {depth_test}, blend: {blend}, stencil_op: StencilOp(fail: Keep, zfail: Keep, zpass: Keep, write_mask: 0xFFFF_FFFF), scissor_box: None)"
    )
}

/// Picks a raw-string fence that the code does not contain.
fn raw(code: &str) -> String {
    let mut hashes = String::from("#");
    while code.contains(&format!("\"{hashes}")) {
        hashes.push('#');
    }
    format!("r{hashes}\"\n{code}\"{hashes}")
}

/// Writes the Fyrox shader description.
pub fn to_ron(name: &str, o: &Options, params: &[Param], builtins: &[&str], passes: &[PassSource]) -> String {
    let mut r = String::new();
    let _ = writeln!(r, "(\n    name: \"{name}\",\n    resources: [");
    let mut tex_binding = 0;
    for p in params.iter().filter(|p| p.ty.is_texture()) {
        let fallback = match p.fallback {
            Fallback::White => "White",
            Fallback::Black => "Black",
            Fallback::Normal => "Normal",
        };
        let _ = writeln!(r, "        (name: \"{}\", kind: Texture(kind: {}, fallback: {fallback}), binding: {tex_binding}),", p.name, sampler_kind(p.ty));
        tex_binding += 1;
    }
    let mut group_binding = 0;
    let mut groups: Vec<&str> = Vec::new();
    for p in params.iter().filter(|p| !p.ty.is_texture()) {
        if !groups.contains(&p.group.as_str()) {
            groups.push(&p.group);
        }
    }
    for g in groups {
        let _ = writeln!(r, "        (name: \"{g}\", kind: PropertyGroup([");
        for p in params.iter().filter(|p| !p.ty.is_texture() && p.group == g) {
            let _ = writeln!(r, "            (name: \"{}\", kind: {}),", p.name, property_kind(p));
        }
        let _ = writeln!(r, "        ]), binding: {group_binding}),");
        group_binding += 1;
    }
    for b in builtins {
        if *b == "fyrox_sceneDepth" {
            let _ = writeln!(r, "        (name: \"{b}\", kind: Texture(kind: Sampler2D, fallback: White), binding: {tex_binding}),");
            tex_binding += 1;
        } else {
            let _ = writeln!(r, "        (name: \"{b}\", kind: PropertyGroup([]), binding: {group_binding}),");
            group_binding += 1;
        }
    }
    let _ = writeln!(r, "    ],");
    let present: Vec<&str> = passes.iter().map(|p| p.name).collect();
    let disabled: Vec<String> = ["GBuffer", "Forward", "DirectionalShadow", "SpotShadow", "PointShadow"]
        .iter()
        .filter(|p| !present.contains(p) && o.pass != PassKind::Post)
        .map(|p| format!("\"{p}\""))
        .collect();
    let _ = writeln!(r, "    disabled_passes: [{}],", disabled.join(", "));
    let _ = writeln!(r, "    passes: [");
    for p in passes {
        let _ = writeln!(
            r,
            "        (\n            name: \"{}\",\n            draw_parameters: {},\n            vertex_shader: {},\n            fragment_shader: {},\n        ),",
            p.name,
            draw_params(p.name, o),
            raw(&p.vertex),
            raw(&p.fragment)
        );
    }
    let _ = writeln!(r, "    ],\n)");
    r
}

/// Reads parameters from a Fyrox `.shader` definition (for reflection of
/// shaders loaded as they are).
pub fn params_of_definition(def: &fyrox::material::shader::ShaderDefinition) -> Vec<Param> {
    use fyrox::graphics::gpu_program::{SamplerFallback, SamplerKind, ShaderPropertyKind as K, ShaderResourceKind};
    let mut out = Vec::new();
    for r in &def.resources {
        if r.name.starts_with("fyrox_") {
            continue;
        }
        match &r.kind {
            ShaderResourceKind::Texture { kind, fallback } => out.push(Param {
                group: String::new(),
                name: r.name.to_string(),
                ty: match kind {
                    SamplerKind::SamplerCube => Ty::SamplerCube,
                    SamplerKind::Sampler3D => Ty::Sampler3D,
                    SamplerKind::USampler2D => Ty::USampler2D,
                    _ => Ty::Sampler2D,
                },
                default: Vec::new(),
                fallback: match fallback {
                    SamplerFallback::Black => Fallback::Black,
                    SamplerFallback::Normal => Fallback::Normal,
                    _ => Fallback::White,
                },
            }),
            ShaderResourceKind::PropertyGroup(fields) => {
                for f in fields {
                    let (ty, default) = match &f.kind {
                        K::Float { value } => (Ty::Float, vec![*value]),
                        K::Int { value } => (Ty::Int, vec![*value as f32]),
                        K::UInt { value } => (Ty::UInt, vec![*value as f32]),
                        K::Bool { value } => (Ty::Bool, vec![*value as i32 as f32]),
                        K::Vector2 { value } => (Ty::Vec2, value.as_slice().to_vec()),
                        K::Vector3 { value } => (Ty::Vec3, value.as_slice().to_vec()),
                        K::Vector4 { value } => (Ty::Vec4, value.as_slice().to_vec()),
                        K::Color { r, g, b, a } => (Ty::Vec4, vec![*r as f32 / 255.0, *g as f32 / 255.0, *b as f32 / 255.0, *a as f32 / 255.0]),
                        K::Matrix3 { value } => (Ty::Mat3, value.as_slice().to_vec()),
                        K::Matrix4 { value } => (Ty::Mat4, value.as_slice().to_vec()),
                        K::FloatArray { value, max_len } => (Ty::FloatArray(*max_len as u16), value.clone()),
                        K::IntArray { value, max_len } => (Ty::IntArray(*max_len as u16), value.iter().map(|v| *v as f32).collect()),
                        K::Vector2Array { value, max_len } => (Ty::Vec2Array(*max_len as u16), value.iter().flat_map(|v| v.as_slice().to_vec()).collect()),
                        K::Vector3Array { value, max_len } => (Ty::Vec3Array(*max_len as u16), value.iter().flat_map(|v| v.as_slice().to_vec()).collect()),
                        K::Vector4Array { value, max_len } => (Ty::Vec4Array(*max_len as u16), value.iter().flat_map(|v| v.as_slice().to_vec()).collect()),
                        K::Matrix4Array { value, max_len } => (Ty::Mat4Array(*max_len as u16), value.iter().flat_map(|v| v.as_slice().to_vec()).collect()),
                        _ => continue,
                    };
                    out.push(Param { name: f.name.to_string(), ty, default, fallback: Fallback::White, group: r.name.to_string() });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<Processed, String> {
        run_k(src, &[])
    }

    fn run_k(src: &str, k: &[&str]) -> Result<Processed, String> {
        let keywords: BTreeSet<String> = k.iter().map(|s| s.to_string()).collect();
        process(&Input { source: src, file_name: "test.glsl", path: None, keywords: &keywords, includes: &DiskIncludes })
    }

    fn parses(p: &Processed) {
        fyrox::material::shader::Shader::from_string(&p.ron).unwrap_or_else(|e| panic!("{e:?}\n{}", p.ron));
    }

    #[test]
    fn uniforms_become_parameters() {
        let p = run("uniform float speed = 2.0;\nuniform vec4 tint = vec4(1, 0.5, 0.25, 1);\nuniform sampler2D mainTex;\nuniform sampler2D bumpNormal;\n//! fragment\nvoid main() { FragColor = tint * texture(mainTex, nb_uv) * speed; }").unwrap();
        assert_eq!(p.param("speed").unwrap().default, vec![2.0]);
        assert_eq!(p.param("tint").unwrap().ty, Ty::Vec4);
        assert_eq!(p.param("mainTex").unwrap().ty, Ty::Sampler2D);
        assert_eq!(p.param("bumpNormal").unwrap().fallback, Fallback::Normal);
        assert!(p.passes[0].fragment.contains("properties.tint * texture(mainTex, nb_uv) * properties.speed"), "{}", p.passes[0].fragment);
        assert!(!p.passes[0].fragment.contains("uniform float speed"));
        parses(&p);
    }

    #[test]
    fn surface_shaders_get_a_main_and_shadow_passes() {
        let p = run("//! pass gbuffer\n//! shadows on\n//! fragment\nvoid nb_surface(inout NbSurface s) { s.albedo = vec3(1, 0, 0); }").unwrap();
        let names: Vec<&str> = p.passes.iter().map(|p| p.name).collect();
        assert_eq!(names, ["GBuffer", "DirectionalShadow", "SpotShadow", "PointShadow"]);
        assert!(p.passes[0].fragment.contains("outDecalMask"));
        assert!(p.builtins.contains(&"fyrox_lightData"));
        parses(&p);
    }

    #[test]
    fn directives_and_keywords() {
        let src = "//! blend alpha if GLASS\n//! cull none\n//! fragment\nvoid main() {\n#ifdef GLASS\nFragColor = vec4(1, 1, 1, 0.5);\n#else\nFragColor = vec4(1);\n#endif\n}";
        let p = run_k(src, &["GLASS"]).unwrap();
        assert_eq!(p.options.blend, Blend::Alpha);
        assert!(p.ron.contains("SrcAlpha"));
        assert!(p.passes[0].fragment.contains("#define GLASS 1"));
        let q = run(src).unwrap();
        assert_eq!(q.options.blend, Blend::None);
        parses(&p);
    }

    #[test]
    fn errors_are_reported_with_places() {
        assert!(run("//! geometry\nvoid main(){}").unwrap_err().contains("geometry shaders are not supported"));
        assert!(run("//! fragment\nvoid nothing(){}").unwrap_err().contains("needs a main()"));
        assert!(run("//! pass sideways\n//! fragment\nvoid main(){}").unwrap_err().starts_with("test.glsl:1:"));
        assert!(run("uniform vec5 x;\n//! fragment\nvoid main(){}").unwrap_err().contains("not supported"));
        assert!(run("#include \"missing.glsl\"\n//! fragment\nvoid main(){}").unwrap_err().contains("cannot include"));
    }

    #[test]
    fn driver_errors_map_to_source_lines() {
        let p = run("uniform float k;\n//! fragment\nvoid main() {\n  FragColor = vec4(k);\n  oops;\n}").unwrap();
        // find the generated line holding "oops"
        let n = p.passes[0].fragment.lines().position(|l| l.contains("oops")).unwrap() as isize + 2 + p.stage_line("Forward", true);
        let nv = format!("0({n}) : error C1008: undefined variable \"oops\"");
        let m = p.map_error("NeptuneShader_Forward_FragmentShader", &nv);
        assert!(m.starts_with("test.glsl:5"), "{m}");
        let amd = format!("ERROR: 0:{n}: 'oops' : undeclared identifier");
        assert!(p.map_error("NeptuneShader_Forward_FragmentShader", &amd).contains("test.glsl:5"));
    }

    #[test]
    fn library_includes_and_auto_parameters() {
        let p = run("#include <neptune/noise.glsl>   // noise, with a comment after it\n//! fragment\nvoid nb_surface(inout NbSurface s) { s.albedo = vec3(nb_fbm(nb_uv * 4.0 + nb_time, 4)); }").unwrap();
        assert!(p.param("nb_time").is_some());
        assert!(p.files.iter().any(|f| f == "<neptune/noise.glsl>"));
        parses(&p);
    }

    #[test]
    fn every_library_preset_processes() {
        for name in crate::shaderlib::PRESETS {
            let src = crate::shaderlib::preset(name).unwrap();
            let p = process(&Input { source: &src, file_name: name, path: None, keywords: &BTreeSet::new(), includes: &DiskIncludes })
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            parses(&p);
        }
    }

    #[test]
    fn post_shaders() {
        let p = run("//! pass post\nuniform float amount = 0.5;\n//! fragment\nvec4 nb_post(vec2 uv) { return texture(nb_screenTexture, uv) * amount; }").unwrap();
        assert_eq!(p.passes[0].name, "Primary");
        assert!(p.param("nb_wvp").is_some());
        assert_eq!(p.param("nb_screenTexture").unwrap().ty, Ty::Sampler2D);
        parses(&p);
    }

    #[test]
    fn words() {
        assert!(has_word("a nb_time b", "nb_time"));
        assert!(!has_word("nb_timer", "nb_time"));
        assert_eq!(replace_word("x _g_vs.time _g_vs2", "_g_vs", "properties"), "x properties.time _g_vs2");
        let n = [("roughness", "properties"), ("nb_time", "properties")];
        assert_eq!(qualify("s.roughness = roughness * nb_time; // roughness", &n, false), "s.roughness = properties.roughness * properties.nb_time; // roughness");
        assert_eq!(qualify("float roughness; x = nb_time;", &n, true), "float roughness; x = properties.nb_time;");
    }
}
