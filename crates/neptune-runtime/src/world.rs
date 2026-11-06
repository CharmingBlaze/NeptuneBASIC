//! Entity bookkeeping on top of the Fyrox scene graph.
//!
//! An entity handle (the `Int` a program holds) indexes this table; each
//! entry owns one Fyrox node and its Blitz-space local transform.

use fyrox::{
    asset::untyped::ResourceKind,
    core::{
        algebra::{Matrix4, UnitQuaternion, Vector3},
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

use crate::commands::COMMANDS;
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
    Camera { zoom: f32 },
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
}

impl<'a> Host3D<'a> {
    /// Host for the console phase (before `Graphics3D`).
    pub fn console(world: &'a mut World, input: &'a mut Input) -> Self {
        Host3D { world, input, scene: None }
    }
    pub fn graphics(world: &'a mut World, input: &'a mut Input, scene: &'a mut Scene) -> Self {
        Host3D { world, input, scene: Some(scene) }
    }

    pub fn has_graphics(&self) -> bool {
        self.scene.is_some()
    }

    pub fn scene(&mut self) -> Result<&mut Scene, String> {
        self.scene.as_deref_mut().ok_or_else(|| "Graphics3D must be called before using 3D commands".to_string())
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
            chain.push(e.xf);
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
        let (node, xf) = (e.node, e.xf);
        let graph = self.graph()?;
        let t = graph[node].local_transform_mut();
        t.set_position(xf.fyrox_position());
        t.set_rotation(xf.fyrox_rotation());
        t.set_scale(xf.scale);
        Ok(())
    }

    pub fn set_xf(&mut self, id: i32, f: impl FnOnce(&mut Xform)) -> Result<(), String> {
        f(&mut self.ent_mut(id)?.xf);
        self.sync(id)
    }

    /// Adds a node to the scene as a new entity, optionally parented.
    pub fn add_entity(&mut self, node: Node, kind: Kind, parent: i32) -> Result<i32, String> {
        if parent != 0 {
            self.ent(parent)?;
        }
        let graph = self.graph()?;
        let handle = graph.add_node(node);
        let entity = Entity {
            node: handle,
            kind,
            parent: parent as u32,
            children: Vec::new(),
            xf: Xform::default(),
            name: String::new(),
            color: Color::WHITE,
            hidden: false,
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
            let pnode = self.ent(parent)?.node;
            self.graph()?.link_nodes(handle, pnode);
            self.ent_mut(parent)?.children.push(id as u32);
        }
        self.sync(id)?;
        Ok(id)
    }

    /// Removes an entity and all of its children.
    pub fn free_entity(&mut self, id: i32) -> Result<(), String> {
        let (node, parent) = {
            let e = self.ent(id)?;
            (e.node, e.parent)
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
        Ok(())
    }

    /// Re-parents an entity. With `global`, its world transform is preserved.
    pub fn set_parent(&mut self, id: i32, parent: i32, global: bool) -> Result<(), String> {
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
    pub fn with_materials(&mut self, id: i32, mut f: impl FnMut(&mut Material)) -> Result<(), String> {
        let node = self.ent(id)?.node;
        let graph = self.graph()?;
        if let Some(mesh) = graph[node].cast_mut::<Mesh>() {
            for s in mesh.surfaces_mut() {
                let mut m = s.material().data_ref();
                f(&mut m);
            }
        }
        Ok(())
    }

    /// Recomputes a camera's vertical field of view from its Blitz zoom.
    pub fn update_camera_fov(&mut self, id: i32) -> Result<(), String> {
        let (node, kind) = {
            let e = self.ent(id)?;
            (e.node, e.kind)
        };
        let Kind::Camera { zoom } = kind else { return Err(format!("Entity {id} is not a camera")) };
        let aspect = self.world.width as f32 / self.world.height.max(1) as f32;
        let graph = self.graph()?;
        if let Some(cam) = graph[node].cast_mut::<fyrox::scene::camera::Camera>() {
            // Blitz zoom defines the horizontal FOV: hfov = 2*atan(1/zoom)
            let fov = 2.0 * (1.0 / (zoom.max(0.0001) * aspect)).atan();
            if let Projection::Perspective(p) = cam.projection_mut() {
                p.fov = fov;
            }
        }
        Ok(())
    }

    pub fn entities(&self) -> impl Iterator<Item = (i32, &Entity)> {
        self.world.entities.iter().enumerate().filter_map(|(i, e)| e.as_ref().map(|e| (i as i32 + 1, e)))
    }
}

impl Host for Host3D<'_> {
    fn call(&mut self, id: u32, args: &[Value], ctx: &mut NativeCtx) -> Result<Value, String> {
        (COMMANDS[id as usize].1)(self, args, ctx)
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

/// Geometry for Blitz primitives, generated directly in Fyrox space with Blitz dimensions.
pub fn primitive(kind: &str, segments: i32, solid: bool) -> SurfaceData {
    let seg = segments.clamp(3, 100) as usize;
    match kind {
        "cube" => SurfaceData::make_cube(Matrix4::new_scaling(2.0)),
        "sphere" => SurfaceData::make_sphere(seg * 2, seg, 1.0, &Matrix4::identity()),
        "cylinder" => SurfaceData::make_cylinder(seg, 1.0, 2.0, solid, &Matrix4::new_translation(&Vector3::new(0.0, -1.0, 0.0))),
        "cone" => SurfaceData::make_cone(seg, 1.0, 2.0, &Matrix4::new_translation(&Vector3::new(0.0, -1.0, 0.0))),
        _ => {
            // Blitz planes are infinite; approximate with a large quad facing +Y.
            let m = Matrix4::from_scaled_axis(Vector3::x() * std::f32::consts::FRAC_PI_2) * Matrix4::new_scaling(10_000.0);
            SurfaceData::make_quad(&m)
        }
    }
}
