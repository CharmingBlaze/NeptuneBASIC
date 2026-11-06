//! Saving scenes (ext): `SaveScene`, `SaveEntity`.
//!
//! Entities are written as a Fyrox scene file (`.rgs`), the format of the
//! Fyrox editor. `LoadMesh` (or `LoadScene`) loads it back as one entity
//! holding everything that was saved, and `FindChild` finds the entities that
//! had names. Scenes made in the Fyrox editor load the same way.
//!
//! Meshes, materials, textures, lights, sprites and terrain are saved in the
//! file. Loaded models (`LoadMesh`) are saved by file name and placement and
//! loaded again from their files, so they come back exactly as loaded
//! (animated characters included); changes made to a model's own parts after
//! loading are not kept. Cameras are left out, and so are the program's own
//! settings that are not part of the scene: collision types, pick modes,
//! physics settings and sounds.

use std::collections::HashMap;

use fyrox::{
    core::{
        futures::executor::block_on,
        pool::Handle,
        reflect::Reflect,
        visitor::{Visitor, VisitorFlags},
    },
    graph::SceneGraph,
    asset::untyped::ResourceKind,
    material::MaterialResource,
    resource::{
        model::{Model, ModelResource, ModelResourceExtension},
        texture::TextureResource,
    },
    scene::{
        base::Base,
        camera::Camera,
        mesh::{surface::Surface, surface::SurfaceResource, Mesh},
        node::Node,
        Scene,
    },
};

use crate::world::{Host3D, Kind};

/// Saves the given entities, each with its children, to `path`.
pub fn save(h: &mut Host3D, entities: &[i32], path: &str) -> Result<(), String> {
    let mut roots = Vec::new();
    for &e in entities {
        roots.push(h.ent(e)?.node);
    }
    // entity names become node names, so FindChild finds them after loading
    let named: Vec<(Handle<Node>, String)> =
        h.entities().filter(|(_, e)| !e.name.is_empty()).map(|(_, e)| (e.node, e.name.clone())).collect();
    let rm = h.resources().ok_or("Graphics3D must be called before SaveScene")?;
    let graph = h.graph()?;
    for (node, name) in named {
        graph[node].set_name(name);
    }
    // loaded models are saved as a marker naming their file
    let mut models: HashMap<Handle<Node>, String> = HashMap::new();
    for &r in &roots {
        for n in graph.traverse_handle_iter(r) {
            let node = &graph[n];
            if !node.is_resource_instance_root() {
                continue;
            }
            let Some(path) = node.resource().and_then(|res| rm.resource_path(res.as_ref())) else { continue };
            let path = path.to_string_lossy().replace('\\', "/");
            // a loaded scene is saved in full (it may be the file being written)
            if !path.to_lowercase().ends_with(".rgs") {
                models.insert(n, path);
            }
        }
    }
    let mut scene = Scene::new();
    let dest_root = scene.graph.get_root();
    for r in roots {
        let (copy, _) = graph.copy_node(
            r,
            &mut scene.graph,
            false,
            &mut |_, n: &Node| n.cast::<Camera>().is_none() && !models.contains_key(&n.parent()),
            &mut |old, new: &mut Node| {
                if let Some(path) = models.get(&old) {
                    new.set_tag(format!("{MODEL_TAG}{path}"));
                }
            },
            &mut |_, _, _| {},
        );
        scene.graph.link_nodes(copy, dest_root);
    }
    // Make the file self-contained: nodes of loaded models stop referring to
    // their model file (whose ID is only known to this run) and every
    // property is written out.
    for node in scene.graph.linear_iter_mut() {
        let base: &mut Base = node;
        let mut ok = true;
        base.set_field("resource", Box::new(None::<ModelResource>), &mut |r| ok &= r.is_ok());
        base.set_field("is_resource_instance_root", Box::new(false), &mut |r| ok &= r.is_ok());
        if !ok {
            eprintln!("SaveScene: cannot detach a model node");
        }
        if let Some(mesh) = node.cast_mut::<Mesh>() {
            for surface in mesh.surfaces_mut() {
                embed_surface(surface);
            }
        }
    }
    let mut visitor = Visitor::new();
    visitor.flags |= VisitorFlags::SERIALIZE_EVERYTHING;
    scene.save("Scene", &mut visitor).map_err(|e| format!("cannot save the scene: {e:?}"))?;
    if let Some(dir) = std::path::Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot write '{path}': {e}"))?;
    }
    visitor.save_binary_to_file(path).map_err(|e| format!("cannot write '{path}': {e:?}"))
}

/// Tag of a node that stands for a model file.
const MODEL_TAG: &str = "neptune-model:";

/// After loading a scene file: loads the models it names, each in place of
/// its marker node. Returns how many markers there were.
pub fn restore_models(h: &mut Host3D, root: Handle<Node>) -> Result<usize, String> {
    let markers: Vec<(Handle<Node>, String)> = {
        let graph = h.graph()?;
        graph
            .traverse_handle_iter(root)
            .filter_map(|n| graph[n].tag().strip_prefix(MODEL_TAG).map(|p| (n, p.to_string())))
            .collect()
    };
    let count = markers.len();
    if count == 0 {
        return Ok(0);
    }
    let rm = h.resources().ok_or("Graphics3D must be called before LoadMesh")?;
    for (marker, path) in markers {
        let Some(file) = crate::model::resource_path("LoadMesh", &path) else { continue };
        let model = match block_on(rm.request::<Model>(file)) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("LoadMesh: cannot load {path}: {e:?}");
                continue;
            }
        };
        let instance = model.instantiate(h.scene()?);
        let graph = h.graph()?;
        let transform = graph[marker].local_transform().clone();
        let (name, visible, parent) = (graph[marker].name_owned(), graph[marker].visibility(), graph[marker].parent());
        let node = &mut graph[instance];
        *node.local_transform_mut() = transform;
        node.set_name(name);
        node.set_visibility(visible);
        graph.link_nodes(instance, parent);
        graph.remove_node(marker);
    }
    Ok(count)
}

/// Replaces geometry, materials and textures that come from files (whose
/// IDs are known only to this run) with copies stored in the scene file.
fn embed_surface(surface: &mut Surface) {
    let mut material = surface.material().clone();
    if material.kind() == ResourceKind::External {
        let copy = material.data_ref().clone();
        material = MaterialResource::new_embedded(copy);
    }
    let textures: Vec<(String, TextureResource)> = material
        .data_ref()
        .bindings()
        .iter()
        .filter_map(|(name, b)| b.as_texture().filter(|t| t.kind() == ResourceKind::External).map(|t| (name.to_string(), t)))
        .collect();
    if !textures.is_empty() {
        // a material shared with the running scene is not changed
        let copy = material.data_ref().clone();
        material = MaterialResource::new_embedded(copy);
        let mut m = material.data_ref();
        for (name, t) in textures {
            m.bind(name, Some(TextureResource::new_embedded(t.data_ref().clone())));
        }
    }
    if surface.data_ref().kind() == ResourceKind::External {
        let data = SurfaceResource::new_embedded(surface.data_ref().data_ref().clone());
        let bones = surface.bones().to_vec();
        *surface = Surface::new(data);
        surface.bones.set_value_and_mark_modified(bones);
    }
    surface.set_material(material);
}

/// The entities `SaveScene` saves: every top-level entity except cameras
/// and mirrors (whose reflections are made by a camera).
pub fn top_level(h: &Host3D) -> Vec<i32> {
    h.entities()
        .filter(|(_, e)| e.parent == 0 && !matches!(e.kind, Kind::Camera { .. }) && e.mirror.is_none())
        .map(|(i, _)| i)
        .collect()
}
