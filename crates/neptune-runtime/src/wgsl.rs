//! WGSL shaders (ext): parsed and validated by naga, translated to GLSL
//! 3.30 for Fyrox's OpenGL renderer, and wrapped like GLSL shaders.
//!
//! Conventions:
//! - one `@vertex` and one `@fragment` entry point (a `@compute` entry
//!   point is an error: Fyrox has no compute stage);
//! - `var<uniform> name: Struct;` becomes a uniform block of material
//!   parameters named `name` (its members are the parameters), except the
//!   Fyrox built-ins `fyrox_instanceData`, `fyrox_cameraData`,
//!   `fyrox_lightsBlock`, `fyrox_lightData`, `fyrox_boneMatrices` and
//!   `fyrox_graphicsSettings`, whose structs must use Fyrox's member names;
//!   read members directly (`fyrox_instanceData.worldViewProjection`);
//! - `var name: texture_2d<f32>` (or `texture_3d`, `texture_cube`) is a
//!   texture parameter; samplers are taken from the texture;
//! - vertex inputs use the mesh locations (0 position, 1 uv, 2 normal, 3
//!   tangent, 4 bone weights, 5 bone indices, 6 second uv, 7 colour, 8-11
//!   `VertexData`), and clip space is OpenGL's;
//! - `#ifdef KEYWORD` / `#ifndef` / `#else` / `#endif` lines select code
//!   by shader keyword; `//!` directives work as in GLSL shaders.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use naga::back::glsl;

use crate::shadersrc::{self, Fallback, Input, Loc, Param, PassKind, PassSource, Processed, Ty};

const BUILTIN_GROUPS: &[&str] = &["fyrox_instanceData", "fyrox_boneMatrices", "fyrox_cameraData", "fyrox_lightData", "fyrox_lightsBlock", "fyrox_graphicsSettings"];

/// Applies `#ifdef` blocks, keeping line numbers (removed lines go blank).
fn conditionals(text: &str, keywords: &BTreeSet<String>) -> Result<String, String> {
    let mut out = String::new();
    // stack of (active, any branch taken)
    let mut stack: Vec<(bool, bool)> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        let active = stack.iter().all(|s| s.0);
        if let Some(k) = t.strip_prefix("#ifdef ").or_else(|| t.strip_prefix("#if ")) {
            let on = keywords.contains(k.trim());
            stack.push((on, on));
        } else if let Some(k) = t.strip_prefix("#ifndef ") {
            let on = !keywords.contains(k.trim());
            stack.push((on, on));
        } else if t == "#else" {
            let Some(top) = stack.last_mut() else { return Err(format!("line {}: #else without #ifdef", i + 1)) };
            *top = (!top.1, true);
        } else if t == "#endif" {
            if stack.pop().is_none() {
                return Err(format!("line {}: #endif without #ifdef", i + 1));
            }
        } else if active {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !stack.is_empty() {
        return Err("#ifdef without #endif".into());
    }
    Ok(out)
}

fn place(map: &[Loc], files: &[String], line: u32) -> String {
    match map.get(line.saturating_sub(1) as usize) {
        Some(l) => format!("{}:{}", files[l.file as usize], l.line),
        None => format!("<wgsl>:{line}"),
    }
}

fn param_ty(module: &naga::Module, ty: naga::Handle<naga::Type>) -> Option<Ty> {
    use naga::{ScalarKind as S, TypeInner as T, VectorSize as V};
    Some(match &module.types[ty].inner {
        T::Scalar(s) => match s.kind {
            S::Float => Ty::Float,
            S::Sint => Ty::Int,
            S::Uint => Ty::UInt,
            S::Bool => Ty::Bool,
            _ => return None,
        },
        T::Vector { size, scalar } if scalar.kind == S::Float => match size {
            V::Bi => Ty::Vec2,
            V::Tri => Ty::Vec3,
            V::Quad => Ty::Vec4,
        },
        T::Matrix { columns: V::Tri, rows: V::Tri, .. } => Ty::Mat3,
        T::Matrix { columns: V::Quad, rows: V::Quad, .. } => Ty::Mat4,
        T::Array { base, size: naga::ArraySize::Constant(n), .. } => {
            let n = u16::try_from(n.get()).ok()?;
            match param_ty(module, *base)? {
                Ty::Float => Ty::FloatArray(n),
                Ty::Int => Ty::IntArray(n),
                Ty::Vec2 => Ty::Vec2Array(n),
                Ty::Vec3 => Ty::Vec3Array(n),
                Ty::Vec4 => Ty::Vec4Array(n),
                Ty::Mat4 => Ty::Mat4Array(n),
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// Processes a WGSL shader.
pub fn process(input: &Input) -> Result<Processed, String> {
    let prep = shadersrc::prepare(input)?;
    let mut opts = prep.options.clone();
    let text = conditionals(&prep.text, input.keywords).map_err(|e| format!("{}: {e}", input.file_name))?;
    let module = naga::front::wgsl::parse_str(&text).map_err(|e| {
        let at = e.location(&text).map(|l| place(&prep.map, &prep.files, l.line_number)).unwrap_or_else(|| input.file_name.to_string());
        format!("{at}: {}", e.emit_to_string(&text).lines().next().unwrap_or(e.message()).trim_start_matches("error: "))
    })?;
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&module).map_err(|e| {
        let at = e
            .spans()
            .next()
            .map(|(span, _)| place(&prep.map, &prep.files, span.location(&text).line_number))
            .unwrap_or_else(|| input.file_name.to_string());
        let mut msg = e.as_inner().to_string();
        let mut src: Option<&dyn std::error::Error> = std::error::Error::source(e.as_inner());
        while let Some(s) = src {
            let _ = write!(msg, ": {s}");
            src = s.source();
        }
        format!("{at}: {msg}")
    })?;
    if module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Compute) {
        return Err(format!("{}: {}", input.file_name, shadersrc::COMPUTE_UNSUPPORTED));
    }
    let entry = |stage| module.entry_points.iter().find(|e| e.stage == stage).map(|e| e.name.clone());
    let (Some(vs), Some(fs)) = (entry(naga::ShaderStage::Vertex), entry(naga::ShaderStage::Fragment)) else {
        return Err(format!("{}: a WGSL shader needs a @vertex and a @fragment entry point", input.file_name));
    };

    // resources
    let mut params: Vec<Param> = Vec::new();
    let mut builtins: Vec<&'static str> = Vec::new();
    let mut renames: Vec<(String, String)> = Vec::new();
    for (_, var) in module.global_variables.iter() {
        let name = var.name.clone().unwrap_or_default();
        let binding = var.binding.as_ref();
        match (&var.space, &module.types[var.ty].inner) {
            (naga::AddressSpace::Uniform, naga::TypeInner::Struct { members, .. }) => {
                if let Some(b) = binding {
                    for st in ["vs", "fs"] {
                        renames.push((format!("_group_{}_binding_{}_{st}", b.group, b.binding), name.clone()));
                    }
                }
                if let Some(bi) = BUILTIN_GROUPS.iter().find(|b| **b == name) {
                    builtins.push(bi);
                    continue;
                }
                for m in members {
                    let mname = m.name.clone().unwrap_or_default();
                    let ty = param_ty(&module, m.ty).ok_or_else(|| format!("{}: member '{mname}' of '{name}' has a type Fyrox cannot store in a uniform block", input.file_name))?;
                    let mut p = Param::new(&mname, ty, prep.defaults.get(&mname).cloned().unwrap_or_default(), Fallback::White);
                    p.group = name.clone();
                    params.push(p);
                }
            }
            (naga::AddressSpace::Uniform, _) => {
                return Err(format!("{}: uniform '{name}' must be a struct (its members become parameters)", input.file_name));
            }
            (naga::AddressSpace::Handle, naga::TypeInner::Image { dim, arrayed, .. }) => {
                if *arrayed {
                    return Err(format!("{}: '{name}': array textures are not supported by Fyrox; use texture_3d", input.file_name));
                }
                let ty = match dim {
                    naga::ImageDimension::D3 => Ty::Sampler3D,
                    naga::ImageDimension::Cube => Ty::SamplerCube,
                    _ => Ty::Sampler2D,
                };
                let fallback = prep.fallbacks.get(&name).copied().unwrap_or(if name.to_ascii_lowercase().contains("normal") { Fallback::Normal } else { Fallback::White });
                if name == "fyrox_sceneDepth" {
                    builtins.push("fyrox_sceneDepth");
                } else {
                    params.push(Param::new(&name, ty, Vec::new(), fallback));
                }
            }
            _ => {}
        }
    }

    // translate each stage
    let stage_glsl = |stage: naga::ShaderStage, ep: &str| -> Result<String, String> {
        let mut out = String::new();
        let options = glsl::Options { version: glsl::Version::Desktop(330), writer_flags: glsl::WriterFlags::empty(), ..Default::default() };
        let pipeline = glsl::PipelineOptions { shader_stage: stage, entry_point: ep.to_string(), multiview: None };
        let mut w = glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, naga::proc::BoundsCheckPolicies::default())
            .map_err(|e| format!("{}: cannot translate to GLSL: {e}", input.file_name))?;
        let reflection = w.write().map_err(|e| format!("{}: cannot translate to GLSL: {e}", input.file_name))?;
        let mut samplers: Vec<(String, String)> = Vec::new();
        for (combined, m) in &reflection.texture_mapping {
            let tex = module.global_variables[m.texture].name.clone().unwrap_or_default();
            samplers.push((combined.clone(), tex));
        }
        let mut body = String::new();
        for line in out.lines() {
            let t = line.trim_start();
            if t.starts_with("#version") || t.starts_with("precision ") {
                continue;
            }
            // Fyrox declares the blocks and samplers itself
            if t.starts_with("layout(std140) uniform ") {
                continue;
            }
            if t.starts_with("uniform ") && samplers.iter().any(|(c, _)| t.trim_end_matches(';').ends_with(c.as_str())) {
                continue;
            }
            body.push_str(line);
            body.push('\n');
        }
        for (from, to) in renames.iter().chain(samplers.iter()) {
            body = shadersrc::replace_word(&body, from, to);
        }
        Ok(body)
    };
    let vertex = stage_glsl(naga::ShaderStage::Vertex, &vs)?;
    let fragment = stage_glsl(naga::ShaderStage::Fragment, &fs)?;

    let name: &'static str = match opts.pass {
        PassKind::Forward => "Forward",
        PassKind::GBuffer => "GBuffer",
        PassKind::Post => "Primary",
    };
    if opts.pass == PassKind::Post {
        opts.shadows = false;
    }
    let generated = |s: &str| vec![Loc { file: 0, line: 0 }; s.lines().count()];
    let mut passes = vec![PassSource { name, vertex_map: generated(&vertex), fragment_map: generated(&fragment), vertex: vertex.clone(), fragment }];
    if opts.shadows {
        passes.extend(shadersrc::shadow_passes(&vertex));
        if !builtins.contains(&"fyrox_lightData") {
            builtins.push("fyrox_lightData");
        }
        if !builtins.contains(&"fyrox_cameraData") {
            builtins.push("fyrox_cameraData");
        }
    }
    // keep Fyrox's order of built-ins
    builtins.sort_by_key(|b| BUILTIN_GROUPS.iter().position(|x| x == b).unwrap_or(99));
    builtins.dedup();
    let ron = shadersrc::to_ron("NeptuneShader", &opts, &params, &builtins, &passes);
    let attributes = crate::shaderlib::ATTRIBUTES.iter().filter(|(_, l)| vertex.contains(&format!("location = {l})"))).map(|(n, l)| (n.to_string(), *l)).collect();
    Ok(Processed { options: opts, params, builtins, attributes, passes, ron, files: prep.files, deps: prep.deps, warnings: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shadersrc::DiskIncludes;

    const SRC: &str = r#"
//! cull none
struct Props { time: f32, tint: vec4<f32> };
struct Instance { worldMatrix: mat4x4<f32>, worldViewProjection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> properties: Props;
@group(0) @binding(1) var<uniform> fyrox_instanceData: Instance;
@group(1) @binding(0) var diffuseTexture: texture_2d<f32>;
@group(1) @binding(1) var diffuseSampler: sampler;
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@location(0) p: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var o: VOut;
    o.pos = fyrox_instanceData.worldViewProjection * vec4<f32>(p.x, p.y + sin(properties.time), p.z, 1.0);
    o.uv = uv;
    return o;
}
@fragment fn fs(i: VOut) -> @location(0) vec4<f32> {
#ifdef RED
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
#else
    return textureSample(diffuseTexture, diffuseSampler, i.uv) * properties.tint;
#endif
}
"#;

    fn run(src: &str, k: &[&str]) -> Result<Processed, String> {
        let keywords: BTreeSet<String> = k.iter().map(|s| s.to_string()).collect();
        process(&Input { source: src, file_name: "test.wgsl", path: None, keywords: &keywords, includes: &DiskIncludes })
    }

    #[test]
    fn translates_and_reflects() {
        let p = run(SRC, &[]).unwrap();
        assert_eq!(p.param("time").unwrap().group, "properties");
        assert_eq!(p.param("diffuseTexture").unwrap().ty, Ty::Sampler2D);
        assert!(p.builtins.contains(&"fyrox_instanceData"));
        let f = &p.passes[0].fragment;
        assert!(f.contains("properties.tint") && f.contains("diffuseTexture"), "{f}");
        assert!(!f.contains("_group_"), "{f}");
        assert!(!p.passes[0].vertex.contains("#version"));
        fyrox::material::shader::Shader::from_string(&p.ron).unwrap();
        assert!(!run(SRC, &["RED"]).unwrap().passes[0].fragment.contains("diffuseTexture"));
    }

    #[test]
    fn errors_have_lines() {
        let e = run("@fragment fn fs() -> @location(0) vec4<f32> {\n  return vec4<f32>(1.0, x, 0.0, 1.0);\n}", &[]).unwrap_err();
        assert!(e.starts_with("test.wgsl:2"), "{e}");
        let e = run("@compute @workgroup_size(1) fn c() {}\n@vertex fn v() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0); }\n@fragment fn f() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }", &[]).unwrap_err();
        assert!(e.contains("compute shaders are not supported"), "{e}");
    }
}
