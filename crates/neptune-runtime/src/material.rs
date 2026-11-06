//! Entity appearance: chooses between Fyrox's standard PBR material and the
//! classic material ([`crate::classic`]) and keeps either up to date.

use fyrox::{
    asset::untyped::ResourceKind,
    core::{algebra::Vector2, color::Color, pool::Handle, uuid::Uuid},
    material::{Material, MaterialResource, MaterialResourceBinding},
    resource::texture::TextureResource,
    scene::{
        mesh::{Mesh, RenderPath},
        node::Node,
    },
};

use fyrox::graph::SceneGraph;

use crate::classic::{self, ClassicState, Layer, BLEND_ALPHA};
use crate::texture::flags;
use crate::world::Host3D;

impl Host3D<'_> {
    /// Mesh nodes that belong to an entity (its own node and the nodes of a
    /// loaded model inside it, but not child entities).
    pub fn entity_mesh_nodes(&mut self, id: i32) -> Result<Vec<Handle<Node>>, String> {
        let node = self.ent(id)?.node;
        let skip = self.other_entity_nodes(id);
        let graph = self.graph()?;
        let mut out = Vec::new();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            stack.extend(graph[n].children().iter().copied().filter(|c| !skip(*c)));
            if graph[n].cast::<Mesh>().is_some() {
                out.push(n);
            }
        }
        Ok(out)
    }

    /// Does this entity need the classic material?
    fn needs_classic(&self, id: i32) -> Result<bool, String> {
        let e = self.ent(id)?;
        if e.fx != 0 || e.blend != BLEND_ALPHA || e.shininess > 0.0 || e.color.a < 255 || e.order != 0 {
            return Ok(true);
        }
        if self.world.classic.fog_mode != 0 || self.has_painted_surfaces(id) {
            return Ok(true);
        }
        if e.textures[1..].iter().any(|&t| t != 0) {
            return Ok(true);
        }
        if e.textures[0] != 0 {
            if let Ok(t) = self.texture(e.textures[0]) {
                if t.needs_classic() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Re-applies colour, alpha, textures and effects to an entity's surfaces.
    pub fn refresh_material(&mut self, id: i32) -> Result<(), String> {
        if self.needs_classic(id)? {
            self.apply_classic(id)
        } else {
            self.restore_originals(id)?;
            self.apply_standard(id)
        }
    }

    /// Refreshes every entity (after a scene-wide change such as fog).
    pub fn refresh_all_materials(&mut self) -> Result<(), String> {
        let ids: Vec<i32> = self.entities().map(|(i, _)| i).collect();
        for id in ids {
            self.refresh_material(id)?;
        }
        Ok(())
    }

    fn layers(&self, id: i32) -> Result<Vec<Layer>, String> {
        let (textures, frames) = (self.ent(id)?.textures, self.ent(id)?.tex_frames);
        let mut layers = self.texture_layers(&textures)?;
        // animated textures: show the chosen frame
        let used = textures.iter().zip(frames.iter()).filter(|(&t, _)| t != 0);
        for (layer, (&t, &f)) in layers.iter_mut().zip(used) {
            if let Ok(info) = self.texture(t) {
                if info.frames > 0 {
                    let ((sx, sy), (ox, oy)) = info.frame_window(f);
                    layer.scale = (layer.scale.0 / sx, layer.scale.1 / sy);
                    layer.offset = (layer.offset.0 + ox, layer.offset.1 + oy);
                }
            }
        }
        Ok(layers)
    }

    fn brush_layers(&self, b: &crate::mesh::Brush) -> Result<Vec<Layer>, String> {
        self.texture_layers(&b.textures)
    }

    fn texture_layers(&self, textures: &[i32]) -> Result<Vec<Layer>, String> {
        let mut layers = Vec::new();
        for &t in textures {
            if t == 0 {
                continue;
            }
            let info = self.texture(t)?;
            layers.push(Layer {
                texture: info.resource.clone(),
                blend: info.blend,
                scale: (info.scale_u, info.scale_v),
                offset: (info.pos_u, info.pos_v),
                rotation: info.rotation,
                coords: info.coords,
                sphere_map: info.flags & flags::SPHERE_MAP != 0,
            });
        }
        Ok(layers)
    }

    fn apply_classic(&mut self, id: i32) -> Result<(), String> {
        let layers = self.layers(id)?;
        let (color, shininess, fx, blend, normal_tex, base_tex) = {
            let e = self.ent(id)?;
            (e.color, e.shininess, e.fx, e.blend, e.normal_map, e.textures[0])
        };
        let normal_map = if normal_tex != 0 { Some(self.texture(normal_tex)?.resource.clone()) } else { None };
        let alpha_test = base_tex != 0 && self.texture(base_tex).is_ok_and(|t| t.flags & flags::MASKED != 0);
        let globals = self.world.classic;
        let nodes = self.entity_mesh_nodes(id)?;
        let had_originals = !self.ent(id)?.originals.is_empty();
        let mut originals = Vec::new();
        let mut state = ClassicState { color, shininess, fx, blend, layers, normal_map, alpha_test };
        let transparent = state.transparent();
        let entity_node = self.ent(id)?.node;
        let mut brushes = Vec::new();
        if self.has_painted_surfaces(id) {
            let count = self.graph()?[entity_node].cast::<Mesh>().map_or(0, |m| m.surfaces().len());
            for i in 0..count {
                let b = self.surface_brush(id, i);
                let layers = match &b {
                    Some(b) => self.brush_layers(b)?,
                    None => Vec::new(),
                };
                brushes.push(b.map(|b| (b, layers)));
            }
        }
        let graph = self.graph()?;
        for n in nodes {
            let Some(mesh) = graph[n].cast_mut::<Mesh>() else { continue };
            for (i, s) in mesh.surfaces_mut().iter_mut().enumerate() {
                let current = s.material().clone();
                let is_classic = classic::is_classic(&current.data_ref());
                if !is_classic && !had_originals {
                    originals.push((n, i, current.clone()));
                }
                // a model's own texture shows when the program set none
                let mut surface_state = state.clone();
                if n == entity_node {
                    if let Some(Some((b, layers))) = brushes.get(i) {
                        // a painted surface combines its brush with the entity settings
                        let c = surface_state.color;
                        let m = |x: u8, y: u8| ((x as u16 * y as u16) / 255) as u8;
                        surface_state.color = Color::from_rgba(m(c.r, b.color.r), m(c.g, b.color.g), m(c.b, b.color.b), m(c.a, b.color.a));
                        surface_state.fx |= b.fx;
                        surface_state.shininess = surface_state.shininess.max(b.shininess);
                        if b.blend != BLEND_ALPHA {
                            surface_state.blend = b.blend;
                        }
                        if surface_state.layers.is_empty() {
                            surface_state.layers = layers.clone();
                        }
                    }
                }
                if surface_state.layers.is_empty() && !is_classic {
                    if let Some(t) = texture_binding(&current.data_ref(), "diffuseTexture") {
                        surface_state.layers.push(Layer {
                            texture: t,
                            blend: 2,
                            scale: (1.0, 1.0),
                            offset: (0.0, 0.0),
                            rotation: 0.0,
                            coords: 0,
                            sphere_map: false,
                        });
                    }
                } else if surface_state.layers.is_empty() && is_classic {
                    if let Some(t) = texture_binding(&current.data_ref(), "layerTexture0") {
                        surface_state.layers.push(Layer {
                            texture: t,
                            blend: 2,
                            scale: (1.0, 1.0),
                            offset: (0.0, 0.0),
                            rotation: 0.0,
                            coords: 0,
                            sphere_map: false,
                        });
                    }
                }
                s.set_material(surface_state.material(&globals));
            }
            mesh.set_render_path(RenderPath::Forward);
        }
        let _ = transparent;
        state.layers.clear();
        if !had_originals {
            self.ent_mut(id)?.originals = originals;
        }
        self.ent_mut(id)?.own_materials = true;
        Ok(())
    }

    /// Puts back the materials an entity had before it became classic.
    fn restore_originals(&mut self, id: i32) -> Result<(), String> {
        let originals = std::mem::take(&mut self.ent_mut(id)?.originals);
        if originals.is_empty() {
            return Ok(());
        }
        let graph = self.graph()?;
        for (n, i, m) in originals {
            if let Some(mesh) = graph.try_get_mut(n).ok().and_then(|n| n.cast_mut::<Mesh>()) {
                if let Some(s) = mesh.surfaces_mut().get_mut(i) {
                    s.set_material(m);
                }
            }
        }
        // the originals may be shared with the model resource
        self.ent_mut(id)?.own_materials = false;
        Ok(())
    }

    fn apply_standard(&mut self, id: i32) -> Result<(), String> {
        let (color, tex_id, normal_id) = {
            let e = self.ent(id)?;
            (e.color, e.textures[0], e.normal_map)
        };
        let tex = if tex_id != 0 { self.texture(tex_id).ok().cloned() } else { None };
        let normal = if normal_id != 0 { self.texture(normal_id).ok().map(|t| t.resource.clone()) } else { None };
        let blend = tex.as_ref().is_some_and(|t| t.needs_blending());
        let is_model = self.ent(id)?.model;
        self.with_materials(id, |m| {
            m.set_property("diffuseColor", color);
            match &tex {
                Some(t) => {
                    m.bind("diffuseTexture", t.resource.clone());
                    m.set_property("texCoordScale", Vector2::new(1.0 / t.scale_u, 1.0 / t.scale_v));
                }
                None if !is_model => {
                    // models keep their own textures; primitives go back to plain colour
                    m.unbind("diffuseTexture");
                    m.set_property("texCoordScale", Vector2::new(1.0f32, 1.0));
                }
                None => {}
            }
            if let Some(n) = &normal {
                m.bind("normalTexture", n.clone());
            }
        })?;
        let nodes = self.entity_mesh_nodes(id)?;
        let graph = self.graph()?;
        for n in nodes {
            if let Some(mesh) = graph[n].cast_mut::<Mesh>() {
                mesh.set_render_path(if blend { RenderPath::Forward } else { RenderPath::Deferred });
            }
        }
        Ok(())
    }
}

/// The texture bound to `name` in a material, if any.
fn texture_binding(m: &Material, name: &str) -> Option<TextureResource> {
    match m.binding_ref(name) {
        Some(MaterialResourceBinding::Texture(t)) => t.value.clone(),
        _ => None,
    }
}

/// A fresh standard material (used by commands that need one).
pub fn standard(color: fyrox::core::color::Color) -> MaterialResource {
    let mut m = Material::standard();
    m.set_property("diffuseColor", color);
    MaterialResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, m)
}
