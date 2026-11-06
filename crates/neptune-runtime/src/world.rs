//! Entity bookkeeping on top of the Fyrox scene graph.
//!
//! An entity handle (the `Int` a program holds) indexes this table; each
//! entry owns one Fyrox node and its Blitz-space local transform.

use fyrox::{
    asset::untyped::ResourceKind,
    core::{
        algebra::{UnitQuaternion, Vector3},
        color::Color,
        pool::Handle,
        uuid::Uuid,
    },
    material::{Material, MaterialResource},
    scene::{
        base::BaseBuilder,
        camera::{CameraBuilder, PerspectiveProjection, Projection},
        graph::Graph,
        light::{directional::DirectionalLightBuilder, point::PointLightBuilder, spot::SpotLightBuilder, BaseLightBuilder},
        mesh::{
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            Mesh, MeshBuilder,
        },
        node::Node,
        pivot::PivotBuilder,
        transform::TransformBuilder,
        EnvironmentLightingSource, Scene,
    },
};
use fyrox::graph::SceneGraph;
use neptune_vm::{Host, NativeCtx, Value};

use crate::draw2d::Canvas2D;
use crate::texture::TextureInfo;
use crate::input::Input;
use crate::transform::{WorldXform, Xform};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    Directional = 1,
    Point = 2,
    Spot = 3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Pivot,
    /// `mode` is the Blitz `CameraProjMode`: 0 off, 1 perspective, 2 orthographic.
    Camera { zoom: f32, mode: i32 },
    /// The light node is a child of the entity node, rotated so it shines along +Z.
    Light { kind: LightKind, light: Handle<Node> },
    Mesh,
}

#[derive(Debug)]
pub struct Entity {
    pub node: Handle<Node>,
    pub kind: Kind,
    pub parent: u32,
    pub children: Vec<u32>,
    pub xf: Xform,
    pub name: String,
    pub color: Color,
    pub hidden: bool,
    /// Texture handles per layer applied with `EntityTexture` (0 = none).
    pub textures: [i32; crate::classic::MAX_LAYERS],
    /// `EntityNormalMap` (ext) texture, 0 = none.
    pub normal_map: i32,
    /// `EntityShininess`, `EntityFX`, `EntityBlend`.
    pub shininess: f32,
    pub fx: i32,
    pub blend: i32,
    /// Materials replaced by the classic material, restored when the entity
    /// no longer needs it: (mesh node, surface index, original material).
    pub originals: Vec<(Handle<Node>, usize, MaterialResource)>,
    /// Loaded with `LoadMesh`: surfaces keep the model's own textures.
    pub model: bool,
    /// Animation playback of a loaded model.
    pub anim: Option<crate::anim::AnimState>,
    /// A node inside a loaded model registered by `FindChild`/`GetChild`;
    /// its transform is read back from the node because animation moves it.
    pub adopted: bool,
    /// Set for sprites (`CreateSprite`) and terrains (`CreateTerrain`).
    pub sprite: Option<crate::sprite::SpriteState>,
    pub terrain: Option<crate::terrain::TerrainState>,
    /// Rigid body nodes when `EntityPhysics` was used.
    pub phys: Option<crate::physics::PhysState>,
    /// The model a `LoadMesh` entity was made from (for `CopyEntity`).
    pub model_res: Option<fyrox::resource::model::ModelResource>,
    /// `EntityOrder`: non-zero draws in the forward pass, after normal geometry.
    pub order: i32,
    /// False while the entity's materials are shared (loaded models, copies);
    /// they are cloned before the first per-entity change.
    pub own_materials: bool,
    /// Collision type (`EntityType`), 0 = none.
    pub col_type: i32,
    /// Ellipsoid radii (x/z, y) used when this entity moves or is hit as a sphere.
    pub radius: (f32, f32),
    /// Collision box `(x, y, z, width, height, depth)` in local space.
    pub col_box: Option<[f32; 6]>,
    /// World position at the last `UpdateWorld`/`ResetEntity`.
    pub last_pos: Option<Vector3<f32>>,
    /// Contacts found by the last `UpdateWorld`.
    pub contacts: Vec<crate::collision::Contact>,
    /// `EntityPickMode`: 0 none, 1 sphere, 2 polygon, 3 box.
    pub pick_mode: i32,
    /// Whether this entity blocks `EntityVisible` line-of-sight checks.
    pub obscurer: bool,
}

/// Program-visible state that persists across frames.
#[derive(Debug)]
pub struct World {
    entities: Vec<Option<Entity>>,
    free: Vec<u32>,
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub graphics_requested: bool,
    pub pending_title: Option<String>,
    pub pending_size: Option<(u32, u32)>,
    /// Incremented by `RenderWorld`; `Flip` only ends a frame if nothing was rendered since the last one.
    pub renders: u64,
    pub renders_at_flip: u64,
    pub ambient: Color,
    /// 2D drawing (Text, fonts, colours).
    pub canvas: Canvas2D,
    pub textures: Vec<Option<TextureInfo>>,
    pub images: Vec<Option<crate::image2d::Image2D>>,
    /// `AutoMidHandle`: new images get a centred handle.
    pub auto_mid_handle: bool,
    /// Result of the last pick command.
    pub picked: Option<crate::pick::PickHit>,
    /// Result of the last `CameraProject`.
    pub projected: (f32, f32, f32),
    /// Surfaces opened with `CreateSurface`/`GetSurface` and their CPU geometry.
    pub surfaces: Vec<Option<crate::mesh::SurfaceInfo>>,
    pub brushes: Vec<Option<crate::mesh::Brush>>,
    pub sounds: Vec<Option<crate::sound::SoundInfo>>,
    /// Channel number → sound node (stale handles mean the channel ended).
    pub channels: Vec<Handle<Node>>,
    /// Current drawing buffer (`SetBuffer`), see `buffer.rs`.
    pub buffer: i32,
    /// Renderer quality (ext); `quality_dirty` asks the engine to apply it.
    pub quality: crate::quality::Quality,
    pub quality_dirty: bool,
    /// Physics joints (`CreateBallJoint`, ...).
    pub joints: Vec<Handle<Node>>,
    pub listener: Option<Handle<Node>>,
    pub listener_rolloff: f32,
    pub listener_distance: f32,
    /// Fog and other settings shared by every classic material.
    pub classic: crate::classic::Globals,
    /// `Collisions` rules: (source type, destination type, method, response).
    pub collision_rules: Vec<(i32, i32, i32, crate::collision::Response)>,
}

impl Default for World {
    fn default() -> Self {
        World {
            entities: Vec::new(),
            free: Vec::new(),
            width: 1280,
            height: 720,
            title: "NeptuneBASIC".into(),
            graphics_requested: false,
            pending_title: None,
            pending_size: None,
            renders: 0,
            renders_at_flip: 0,
            ambient: Color::opaque(127, 127, 127),
            canvas: Canvas2D::default(),
            textures: Vec::new(),
            images: Vec::new(),
            auto_mid_handle: false,
            picked: None,
            projected: (0.0, 0.0, 0.0),
            collision_rules: Vec::new(),
            classic: Default::default(),
            surfaces: Vec::new(),
            brushes: Vec::new(),
            sounds: Vec::new(),
            channels: Vec::new(),
            joints: Vec::new(),
            buffer: crate::buffer::BACK,
            quality: Default::default(),
            // apply the starting level once the window exists
            quality_dirty: true,
            listener: None,
            listener_rolloff: 1.0,
            listener_distance: 1.0,
        }
    }
}

/// Initial scene settings matching Blitz3D defaults.
pub fn setup_scene(scene: &mut Scene) {
    // Blitz3D clears to black; Fyrox's default sky would hide CameraClsColor.
    scene.set_skybox(None);
    let opts = scene.rendering_options.get_value_mut_silent();
    opts.clear_color = Some(Color::BLACK);
    opts.environment_lighting_source = EnvironmentLightingSource::AmbientColor;
    opts.ambient_lighting_color = Color::opaque(127, 127, 127);
}

/// The [`Host`] implementation: runs runtime commands against the world.
pub struct Host3D<'a> {
    pub world: &'a mut World,
    pub input: &'a mut Input,
    scene: Option<&'a mut Scene>,
    resources: Option<fyrox::asset::manager::ResourceManager>,
}

impl<'a> Host3D<'a> {
    /// Host for the console phase (before `Graphics3D`).
    pub fn console(world: &'a mut World, input: &'a mut Input) -> Self {
        Host3D { world, input, scene: None, resources: None }
    }
    pub fn graphics(
        world: &'a mut World,
        input: &'a mut Input,
        scene: &'a mut Scene,
        resources: fyrox::asset::manager::ResourceManager,
    ) -> Self {
        Host3D { world, input, scene: Some(scene), resources: Some(resources) }
    }

    pub fn resources(&self) -> Option<fyrox::asset::manager::ResourceManager> {
        self.resources.clone()
    }

    /// Returns a predicate that is true for the nodes of every entity except
    /// `id`; used to keep subtree walks inside one entity.
    pub fn other_entity_nodes(&self, id: i32) -> impl Fn(Handle<Node>) -> bool {
        let set: std::collections::HashSet<Handle<Node>> =
            self.entities().filter(|(i, _)| *i != id).map(|(_, e)| e.node).collect();
        move |h| set.contains(&h)
    }

    /// Triangles of an entity's own meshes in its local Blitz space.
    pub fn mesh_triangles(&mut self, id: i32) -> Result<Vec<[Vector3<f32>; 3]>, String> {
        self.flush_meshes()?;
        let node = self.ent(id)?.node;
        let skip = self.other_entity_nodes(id);
        Ok(crate::model::triangles(self.graph()?, node, &skip))
    }

    pub fn has_graphics(&self) -> bool {
        self.scene.is_some()
    }

    pub fn scene(&mut self) -> Result<&mut Scene, String> {
        self.scene.as_deref_mut().ok_or_else(|| "Graphics3D must be called before using 3D commands".to_string())
    }

    pub fn scene_opt(&mut self) -> Option<&mut Scene> {
        self.scene.as_deref_mut()
    }

    pub fn graph(&mut self) -> Result<&mut Graph, String> {
        Ok(&mut self.scene()?.graph)
    }

    pub fn ent(&self, id: i32) -> Result<&Entity, String> {
        usize::try_from(id - 1)
            .ok()
            .and_then(|i| self.world.entities.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Entity {id} does not exist"))
    }

    pub fn ent_mut(&mut self, id: i32) -> Result<&mut Entity, String> {
        usize::try_from(id - 1)
            .ok()
            .and_then(|i| self.world.entities.get_mut(i))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("Entity {id} does not exist"))
    }

    /// World transform of an entity's *parent frame* (identity for roots).
    pub fn parent_world(&self, id: i32) -> Result<WorldXform, String> {
        let p = self.ent(id)?.parent;
        if p == 0 {
            Ok(WorldXform::identity())
        } else {
            self.world_of(p as i32)
        }
    }

    pub fn world_of(&self, id: i32) -> Result<WorldXform, String> {
        let mut chain = Vec::new();
        let mut cur = id;
        while cur != 0 {
            let e = self.ent(cur)?;
            chain.push(self.live_xf(e));
            cur = e.parent as i32;
            if chain.len() > 10_000 {
                return Err("entity hierarchy is cyclic".into());
            }
        }
        Ok(chain.iter().rev().fold(WorldXform::identity(), |w, xf| w.child(xf)))
    }

    /// Writes an entity's local transform into its Fyrox node.
    pub fn sync(&mut self, id: i32) -> Result<(), String> {
        let e = self.ent(id)?;
        let (node, xf, phys) = (e.node, e.xf, e.phys);
        let graph = self.graph()?;
        if let Some(p) = phys {
            // physics bodies carry position and rotation; the mesh keeps the scale
            let t = graph[p.body].local_transform_mut();
            t.set_position(xf.fyrox_position());
            t.set_rotation(xf.fyrox_rotation());
            graph[node].local_transform_mut().set_scale(xf.scale);
            if p.body_type == 3 {
                if let Some(b) = graph[p.body].cast_mut::<fyrox::scene::rigidbody::RigidBody>() {
                    b.set_next_kinematic_translation(xf.fyrox_position());
                    b.set_next_kinematic_rotation(xf.fyrox_rotation());
                }
            }
            return Ok(());
        }
        let t = graph[node].local_transform_mut();
        t.set_position(xf.fyrox_position());
        t.set_rotation(xf.fyrox_rotation());
        t.set_scale(xf.scale);
        Ok(())
    }

    /// Local transform; for nodes inside animated models, the current one.
    pub fn live_xf(&self, e: &Entity) -> Xform {
        match (&self.scene, e.adopted, e.phys) {
            (Some(scene), _, Some(p)) => {
                let mut xf = crate::anim::xform_of_node(&scene.graph, p.body);
                xf.scale = e.xf.scale;
                xf
            }
            (Some(scene), true, None) => crate::anim::xform_of_node(&scene.graph, e.node),
            _ => e.xf,
        }
    }

    pub fn set_xf(&mut self, id: i32, f: impl FnOnce(&mut Xform)) -> Result<(), String> {
        let live = self.live_xf(self.ent(id)?);
        let e = self.ent_mut(id)?;
        e.xf = live;
        f(&mut e.xf);
        self.sync(id)
    }

    /// Adds a node to the scene as a new entity, optionally parented.
    pub fn add_entity(&mut self, node: Node, kind: Kind, parent: i32) -> Result<i32, String> {
        if parent != 0 {
            self.ent(parent)?;
        }
        let handle = self.graph()?.add_node(node);
        self.attach_entity(handle, kind, parent)
    }

    /// Registers an existing scene node as a new entity.
    pub fn attach_entity(&mut self, handle: Handle<Node>, kind: Kind, parent: i32) -> Result<i32, String> {
        self.attach(handle, kind, parent, true)
    }

    /// Registers a node that is already linked under the parent entity's node,
    /// keeping its current transform.
    pub fn attach_entity_linked(&mut self, handle: Handle<Node>, kind: Kind, parent: i32) -> Result<i32, String> {
        self.attach(handle, kind, parent, false)
    }

    fn attach(&mut self, handle: Handle<Node>, kind: Kind, parent: i32, link: bool) -> Result<i32, String> {
        if parent != 0 {
            self.ent(parent)?;
        }
        let entity = Entity {
            node: handle,
            kind,
            parent: parent as u32,
            children: Vec::new(),
            xf: Xform::default(),
            name: String::new(),
            color: Color::WHITE,
            hidden: false,
            textures: [0; crate::classic::MAX_LAYERS],
            normal_map: 0,
            shininess: 0.0,
            fx: 0,
            blend: crate::classic::BLEND_ALPHA,
            originals: Vec::new(),
            order: 0,
            model: false,
            anim: None,
            adopted: false,
            model_res: None,
            phys: None,
            sprite: None,
            terrain: None,
            own_materials: true,
            col_type: 0,
            radius: (1.0, 1.0),
            col_box: None,
            last_pos: None,
            contacts: Vec::new(),
            pick_mode: 0,
            obscurer: false,
        };
        let id = match self.world.free.pop() {
            Some(i) => {
                self.world.entities[i as usize] = Some(entity);
                i as i32 + 1
            }
            None => {
                self.world.entities.push(Some(entity));
                self.world.entities.len() as i32
            }
        };
        if parent != 0 {
            if link {
                let pnode = self.ent(parent)?.node;
                self.graph()?.link_nodes(handle, pnode);
            }
            self.ent_mut(parent)?.children.push(id as u32);
        }
        if link {
            self.sync(id)?;
        }
        Ok(id)
    }

    /// Removes an entity and all of its children.
    pub fn free_entity(&mut self, id: i32) -> Result<(), String> {
        let (node, parent) = {
            let e = self.ent(id)?;
            // a physics entity's root node is its body
            (e.phys.map_or(e.node, |p| p.body), e.parent)
        };
        if parent != 0 {
            if let Ok(p) = self.ent_mut(parent as i32) {
                p.children.retain(|&c| c != id as u32);
            }
        }
        let mut stack = vec![id as u32];
        while let Some(i) = stack.pop() {
            if let Some(e) = self.world.entities[(i - 1) as usize].take() {
                stack.extend(e.children);
                self.world.free.push(i - 1);
            }
        }
        self.graph()?.remove_node(node);
        self.prune_surfaces();
        self.forget_channels();
        Ok(())
    }

    /// Re-parents an entity. With `global`, its world transform is preserved.
    pub fn set_parent(&mut self, id: i32, parent: i32, global: bool) -> Result<(), String> {
        if parent != 0 && self.ent(id)?.phys.is_some() {
            return Err(format!("Entity {id} has physics; physics bodies move in world space and cannot have a parent"));
        }
        if parent != 0 {
            // reject cycles
            let mut cur = parent;
            while cur != 0 {
                if cur == id {
                    return Err("EntityParent would create a cycle".into());
                }
                cur = self.ent(cur)?.parent as i32;
            }
        }
        let world = self.world_of(id)?;
        let old = self.ent(id)?.parent;
        if old != 0 {
            self.ent_mut(old as i32)?.children.retain(|&c| c != id as u32);
        }
        self.ent_mut(id)?.parent = parent as u32;
        let node = self.ent(id)?.node;
        if parent != 0 {
            self.ent_mut(parent)?.children.push(id as u32);
            let pnode = self.ent(parent)?.node;
            self.graph()?.link_nodes(node, pnode);
        } else {
            let root = self.graph()?.get_root();
            self.graph()?.link_nodes(node, root);
        }
        if global {
            let pw = self.parent_world(id)?;
            let pos = pw.to_local_point(&world.position());
            let rot = pw.rot.transpose() * world.rot;
            let scale = world.scale.component_div(&pw.scale);
            self.set_xf(id, |xf| {
                xf.pos = pos;
                xf.rot = rot;
                xf.scale = scale;
            })?;
        }
        Ok(())
    }

    /// Applies `f` to the material of every surface of a mesh entity.
    /// Applies `f` to the material of every surface of an entity's meshes
    /// (including the meshes inside a loaded model, but not child entities).
    pub fn with_materials(&mut self, id: i32, mut f: impl FnMut(&mut Material)) -> Result<(), String> {
        let (node, own) = {
            let e = self.ent(id)?;
            (e.node, e.own_materials)
        };
        let skip = self.other_entity_nodes(id);
        let graph = self.graph()?;
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            stack.extend(graph[n].children().iter().copied().filter(|c| !skip(*c)));
            if let Some(mesh) = graph[n].cast_mut::<Mesh>() {
                for s in mesh.surfaces_mut() {
                    if !own {
                        let copy = s.material().data_ref().clone();
                        s.set_material(MaterialResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, copy));
                    }
                    let mut m = s.material().data_ref();
                    f(&mut m);
                }
            }
        }
        self.ent_mut(id)?.own_materials = true;
        Ok(())
    }

    /// Copies an entity and its child entities (`CopyEntity`).
    pub fn copy_entity(&mut self, id: i32, parent: i32) -> Result<i32, String> {
        if let Some(res) = self.ent(id)?.model_res.clone() {
            // animated models need their own skeleton and animation player
            let new = crate::model::instantiate(self, &res, parent)?;
            let (xf, name, hidden) = {
                let e = self.ent(id)?;
                (e.xf, e.name.clone(), e.hidden)
            };
            let copied = {
                let e = self.ent(id)?;
                (e.col_type, e.radius, e.col_box, e.pick_mode, e.obscurer, e.color, e.textures, e.shininess, e.fx, e.blend)
            };
            {
                let e = self.ent_mut(new)?;
                e.xf = xf;
                e.name = name;
                (e.col_type, e.radius, e.col_box, e.pick_mode, e.obscurer, e.color, e.textures, e.shininess, e.fx, e.blend) = copied;
            }
            self.sync(new)?;
            if hidden {
                self.ent_mut(new)?.hidden = true;
                let n = self.ent(new)?.node;
                self.graph()?[n].set_visibility(false);
            }
            self.refresh_material(new)?;
            return Ok(new);
        }
        let (node, kind, xf, name, color, hidden, children) = {
            let e = self.ent(id)?;
            (e.node, e.kind, self.live_xf(e), e.name.clone(), e.color, e.hidden, e.children.clone())
        };
        let (sprite, terrain, phys) = {
            let e = self.ent(id)?;
            (e.sprite, e.terrain.clone(), e.phys)
        };
        let look = {
            let e = self.ent(id)?;
            (e.textures, e.normal_map, e.shininess, e.fx, e.blend, e.order)
        };
        let src_col = {
            let e = self.ent(id)?;
            (e.col_type, e.radius, e.col_box, e.pick_mode, e.obscurer)
        };
        let child_nodes: std::collections::HashSet<Handle<Node>> =
            children.iter().filter_map(|&c| self.ent(c as i32).ok().map(|e| e.node)).collect();
        // the light node of a light entity is a plain child, so it is copied along
        let graph = self.graph()?;
        let new_node = crate::model::copy_tree(graph, node, &|h| child_nodes.contains(&h));
        let kind = match kind {
            Kind::Light { kind: lk, .. } => {
                let light = graph[new_node].children().first().copied().unwrap_or_default();
                Kind::Light { kind: lk, light }
            }
            k => k,
        };
        let new = self.attach_entity(new_node, kind, parent)?;
        {
            let e = self.ent_mut(new)?;
            e.xf = xf;
            e.name = name;
            e.color = color;
            e.hidden = hidden;
            (e.textures, e.normal_map, e.shininess, e.fx, e.blend, e.order) = look;
            e.own_materials = false;
            e.col_type = src_col.0;
            e.radius = src_col.1;
            e.col_box = src_col.2;
            e.pick_mode = src_col.3;
            e.obscurer = src_col.4;
        }
        // the copy shares materials with the original, so neither owns them now
        self.ent_mut(id)?.own_materials = false;
        self.sync(new)?;
        // sprites and terrains edit their own surface, not the original's
        if let Some(mut s) = sprite {
            s.surface = self.surface_handle(new, 0)?;
            self.ent_mut(new)?.sprite = Some(s);
        }
        if let Some(mut t) = terrain {
            t.surface = self.surface_handle(new, 0)?;
            self.ent_mut(new)?.terrain = Some(t);
        }
        for c in children {
            self.copy_entity(c as i32, new)?;
        }
        if let Some(p) = phys {
            self.copy_physics(id, new, p)?;
        }
        Ok(new)
    }

    /// Recomputes a camera's vertical field of view from its Blitz zoom.
    pub fn update_camera_fov(&mut self, id: i32) -> Result<(), String> {
        let (node, kind) = {
            let e = self.ent(id)?;
            (e.node, e.kind)
        };
        let Kind::Camera { zoom, mode } = kind else { return Err(format!("Entity {id} is not a camera")) };
        let aspect = self.world.width as f32 / self.world.height.max(1) as f32;
        let graph = self.graph()?;
        if let Some(cam) = graph[node].cast_mut::<fyrox::scene::camera::Camera>() {
            let (near, far) = (cam.projection().z_near(), cam.projection().z_far());
            if mode == 2 {
                // Orthographic: the view is 2/zoom units wide, like Blitz3D
                // (so programs use small zooms such as 0.05 to see a scene).
                let half_h = 1.0 / (zoom.max(1e-6) * aspect);
                match cam.projection_mut() {
                    Projection::Orthographic(o) => o.vertical_size = half_h,
                    _ => {
                        cam.set_projection(Projection::Orthographic(fyrox::scene::camera::OrthographicProjection {
                            z_near: near,
                            z_far: far,
                            vertical_size: half_h,
                        }));
                    }
                }
            } else {
                // Blitz zoom defines the horizontal FOV: hfov = 2*atan(1/zoom)
                let fov = 2.0 * (1.0 / (zoom.max(0.0001) * aspect)).atan();
                match cam.projection_mut() {
                    Projection::Perspective(p) => p.fov = fov,
                    _ => {
                        cam.set_projection(Projection::Perspective(PerspectiveProjection { fov, z_near: near, z_far: far }));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn image(&self, id: i32) -> Result<&crate::image2d::Image2D, String> {
        usize::try_from(id - 1)
            .ok()
            .and_then(|i| self.world.images.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Image {id} does not exist"))
    }

    pub fn image_mut(&mut self, id: i32) -> Result<&mut crate::image2d::Image2D, String> {
        usize::try_from(id - 1)
            .ok()
            .and_then(|i| self.world.images.get_mut(i))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("Image {id} does not exist"))
    }

    pub fn texture_mut(&mut self, id: i32) -> Result<&mut TextureInfo, String> {
        usize::try_from(id - 1)
            .ok()
            .and_then(|i| self.world.textures.get_mut(i))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("Texture {id} does not exist"))
    }

    pub fn texture(&self, id: i32) -> Result<&TextureInfo, String> {
        usize::try_from(id - 1)
            .ok()
            .and_then(|i| self.world.textures.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Texture {id} does not exist"))
    }

    pub fn entities_mut(&mut self) -> impl Iterator<Item = (i32, &mut Entity)> {
        self.world.entities.iter_mut().enumerate().filter_map(|(i, e)| e.as_mut().map(|e| (i as i32 + 1, e)))
    }

    pub fn entities(&self) -> impl Iterator<Item = (i32, &Entity)> {
        self.world.entities.iter().enumerate().filter_map(|(i, e)| e.as_ref().map(|e| (i as i32 + 1, e)))
    }
}

impl Host for Host3D<'_> {
    fn call(&mut self, id: u32, args: &[Value], ctx: &mut NativeCtx) -> Result<Value, String> {
        (crate::commands::table()[id as usize].1)(self, args, ctx)
    }
}

// ----------------------------------------------------------------------
// node construction

pub fn new_material(color: Color) -> MaterialResource {
    let mut m = Material::standard();
    m.set_property("diffuseColor", color);
    MaterialResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, m)
}

pub fn mesh_node(data: SurfaceData, name: &str) -> Node {
    let surface = SurfaceBuilder::new(SurfaceResource::new_embedded(data)).with_material(new_material(Color::WHITE)).build();
    MeshBuilder::new(BaseBuilder::new().with_name(name)).with_surfaces(vec![surface]).build_node()
}

pub fn pivot_node() -> Node {
    PivotBuilder::new(BaseBuilder::new().with_name("Pivot")).build_node()
}

pub fn camera_node() -> Node {
    CameraBuilder::new(BaseBuilder::new().with_name("Camera"))
        .with_projection(Projection::Perspective(PerspectiveProjection { fov: 75f32.to_radians(), z_near: 1.0, z_far: 1000.0 }))
        .build_node()
}

/// Builds the Fyrox light node for a light entity. Fyrox lights shine along
/// their local -Y; rotating -90° about X makes them shine along +Z like Blitz.
pub fn light_node(kind: LightKind) -> Node {
    let base = BaseBuilder::new().with_name("Light").with_local_transform(
        TransformBuilder::new()
            .with_local_rotation(UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -std::f32::consts::FRAC_PI_2))
            .build(),
    );
    let light = BaseLightBuilder::new(base).with_color(Color::WHITE);
    match kind {
        LightKind::Directional => DirectionalLightBuilder::new(light).build_node(),
        LightKind::Point => PointLightBuilder::new(light).with_radius(1000.0).build_node(),
        LightKind::Spot => SpotLightBuilder::new(light)
            .with_distance(1000.0)
            .with_hotspot_cone_angle(0.0f32.to_radians())
            .with_falloff_angle_delta(90.0f32.to_radians())
            .build_node(),
    }
}

/// Geometry for Blitz primitives, built in Blitz space (see `primitive.rs`).
pub fn primitive(kind: &str, segments: i32, solid: bool) -> SurfaceData {
    use crate::primitive as p;
    let seg = segments.clamp(3, 100) as usize;
    let b = match kind {
        "cube" => p::cube(),
        "sphere" => p::sphere(seg),
        "cylinder" => p::cylinder(seg, solid),
        "cone" => p::cone(seg, solid),
        // Blitz planes are infinite; approximate with a large quad facing +Y
        _ => p::plane(10_000.0),
    };
    crate::mesh::surface_data(&b.verts, &b.tris)
}
