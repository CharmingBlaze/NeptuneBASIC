//! Levels made in Blender or any glTF tool (ext): `LoadLevelFile`.
//!
//! Fyrox's glTF importer brings in meshes, materials, empties and animation,
//! but turns lights and cameras into empty nodes and drops custom properties.
//! `LoadLevelFile` loads the model as `LoadMesh` does, then reads the file's JSON
//! itself and adds what was lost: real lights and cameras, each object's
//! custom properties, solid objects and prefabs.
//!
//! The glTF nodes are matched to the instance's nodes by walking both trees
//! by name. Only objects that need something get an entity, so a level of
//! thousands of plain meshes costs what `LoadMesh` costs.
//!
//! A Fyrox scene file (`.rgs`, from Neptune Level Editor) keeps its lights,
//! cameras and properties, so its nodes are read from the loaded scene
//! (`scene_nodes`) and then treated like a glTF file's.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use fyrox::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        color::Color,
        futures::executor::block_on,
        pool::Handle,
    },
    graph::SceneGraph,
    resource::model::Model,
    scene::{
        base::PropertyValue,
        camera::{Camera, Projection},
        graph::Graph,
        light::{directional::DirectionalLight, point::PointLight, spot::SpotLight},
        node::Node,
    },
};

use crate::json::{self, Json};
use crate::physics::{SHAPE_AUTO, SHAPE_BOX, SHAPE_CAPSULE, SHAPE_CYLINDER, SHAPE_HULL, SHAPE_MESH, SHAPE_SPHERE};
use crate::transform::rotation;
use crate::world::{pivot_node, Host3D, Kind, LightKind};

/// A light from `KHR_lights_punctual`, in the units `Host3D` lights use.
#[derive(Debug, Clone, PartialEq)]
pub struct LightDef {
    pub kind: LightKind,
    /// Linear colour, as stored in glTF.
    pub color: [f32; 3],
    pub intensity: f32,
    /// Point and spot lights: distance at which the light ends.
    pub range: f32,
    /// Spot lights: full cone angles in radians.
    pub cone: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraDef {
    pub zoom: f32,
    pub near: f32,
    pub far: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LevelNode {
    pub name: String,
    pub children: Vec<usize>,
    pub props: Vec<(String, String)>,
    pub light: Option<LightDef>,
    pub camera: Option<CameraDef>,
}

impl LevelNode {
    /// Whether the node gets an entity.
    fn matters(&self) -> bool {
        !self.props.is_empty() || self.light.is_some() || self.camera.is_some() || solid_prefix(&self.name).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LevelFile {
    pub nodes: Vec<LevelNode>,
    /// Top-level nodes of the file's default scene.
    pub roots: Vec<usize>,
}

/// Point and spot lights keep this brightness; their strength sets how far
/// they reach. Fyrox's lights fade out over a set distance instead of by
/// the inverse square, so this matches an inverse-square light at a quarter
/// of its reach: `E = I / d²`, `range² = I / LAMP_CUTOFF`.
const LAMP_BRIGHTNESS: f32 = 0.85 * LIGHT_GAIN;
/// Fyrox's tone mapping shows the same light darker than Blender's Standard
/// view. Measured by rendering one level in both.
const LIGHT_GAIN: f32 = 2.5;
/// Illumination at which a lamp's light is treated as ended.
const LAMP_CUTOFF: f32 = 0.05;
/// Blender writes lamp power with this many lumens per watt.
const LUMENS_PER_WATT: f32 = 683.0;

/// The JSON part of a `.gltf` or `.glb` file.
fn json_text(bytes: &[u8]) -> Result<&str, String> {
    let chunk = if bytes.starts_with(b"glTF") {
        let word = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let len = word(12).ok_or("truncated GLB header")? as usize;
        if word(16) != Some(0x4E4F_534A) {
            return Err("GLB file has no JSON chunk".into());
        }
        bytes.get(20..20 + len).ok_or("truncated GLB file")?
    } else {
        bytes
    };
    std::str::from_utf8(chunk).map_err(|_| "file is not UTF-8 JSON".to_string())
}

/// A custom property's value as text; booleans become `1` and `0`.
pub(crate) fn prop_text(v: &Json) -> String {
    match v {
        Json::Str(s) => s.clone(),
        Json::Bool(b) => (if *b { "1" } else { "0" }).to_string(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}

fn float(v: &Json) -> Option<f32> {
    v.num().map(|n| n as f32)
}

fn parse_light(l: &Json, blender: bool) -> Option<LightDef> {
    let col = l.get("color").arr();
    let color = if col.len() == 3 { [float(&col[0])?, float(&col[1])?, float(&col[2])?] } else { [1.0; 3] };
    let raw = float(l.get("intensity")).unwrap_or(1.0);
    // Blender's "Standard" lighting mode writes the sun in lux and lamps in
    // candela at 683 lm/W, without the factor of pi its own renderer divides by
    let (sun, lamp) = if blender { (raw / LUMENS_PER_WATT, raw / (LUMENS_PER_WATT * std::f32::consts::PI)) } else { (raw, raw) };
    let reach = float(l.get("range")).unwrap_or_else(|| (lamp / LAMP_CUTOFF).sqrt()).max(0.25);
    let spot = l.get("spot");
    let outer = float(spot.get("outerConeAngle")).unwrap_or(std::f32::consts::FRAC_PI_4);
    let inner = float(spot.get("innerConeAngle")).unwrap_or(0.0).min(outer);
    let cone = (inner * 2.0, outer * 2.0);
    let (kind, intensity) = match l.get("type").str()? {
        "directional" => (LightKind::Directional, sun * LIGHT_GAIN),
        "point" => (LightKind::Point, LAMP_BRIGHTNESS),
        "spot" => (LightKind::Spot, LAMP_BRIGHTNESS),
        _ => return None,
    };
    Some(LightDef { kind, color, intensity, range: reach, cone })
}

fn parse_camera(c: &Json) -> Option<CameraDef> {
    let (zoom, spec) = match c.get("type").str()? {
        "perspective" => {
            let p = c.get("perspective");
            // Blitz zoom is set by the horizontal field of view
            let aspect = float(p.get("aspectRatio")).unwrap_or(16.0 / 9.0);
            (1.0 / ((float(p.get("yfov"))? / 2.0).tan() * aspect), p)
        }
        "orthographic" => {
            let o = c.get("orthographic");
            (1.0 / float(o.get("xmag"))?, o)
        }
        _ => return None,
    };
    Some(CameraDef { zoom, near: float(spec.get("znear")).unwrap_or(0.1), far: float(spec.get("zfar")) })
}

/// Reads what `LoadLevelFile` needs from the bytes of a `.gltf` or `.glb` file.
pub fn parse_level(bytes: &[u8]) -> Result<LevelFile, String> {
    let doc = json::parse(json_text(bytes)?)?;
    let blender = doc.at("asset.generator").str().is_some_and(|g| g.contains("Blender"));
    let lights: Vec<Option<LightDef>> =
        doc.at("extensions.KHR_lights_punctual.lights").arr().iter().map(|l| parse_light(l, blender)).collect();
    let cameras: Vec<Option<CameraDef>> = doc.get("cameras").arr().iter().map(parse_camera).collect();
    let index = |v: &Json| v.num().map(|n| n as usize);
    let nodes: Vec<LevelNode> = doc
        .get("nodes")
        .arr()
        .iter()
        .map(|n| LevelNode {
            name: n.get("name").str().unwrap_or("").to_string(),
            children: n.get("children").arr().iter().filter_map(index).collect(),
            props: match n.get("extras") {
                Json::Obj(m) => m.iter().map(|(k, v)| (k.clone(), prop_text(v))).collect(),
                _ => Vec::new(),
            },
            light: index(n.at("extensions.KHR_lights_punctual.light")).and_then(|i| lights.get(i).cloned().flatten()),
            camera: index(n.get("camera")).and_then(|i| cameras.get(i).cloned().flatten()),
        })
        .collect();
    let scenes = doc.get("scenes").arr();
    let scene = scenes.get(index(doc.get("scene")).unwrap_or(0));
    let roots = scene.map_or(&[][..], |s| s.get("nodes").arr()).iter().filter_map(index).filter(|&i| i < nodes.len()).collect();
    Ok(LevelFile { nodes, roots })
}

/// A property by name, ignoring case.
pub fn prop<'a>(props: &'a [(String, String)], name: &str) -> Option<&'a str> {
    props.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// Text as an integer, Blitz style (`2.5` rounds to `2`).
pub fn text_int(s: &str) -> Option<i32> {
    s.trim().parse::<f64>().ok().map(|f| f.round_ties_even() as i32)
}

fn text_float(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok()
}

/// `Some(visible)` for the object name prefixes that make an object solid.
/// Prefixes, because Blender's numbered copies (`col_Wall.001`) keep them.
pub fn solid_prefix(name: &str) -> Option<bool> {
    let head = name.get(..4)?;
    if head.eq_ignore_ascii_case("col_") {
        Some(true)
    } else if head.eq_ignore_ascii_case("hit_") {
        Some(false)
    } else {
        None
    }
}

/// Where a `prefab` property's file is: beside the level, or from the game
/// folder. `exists` lets packaging check against its own root.
pub fn prefab_path(level: &str, value: &str, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let beside = Path::new(level).parent().unwrap_or(Path::new("")).join(value);
    if exists(&beside) {
        return Some(beside);
    }
    let direct = PathBuf::from(value);
    exists(&direct).then_some(direct)
}

/// Whether a file holds glTF, which `LoadLevelFile` can read properties from.
pub fn is_gltf(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|x| x.eq_ignore_ascii_case("glb") || x.eq_ignore_ascii_case("gltf"))
}

/// Whether a file is a Fyrox scene, as Neptune Level Editor saves.
pub fn is_scene(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|x| x.eq_ignore_ascii_case("rgs"))
}

/// The `prefab` properties of a Fyrox scene saved as text, which is how
/// Neptune Level Editor saves. Packaging reads them without loading the scene.
pub fn scene_prefabs(text: &str) -> Vec<String> {
    const NAME: &str = "name<str:\"prefab\">";
    const TEXT: &str = "<str:\"";
    // lower case keeps every byte where it was
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find(NAME) {
        at += i + NAME.len();
        // the value is inside the item: `{ Value[Id<u32:2>0<str:"Tree.glb">]{} }`
        let item = &text[at..];
        let item = &item[..item.find('}').unwrap_or(item.len())];
        if let Some(value) = item.find(TEXT).map(|v| &item[v + TEXT.len()..]) {
            if let Some(end) = value.find("\">") {
                out.push(value[..end].to_string());
            }
        }
    }
    out
}

/// What an object of a Fyrox scene file already is. A glTF file's lights and
/// cameras are empty nodes instead, described by `LevelNode`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Native {
    Plain,
    Light(LightKind),
    Camera,
}

/// A custom property of a scene node as text. Handles of other nodes mean
/// nothing to a program and read as empty.
fn property_text(v: &PropertyValue) -> String {
    match v {
        PropertyValue::String(s) => s.clone(),
        PropertyValue::I64(n) => n.to_string(),
        PropertyValue::U64(n) => n.to_string(),
        PropertyValue::I32(n) => n.to_string(),
        PropertyValue::U32(n) => n.to_string(),
        PropertyValue::I16(n) => n.to_string(),
        PropertyValue::U16(n) => n.to_string(),
        PropertyValue::I8(n) => n.to_string(),
        PropertyValue::U8(n) => n.to_string(),
        PropertyValue::F32(n) => n.to_string(),
        PropertyValue::F64(n) => n.to_string(),
        PropertyValue::NodeHandle(_) | PropertyValue::Handle(_) => String::new(),
    }
}

/// The objects of a loaded Fyrox scene that get an entity, parents before
/// children. A model placed in the scene is one object: what is inside it
/// belongs to its own file.
fn scene_nodes(graph: &Graph, instance: Handle<Node>) -> Vec<(LevelNode, Handle<Node>, Native)> {
    let mut out = Vec::new();
    let mut stack: Vec<Handle<Node>> = graph[instance].children().iter().rev().copied().collect();
    while let Some(handle) = stack.pop() {
        let n = &graph[handle];
        let native = if n.cast::<DirectionalLight>().is_some() {
            Native::Light(LightKind::Directional)
        } else if n.cast::<PointLight>().is_some() {
            Native::Light(LightKind::Point)
        } else if n.cast::<SpotLight>().is_some() {
            Native::Light(LightKind::Spot)
        } else if n.cast::<Camera>().is_some() {
            Native::Camera
        } else {
            Native::Plain
        };
        let node = LevelNode {
            name: n.name().to_string(),
            props: n.properties.iter().map(|p| (p.name.clone(), property_text(&p.value))).collect(),
            ..Default::default()
        };
        if native != Native::Plain || node.matters() {
            out.push((node, handle, native));
        }
        if !n.is_resource_instance_root() {
            stack.extend(n.children().iter().rev().copied());
        }
    }
    out
}

fn body_type(name: &str, text: &str) -> Result<i32, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "dynamic" | "1" => Ok(1),
        "static" | "2" => Ok(2),
        "kinematic" | "3" => Ok(3),
        _ => Err(format!("LoadLevelFile: object '{name}': physics must be static, dynamic or kinematic, not '{text}'")),
    }
}

fn body_shape(name: &str, text: &str) -> Result<i32, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "auto" | "" => Ok(SHAPE_AUTO),
        "box" => Ok(SHAPE_BOX),
        "sphere" => Ok(SHAPE_SPHERE),
        "capsule" => Ok(SHAPE_CAPSULE),
        "cylinder" => Ok(SHAPE_CYLINDER),
        "mesh" => Ok(SHAPE_MESH),
        "hull" => Ok(SHAPE_HULL),
        _ => Err(format!("LoadLevelFile: object '{name}': unknown shape '{text}'")),
    }
}

/// The child of `parent` that a glTF node became. Fyrox renames clashing
/// nodes to `name+something`.
fn match_child(graph: &Graph, parent: Handle<Node>, name: &str, used: &mut HashSet<Handle<Node>>) -> Option<Handle<Node>> {
    let kids = graph[parent].children();
    let renamed = format!("{name}+");
    let found = kids
        .iter()
        .copied()
        .find(|k| !used.contains(k) && graph[*k].name() == name)
        .or_else(|| kids.iter().copied().find(|k| !used.contains(k) && graph[*k].name().starts_with(&renamed)))?;
    used.insert(found);
    Some(found)
}

/// Each file node with its scene node, parents before children.
fn locate_nodes(graph: &Graph, instance: Handle<Node>, file: &LevelFile) -> Vec<(usize, Handle<Node>)> {
    let mut used = HashSet::new();
    let mut out = Vec::new();
    let mut seen = vec![false; file.nodes.len()];
    let mut stack: Vec<(usize, Handle<Node>)> = Vec::new();
    let mut push = |parent: Handle<Node>, list: &[usize], stack: &mut Vec<(usize, Handle<Node>)>| {
        // reversed, so the stack gives them back in file order
        let found: Vec<_> = list
            .iter()
            .filter_map(|&i| file.nodes.get(i).and_then(|n| match_child(graph, parent, &n.name, &mut used)).map(|h| (i, h)))
            .collect();
        stack.extend(found.into_iter().rev());
    };
    push(instance, &file.roots, &mut stack);
    while let Some((i, h)) = stack.pop() {
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        out.push((i, h));
        push(h, &file.nodes[i].children, &mut stack);
    }
    out
}

/// Linear glTF colour channel to the sRGB byte lights are given in.
fn srgb_byte(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// glTF lights and cameras look along -Z, Blitz ones along +Z.
fn face_backwards(h: &mut Host3D, e: i32) -> Result<(), String> {
    h.set_xf(e, |xf| xf.rot = rotation(0.0, 180.0, 0.0))
}

/// Keeps `name` for the new entity, so `FindChild` finds the light or camera
/// and not the empty node holding it.
fn take_name(h: &mut Host3D, holder: Handle<Node>, e: i32, name: &str) -> Result<(), String> {
    let node = h.ent(e)?.node;
    let graph = h.graph()?;
    graph[holder].set_name(format!("{name}.node"));
    graph[node].set_name(name);
    h.ent_mut(e)?.name = name.to_string();
    Ok(())
}

fn add_light(h: &mut Host3D, holder: i32, node: &LevelNode, def: &LightDef) -> Result<i32, String> {
    let e = crate::commands::create_light(h, def.kind, holder)?;
    face_backwards(h, e)?;
    let holder_node = h.ent(holder)?.node;
    take_name(h, holder_node, e, &node.name)?;
    let Kind::Light { kind, light } = h.ent(e)?.kind else { return Err("light missing".into()) };
    let number = |name: &str, default: f32| prop(&node.props, name).and_then(text_float).unwrap_or(default);
    let (intensity, range) = (number("intensity", def.intensity), number("range", def.range));
    let shadows = number("shadows", 1.0) != 0.0;
    let color = Color::opaque(srgb_byte(def.color[0]), srgb_byte(def.color[1]), srgb_byte(def.color[2]));
    let graph = h.graph()?;
    if let Some(l) = crate::commands::base_light_mut(graph, light, kind) {
        l.set_color(color);
        l.set_intensity(intensity);
    }
    graph[light].set_cast_shadows(shadows);
    if let Some(p) = graph[light].cast_mut::<PointLight>() {
        p.set_radius(range);
    }
    if let Some(s) = graph[light].cast_mut::<SpotLight>() {
        s.set_distance(range);
        s.set_hotspot_cone_angle(def.cone.0);
        s.set_falloff_angle_delta((def.cone.1 - def.cone.0).max(0.0));
    }
    Ok(e)
}

/// Makes a light of a Fyrox scene file a light entity, keeping every setting
/// the editor gave it. A light entity is a pivot whose +Z is where the light
/// shines, holding the light node turned to match (see `light_node`), so the
/// pivot is put in between: the light itself does not move.
fn wrap_light(h: &mut Host3D, light: Handle<Node>, kind: LightKind, node: &LevelNode) -> Result<i32, String> {
    // a Fyrox light shines along its -Y, which this turns to +Z
    let turn = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f32::consts::FRAC_PI_2);
    let graph = h.graph()?;
    let parent = graph[light].parent();
    let mut pivot = pivot_node();
    pivot.set_name(&node.name);
    let (position, rotation) = {
        let t = graph[light].local_transform();
        (**t.position(), **t.rotation())
    };
    pivot.local_transform_mut().set_position(position).set_rotation(rotation * turn);
    let pivot = graph.add_node(pivot);
    graph.link_nodes(pivot, parent);
    graph.link_nodes(light, pivot);
    graph[light].set_name(format!("{}.light", node.name));
    graph[light].local_transform_mut().set_position(Vector3::zeros()).set_rotation(turn.inverse());
    let e = h.adopt_node(pivot)?;
    h.ent_mut(e)?.kind = Kind::Light { kind, light };
    let number = |name: &str| prop(&node.props, name).and_then(text_float);
    let graph = h.graph()?;
    if let Some(v) = number("intensity") {
        if let Some(l) = crate::commands::base_light_mut(graph, light, kind) {
            l.set_intensity(v);
        }
    }
    if let Some(v) = number("range") {
        if let Some(p) = graph[light].cast_mut::<PointLight>() {
            p.set_radius(v);
        }
        if let Some(s) = graph[light].cast_mut::<SpotLight>() {
            s.set_distance(v);
        }
    }
    if let Some(v) = number("shadows") {
        graph[light].set_cast_shadows(v != 0.0);
    }
    Ok(e)
}

/// Makes a camera of a Fyrox scene file a camera entity, switched off like
/// every level camera. It keeps its lens: Blitz zoom is set by the
/// horizontal field of view, Fyrox's by the vertical one.
fn adopt_camera(h: &mut Host3D, camera: Handle<Node>) -> Result<i32, String> {
    let e = h.adopt_node(camera)?;
    let aspect = h.world.width as f32 / h.world.height.max(1) as f32;
    let Some(cam) = h.graph()?[camera].cast_mut::<Camera>() else { return Err("camera missing".into()) };
    cam.set_enabled(false);
    let zoom = match cam.projection() {
        Projection::Perspective(p) => 1.0 / ((p.fov / 2.0).tan() * aspect),
        Projection::Orthographic(o) => 1.0 / (o.vertical_size * aspect),
    };
    h.ent_mut(e)?.kind = Kind::Camera { zoom, mode: 0 };
    if !h.world.gamma.is_identity() {
        h.world.gamma_dirty = true;
    }
    Ok(e)
}

/// Level cameras start switched off: every camera that is on draws, so the
/// program chooses with `CameraProjMode`.
fn add_camera(h: &mut Host3D, holder: i32, node: &LevelNode, def: &CameraDef) -> Result<i32, String> {
    let e = crate::commands::create_camera(h, holder)?;
    face_backwards(h, e)?;
    let holder_node = h.ent(holder)?.node;
    take_name(h, holder_node, e, &node.name)?;
    h.ent_mut(e)?.kind = Kind::Camera { zoom: def.zoom, mode: 0 };
    let cam_node = h.ent(e)?.node;
    if let Some(cam) = h.graph()?[cam_node].cast_mut::<Camera>() {
        cam.set_enabled(false);
        cam.projection_mut().set_z_near(def.near.max(0.0001));
        if let Some(far) = def.far {
            cam.projection_mut().set_z_far(far.max(def.near + 0.001));
        }
    }
    h.update_camera_fov(e)?;
    Ok(e)
}

/// Puts the model a `prefab` property names inside the object.
fn place_prefab(h: &mut Host3D, e: i32, level: &str, value: &str) -> Result<(), String> {
    let Some(path) = prefab_path(level, value, Path::is_file) else {
        eprintln!("LoadLevelFile: prefab not found: {value}");
        return Ok(());
    };
    let shown = path.to_string_lossy().to_string();
    let rm = h.resources().ok_or("Graphics3D must be called before LoadLevelFile")?;
    let Some(file) = crate::model::resource_path("LoadLevelFile", &shown) else { return Ok(()) };
    match block_on(rm.request::<Model>(file)) {
        Ok(model) => {
            let loader = crate::model::loader_matrix(h, &shown);
            crate::model::attach_model(h, e, &model, loader)
        }
        Err(err) => {
            eprintln!("LoadLevelFile: cannot load prefab {value}: {err:?}");
            Ok(())
        }
    }
}

/// Visibility, picking and prefabs, which need no other object to be ready.
fn apply_settings(h: &mut Host3D, e: i32, node: &LevelNode, level: &str) -> Result<(), String> {
    if let Some(value) = prop(&node.props, "prefab") {
        place_prefab(h, e, level, value)?;
    }
    let n = h.ent(e)?.node;
    // hit_ objects are only out of sight: a hidden entity would not collide
    if solid_prefix(&node.name) == Some(false) {
        h.graph()?[n].set_visibility(false);
    }
    if prop(&node.props, "hidden").and_then(text_int).is_some_and(|v| v != 0) {
        h.ent_mut(e)?.hidden = true;
        h.graph()?[n].set_visibility(false);
    }
    if let Some(mode) = prop(&node.props, "pick").and_then(text_int) {
        if !(0..=3).contains(&mode) {
            return Err(format!("LoadLevelFile: object '{}': pick must be 0-3, got {mode}", node.name));
        }
        h.ent_mut(e)?.pick_mode = mode;
        h.ent_mut(e)?.obscurer = true;
    }
    Ok(())
}

/// Collision type and rigid body. A physics body leaves the level's tree (it
/// moves in world space), so it is remembered in order to be freed with it.
fn apply_solid(h: &mut Host3D, e: i32, node: &LevelNode, level: i32, collision_type: i32) -> Result<(), String> {
    let solid = solid_prefix(&node.name).is_some();
    let own_type = prop(&node.props, "collision_type").and_then(text_int);
    let col_type = own_type.or((solid && collision_type != 0).then_some(collision_type));
    if let Some(t) = col_type {
        let pos = h.world_of(e)?.position();
        let ent = h.ent_mut(e)?;
        ent.col_type = t;
        ent.last_pos = Some(pos);
    }
    let body = match prop(&node.props, "physics") {
        Some(text) => Some(body_type(&node.name, text)?),
        // without a collision type for Collisions, solid means a static body
        None => (solid && col_type.is_none()).then_some(2),
    };
    let Some(body) = body else { return Ok(()) };
    let shape = prop(&node.props, "shape").map_or(Ok(SHAPE_AUTO), |s| body_shape(&node.name, s))?;
    let number = |name: &str| prop(&node.props, name).and_then(text_float);
    h.enable_physics(e, body, shape, number("mass").unwrap_or(1.0))?;
    if let Some(v) = number("friction") {
        h.collider_mut(e)?.set_friction(v);
    }
    if let Some(v) = number("bounce") {
        h.collider_mut(e)?.set_restitution(v);
    }
    h.ent_mut(e)?.level_owner = level as u32;
    Ok(())
}

/// `LoadLevelFile`: 0 if the file cannot be loaded (like `LoadMesh`).
pub fn load_level(h: &mut Host3D, path: &str, parent: i32, collision_type: i32) -> Result<i32, String> {
    let id = crate::model::load_mesh(h, path, parent)?;
    if id == 0 {
        return Ok(0);
    }
    h.ent_mut(id)?.level_objects = Some(Box::default());
    let scene = is_scene(path);
    if !scene && !is_gltf(path) {
        return Ok(id);
    }
    let file = if scene {
        LevelFile::default()
    } else {
        let bytes = std::fs::read(path).map_err(|e| format!("LoadLevelFile: cannot read {path}: {e}"))?;
        parse_level(&bytes).map_err(|e| format!("LoadLevelFile: {path}: {e}"))?
    };
    let pivot = h.ent(id)?.node;
    let found: Vec<(LevelNode, Handle<Node>, Native)> = {
        let graph = h.graph()?;
        let instance = graph[pivot].children().iter().copied().find(|&n| graph[n].is_resource_instance_root());
        let Some(instance) = instance else { return Err(format!("LoadLevelFile: {path}: model missing")) };
        if scene {
            scene_nodes(graph, instance)
        } else {
            let located = locate_nodes(graph, instance, &file).into_iter();
            located.filter(|(i, _)| file.nodes[*i].matters()).map(|(i, n)| (file.nodes[i].clone(), n, Native::Plain)).collect()
        }
    };
    let mut placed = Vec::new();
    let mut lights = false;
    for (node, handle, native) in found {
        let e = match native {
            Native::Light(kind) => {
                lights = true;
                wrap_light(h, handle, kind, &node)?
            }
            Native::Camera => adopt_camera(h, handle)?,
            Native::Plain => {
                let e = h.adopt_node(handle)?;
                if let Some(def) = &node.light {
                    add_light(h, e, &node, def)?
                } else if let Some(def) = &node.camera {
                    add_camera(h, e, &node, def)?
                } else {
                    e
                }
            }
        };
        let ent = h.ent_mut(e)?;
        ent.props = node.props.clone();
        ent.level_of = id as u32;
        placed.push((e, node));
    }
    if lights {
        // the shadow settings GraphicsQuality gives every light
        h.apply_light_shadows()?;
    }
    let objects: Vec<i32> = placed.iter().filter(|(_, n)| !n.props.is_empty()).map(|(e, _)| *e).collect();
    h.ent_mut(id)?.level_objects = Some(Box::new(objects));
    for (e, node) in &placed {
        apply_settings(h, *e, node, path)?;
    }
    h.flush_meshes()?;
    // children first: a body takes the meshes under it into its collider
    for (e, node) in placed.iter().rev() {
        apply_solid(h, *e, node, id, collision_type)?;
    }
    Ok(id)
}

/// The level's objects that still exist, optionally only those of a `type`.
pub fn objects(h: &Host3D, level: i32, kind: &str) -> Result<Vec<i32>, String> {
    let list = h.ent(level)?.level_objects.as_deref().ok_or_else(|| format!("Entity {level} is not a level"))?;
    Ok(list
        .iter()
        .copied()
        .filter(|&e| {
            h.ent(e).is_ok_and(|ent| {
                ent.level_of == level as u32 && (kind.is_empty() || prop(&ent.props, "type").is_some_and(|t| t.eq_ignore_ascii_case(kind)))
            })
        })
        .collect())
}

/// `LevelObject`: numbered from 1, like `GetChild`.
pub fn object(h: &Host3D, level: i32, index: i32, kind: &str) -> Result<i32, String> {
    let list = objects(h, level, kind)?;
    usize::try_from(index - 1)
        .ok()
        .and_then(|i| list.get(i))
        .copied()
        .ok_or_else(|| format!("LevelObject index {index} out of range (level has {})", list.len()))
}

/// `SetEntityProperty`: replaces the value of a property of the same name.
pub fn set_property(h: &mut Host3D, e: i32, name: &str, value: &str) -> Result<(), String> {
    let props = &mut h.ent_mut(e)?.props;
    match props.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(name)) {
        Some((_, v)) => *v = value.to_string(),
        None => props.push((name.to_string(), value.to_string())),
    }
    Ok(())
}

/// A property as a number; `fallback` if it is missing or not a number.
pub fn property_float(h: &Host3D, e: i32, name: &str, fallback: f32) -> Result<f32, String> {
    Ok(prop(&h.ent(e)?.props, name).and_then(text_float).unwrap_or(fallback))
}

/// A property as an integer; `fallback` if it is missing or not a number.
pub fn property_int(h: &Host3D, e: i32, name: &str, fallback: i32) -> Result<i32, String> {
    Ok(prop(&h.ent(e)?.props, name).and_then(text_int).unwrap_or(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glb(json: &str) -> Vec<u8> {
        let mut text = json.as_bytes().to_vec();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let mut out = b"glTF".to_vec();
        out.extend(2u32.to_le_bytes());
        out.extend((20 + text.len() as u32).to_le_bytes());
        out.extend((text.len() as u32).to_le_bytes());
        out.extend(b"JSON");
        out.extend(text);
        out
    }

    // what Blender 5.1 writes for a sun of strength 3 and lamps of 500 and 200 W
    const BLENDER: &str = r#"{"asset":{"generator":"Khronos glTF Blender I/O v5.1"},"scene":0,
        "scenes":[{"nodes":[0,1,2,3]}],
        "nodes":[{"name":"col_Wall.001","extras":{"type":"wall","hp":5,"ratio":0.5,"on":true}},
                 {"name":"Sun","extensions":{"KHR_lights_punctual":{"light":0}}},
                 {"name":"Group","children":[4]},
                 {"name":"Cam","camera":0},
                 {"name":"Lamp","extensions":{"KHR_lights_punctual":{"light":1}}}],
        "cameras":[{"type":"perspective","perspective":{"aspectRatio":1.7777777,"yfov":0.3995965,"zfar":1000,"znear":0.1}}],
        "extensions":{"KHR_lights_punctual":{"lights":[
            {"color":[1,1,1],"intensity":2049,"type":"directional"},
            {"color":[1,0.5,0.2],"intensity":27175.7,"type":"point"},
            {"color":[1,1,1],"intensity":10870.3,"type":"spot","spot":{"innerConeAngle":0.35,"outerConeAngle":0.5}}]}}}"#;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn reads_a_blender_export() {
        let f = parse_level(&glb(BLENDER)).unwrap();
        assert_eq!(f.roots, [0, 1, 2, 3]);
        assert_eq!(f.nodes[2].children, [4]);
        let wall = &f.nodes[0];
        assert_eq!(prop(&wall.props, "TYPE"), Some("wall"));
        assert_eq!(prop(&wall.props, "hp"), Some("5"));
        assert_eq!(prop(&wall.props, "ratio"), Some("0.5"));
        assert_eq!(prop(&wall.props, "on"), Some("1"));
        assert_eq!(prop(&wall.props, "nothing"), None);
        // the sun's strength comes back as Blender's number
        let sun = f.nodes[1].light.as_ref().unwrap();
        assert_eq!(sun.kind, LightKind::Directional);
        assert!(close(sun.intensity, 3.0 * LIGHT_GAIN), "{sun:?}");
        // a 500 W lamp reaches about 16 units
        let lamp = f.nodes[4].light.as_ref().unwrap();
        assert_eq!(lamp.kind, LightKind::Point);
        assert!((lamp.range - 15.9).abs() < 0.2, "{lamp:?}");
        assert_eq!(lamp.color[1], 0.5);
        // Blender's default 50 mm lens is 39.6 degrees across
        let cam = f.nodes[3].camera.as_ref().unwrap();
        assert!((cam.zoom - 2.778).abs() < 0.01, "{cam:?}");
        assert_eq!(cam.far, Some(1000.0));
    }

    #[test]
    fn plain_gltf_and_units_of_other_tools() {
        let text = r#"{"asset":{"generator":"other"},"scenes":[{"nodes":[0]}],"nodes":[{"name":"L","extensions":{"KHR_lights_punctual":{"light":0}}}],
            "extensions":{"KHR_lights_punctual":{"lights":[{"type":"directional","intensity":2},{"type":"spot","range":7}]}}}"#;
        let f = parse_level(text.as_bytes()).unwrap();
        assert_eq!(f.roots, [0]);
        assert!(close(f.nodes[0].light.as_ref().unwrap().intensity, 2.0 * LIGHT_GAIN));
        let g = text.replace("\"light\":0", "\"light\":1");
        let spot = parse_level(g.as_bytes()).unwrap().nodes[0].light.clone().unwrap();
        assert_eq!(spot.kind, LightKind::Spot);
        assert_eq!(spot.range, 7.0);
    }

    #[test]
    fn bad_files_are_errors() {
        assert!(parse_level(b"not json").is_err());
        assert!(parse_level(b"glTF\x02\0\0\0").is_err());
        // a node index that does not exist is ignored
        let f = parse_level(br#"{"scenes":[{"nodes":[9]}],"nodes":[]}"#).unwrap();
        assert!(f.roots.is_empty());
    }

    fn property(name: &str, value: PropertyValue) -> fyrox::scene::base::Property {
        fyrox::scene::base::Property { name: name.into(), value }
    }

    #[test]
    fn scene_properties_read_as_text() {
        assert_eq!(property_text(&PropertyValue::String("coin".into())), "coin");
        assert_eq!(property_text(&PropertyValue::I32(-5)), "-5");
        assert_eq!(property_text(&PropertyValue::U8(1)), "1");
        assert_eq!(property_text(&PropertyValue::F32(2.5)), "2.5");
        assert_eq!(property_text(&PropertyValue::F32(3.0)), "3");
        assert_eq!(property_text(&PropertyValue::NodeHandle(Handle::NONE)), "");
        assert!(is_scene("levels/One.RGS"));
        assert!(!is_scene("levels/one.glb"));
    }

    #[test]
    fn prefabs_named_in_a_scene_saved_as_text() {
        let text = r#"Item0[Name<str:"type">]{ Value[Id<u32:2>0<str:"tree">]{} }
            Item1[Name<str:"Prefab">]
            {
                Value[Id<u32:2>0<str:"models/Tree.glb">]{}
            }
            Item0[Name<str:"prefab">]{ Value[Id<u32:5>0<i32:1>]{} }
            Tag[Value<str:"prefab">Flags<u8:1>]{}"#;
        assert_eq!(scene_prefabs(text), ["models/Tree.glb"]);
        assert!(scene_prefabs("").is_empty());
    }

    #[test]
    fn objects_of_a_fyrox_scene() {
        use fyrox::scene::{
            base::BaseBuilder,
            camera::CameraBuilder,
            light::{directional::DirectionalLightBuilder, BaseLightBuilder},
            pivot::PivotBuilder,
            Scene,
        };
        let mut scene = Scene::new();
        let root = scene.graph.get_root();
        let mut add = |node: Node, parent: Handle<Node>| {
            let h = scene.graph.add_node(node);
            scene.graph.link_nodes(h, parent);
            h
        };
        let named = |name: &str| BaseBuilder::new().with_name(name);
        add(PivotBuilder::new(named("Plain")).build_node(), root);
        let group = add(PivotBuilder::new(named("Group")).build_node(), root);
        let mut coin = PivotBuilder::new(named("Coin")).build_node();
        coin.set_properties(vec![property("type", PropertyValue::String("coin".into())), property("value", PropertyValue::I32(5))]);
        let coin = add(coin, group);
        let wall = add(PivotBuilder::new(named("col_Wall")).build_node(), root);
        let sun = add(DirectionalLightBuilder::new(BaseLightBuilder::new(named("Sun"))).build_node(), root);
        let cam = add(CameraBuilder::new(named("Overview")).build_node(), root);

        let found = scene_nodes(&scene.graph, root);
        let handles: Vec<_> = found.iter().map(|(_, h, _)| *h).collect();
        assert_eq!(handles, [coin, wall, sun, cam]);
        assert_eq!(prop(&found[0].0.props, "Value"), Some("5"));
        assert_eq!(found[1].2, Native::Plain);
        assert_eq!(found[2].2, Native::Light(LightKind::Directional));
        assert_eq!(found[3].2, Native::Camera);
    }

    /// Writes `examples/media/editor_level.rgs`, the level of
    /// `examples/editorlevel.nb`, as Neptune Level Editor saves a scene:
    ///
    ///     cargo test -p neptune-runtime writes_the_sample_scene -- --ignored
    #[test]
    #[ignore = "writes a file in examples/media"]
    fn writes_the_sample_scene() {
        use fyrox::core::{algebra::Matrix4, visitor::Visitor};
        use fyrox::scene::{
            base::BaseBuilder,
            camera::CameraBuilder,
            light::{directional::DirectionalLightBuilder, point::PointLightBuilder, BaseLightBuilder},
            mesh::{surface::SurfaceData, Mesh},
            pivot::PivotBuilder,
            transform::TransformBuilder,
            Scene,
        };
        let mut scene = Scene::new();
        let root = scene.graph.get_root();
        let text = |name: &str, value: &str| property(name, PropertyValue::String(value.into()));
        let at = |name: &str, pos: [f32; 3], pitch: f32| {
            let turn = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), pitch.to_radians());
            let place = TransformBuilder::new().with_local_position(Vector3::from(pos)).with_local_rotation(turn);
            BaseBuilder::new().with_name(name).with_local_transform(place.build())
        };
        let cube = |name: &str, pos: [f32; 3], size: [f32; 3], rgb: [u8; 3]| {
            let mut node = crate::world::mesh_node(SurfaceData::make_cube(Matrix4::identity()), name);
            node.local_transform_mut().set_position(Vector3::from(pos)).set_scale(Vector3::from(size));
            let colour = crate::world::new_material(Color::opaque(rgb[0], rgb[1], rgb[2]));
            node.cast_mut::<Mesh>().unwrap().surfaces_mut()[0].set_material(colour);
            node
        };
        let mut nodes = vec![
            cube("col_Ground", [0.0, -0.25, 0.0], [24.0, 0.5, 24.0], [96, 132, 88]),
            cube("col_WallNorth", [0.0, 1.5, 12.0], [24.0, 3.0, 0.5], [150, 140, 128]),
            cube("col_WallEast", [-12.0, 1.5, 0.0], [0.5, 3.0, 24.0], [150, 140, 128]),
            cube("hit_Fence", [12.0, 1.5, 0.0], [0.5, 3.0, 24.0], [255, 0, 255]),
            DirectionalLightBuilder::new(BaseLightBuilder::new(at("Sun", [0.0, 10.0, 0.0], -40.0)).with_intensity(2.5)).build_node(),
            CameraBuilder::new(at("Overview", [0.0, 9.0, -14.0], 30.0)).build_node(),
        ];
        let mut lamp = PointLightBuilder::new(BaseLightBuilder::new(at("Lamp", [4.0, 2.5, 4.0], 0.0)).with_color(Color::opaque(255, 170, 90)))
            .with_radius(9.0)
            .build_node();
        lamp.set_properties(vec![property("flicker", PropertyValue::I32(1))]);
        nodes.push(lamp);
        let mut spawn = PivotBuilder::new(at("Spawn", [0.0, 1.0, -6.0], 0.0)).build_node();
        spawn.set_properties(vec![text("type", "spawn")]);
        nodes.push(spawn);
        for (i, x) in [-4.0f32, 0.0, 4.0].into_iter().enumerate() {
            let mut coin = cube(&format!("Coin{}", i + 1), [x, 0.6, 2.0], [0.5, 0.5, 0.5], [240, 200, 60]);
            coin.set_properties(vec![text("type", "coin"), property("value", PropertyValue::I32(10 * (i as i32 + 1)))]);
            nodes.push(coin);
        }
        let mut crate_box = cube("Crate", [3.0, 5.0, -2.0], [1.0, 1.0, 1.0], [170, 120, 70]);
        crate_box.set_properties(vec![text("physics", "dynamic"), text("shape", "box"), property("mass", PropertyValue::F32(5.0))]);
        nodes.push(crate_box);
        for node in nodes {
            let h = scene.graph.add_node(node);
            scene.graph.link_nodes(h, root);
        }
        let mut visitor = Visitor::new();
        scene.save("Scene", &mut visitor).unwrap();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/media/editor_level.rgs");
        visitor.save_ascii_to_file(path).unwrap();
    }

    #[test]
    fn name_prefixes() {
        assert_eq!(solid_prefix("col_Wall.001"), Some(true));
        assert_eq!(solid_prefix("HIT_Fence"), Some(false));
        assert_eq!(solid_prefix("collar"), None);
        assert_eq!(solid_prefix("hit"), None);
        assert_eq!(solid_prefix("é_é_"), None);
    }

    #[test]
    fn numbers_in_properties() {
        assert_eq!(text_int("5"), Some(5));
        assert_eq!(text_int("2.5"), Some(2));
        assert_eq!(text_int("3.5"), Some(4));
        assert_eq!(text_int("door"), None);
    }

    #[test]
    fn prefab_is_looked_for_beside_the_level_first() {
        let only_beside = |p: &Path| p == Path::new("levels/tree.glb");
        assert_eq!(prefab_path("levels/a.glb", "tree.glb", only_beside), Some(PathBuf::from("levels/tree.glb")));
        let only_root = |p: &Path| p == Path::new("art/tree.glb");
        assert_eq!(prefab_path("levels/a.glb", "art/tree.glb", only_root), Some(PathBuf::from("art/tree.glb")));
        assert_eq!(prefab_path("levels/a.glb", "x.glb", |_| false), None);
    }

    #[test]
    fn light_colours_are_stored_as_srgb() {
        assert_eq!(srgb_byte(0.0), 0);
        assert_eq!(srgb_byte(1.0), 255);
        assert_eq!(srgb_byte(0.5), 188);
    }
}
