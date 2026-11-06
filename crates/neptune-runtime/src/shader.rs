//! Custom shaders and vertex data (ext): `LoadShader`, `EntityShader`,
//! `ShaderFloat`, `ShaderVector`, `ShaderTexture`, `VertexData`.
//!
//! A shader is a Fyrox shader file (`.shader`: GLSL inside a small RON
//! description of resources and passes; see the shaders in
//! `examples/media`). `EntityShader` draws an entity with it, and the
//! `Shader*` commands set the properties and textures it declares.
//!
//! `VertexData` gives the vertices of a surface up to four extra `vec4`
//! values, read in a shader at locations 8 to 11. Surfaces without extra
//! data keep the standard layout (position 0, uv 1, normal 2, tangent 3,
//! second uv 6, colour 7).

use std::collections::BTreeMap;

use fyrox::{
    asset::untyped::ResourceKind,
    core::{algebra::Vector4, uuid::Uuid},
    material::{shader::{Shader, ShaderResource}, Material, MaterialResource},
    scene::mesh::Mesh,
};

use crate::world::Host3D;

#[derive(Debug, Clone)]
pub enum Prop {
    Float(f32),
    Vector([f32; 4]),
    Texture(i32),
}

#[derive(Debug, Clone)]
pub struct CustomMaterial {
    pub shader: i32,
    pub props: BTreeMap<String, Prop>,
}

/// Number of extra `vec4` values a vertex can carry.
pub const SLOTS: usize = 4;

/// Reads a shader file.
pub fn load(path: &str) -> Result<ShaderResource, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
    let shader = Shader::from_string(&text).map_err(|e| format!("'{path}': {e:?}"))?;
    Ok(ShaderResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, shader))
}

impl Host3D<'_> {
    pub fn shader(&self, s: i32) -> Result<&ShaderResource, String> {
        usize::try_from(s - 1)
            .ok()
            .and_then(|i| self.world.shaders.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Shader {s} does not exist"))
    }

    /// `EntityShader`: draws the entity with a custom shader (0 = back to
    /// the normal materials).
    pub fn entity_shader(&mut self, e: i32, s: i32) -> Result<(), String> {
        if s != 0 {
            self.shader(s)?;
        }
        let ent = self.ent_mut(e)?;
        ent.shader = (s != 0).then(|| Box::new(CustomMaterial { shader: s, props: BTreeMap::new() }));
        self.refresh_material(e)
    }

    pub fn set_shader_prop(&mut self, e: i32, name: &str, p: Prop) -> Result<(), String> {
        let m = self.ent_mut(e)?.shader.as_mut().ok_or_else(|| format!("Entity {e} has no shader (use EntityShader)"))?;
        m.props.insert(name.to_string(), p);
        self.refresh_material(e)
    }

    /// Gives an entity with a custom shader its material. False if the
    /// entity has none.
    pub fn refresh_custom(&mut self, id: i32) -> Result<bool, String> {
        let Some(cm) = self.ent(id)?.shader.as_deref().cloned() else { return Ok(false) };
        let mut m = Material::from_shader(self.shader(cm.shader)?.clone());
        for (name, p) in &cm.props {
            match p {
                Prop::Float(v) => m.set_property(name.as_str(), *v),
                Prop::Vector(v) => m.set_property(name.as_str(), Vector4::new(v[0], v[1], v[2], v[3])),
                Prop::Texture(t) => {
                    if let Ok(t) = self.texture(*t) {
                        m.bind(name.as_str(), t.resource.clone());
                    }
                }
            }
        }
        let material = MaterialResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, m);
        let nodes = self.entity_mesh_nodes(id)?;
        let graph = self.graph()?;
        for n in nodes {
            if let Some(mesh) = graph[n].cast_mut::<Mesh>() {
                for s in mesh.surfaces_mut() {
                    s.set_material(material.clone());
                }
                // custom shaders draw in the forward pass
                mesh.set_render_path(fyrox::scene::mesh::RenderPath::Forward);
            }
        }
        // a later normal material must not reuse these
        self.ent_mut(id)?.own_materials = true;
        Ok(true)
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
