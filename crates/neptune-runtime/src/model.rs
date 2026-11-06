//! Loaded models (`LoadMesh`), entity copies (`CopyEntity`) and mesh geometry queries.
//!
//! Models are loaded by Fyrox's resource system (glTF and FBX), so anything
//! Fyrox can import is available to BASIC programs. Model data is
//! right-handed; NeptuneBASIC's Blitz-space mapping negates X, which is a
//! proper handedness conversion: a model's front (+Z) is the entity's
//! forward direction and it is not mirrored.

use fyrox::{
    core::{algebra::Vector3, futures::executor::block_on, pool::Handle},
    graph::SceneGraph,
    resource::model::{Model, ModelResourceExtension},
    scene::{
        graph::Graph,
        mesh::{buffer::VertexAttributeUsage, buffer::VertexReadTrait, Mesh},
        node::Node,
    },
};

use crate::world::{pivot_node, Host3D, Kind};

/// Loads a model file and returns a new entity (0 if the file can't be loaded,
/// matching Blitz3D).
pub fn load_mesh(h: &mut Host3D, path: &str, parent: i32) -> Result<i32, String> {
    let rm = h.resources().ok_or("Graphics3D must be called before LoadMesh")?;
    let Some(file) = resource_path("LoadMesh", path) else { return Ok(0) };
    let resource = rm.request::<Model>(file);
    let model = match block_on(resource) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("LoadMesh: cannot load {path}: {e:?}");
            return Ok(0);
        }
    };
    let loader = loader_matrix(h, path);
    instantiate(h, &model, parent, loader)
}

/// The path Fyrox's resource manager needs for a file: relative to the
/// working directory, which Fyrox requires (it cannot load files outside
/// it). `None`, after printing why, if the file is missing or outside.
pub fn resource_path(command: &str, path: &str) -> Option<std::path::PathBuf> {
    let file = match std::fs::canonicalize(path) {
        Ok(f) if f.is_file() => f,
        _ => {
            eprintln!("{command}: file not found: {path}");
            return None;
        }
    };
    let cwd = std::env::current_dir().and_then(std::fs::canonicalize).ok()?;
    match file.strip_prefix(&cwd) {
        Ok(rel) => Some(rel.to_path_buf()),
        Err(_) => {
            let shown = std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default();
            eprintln!("{command}: '{path}' is outside the game folder ({shown}); models must be inside it");
            None
        }
    }
}

/// The `LoaderMatrix` set for a file's extension, if any.
pub fn loader_matrix(h: &Host3D, path: &str) -> Option<fyrox::core::algebra::Matrix3<f32>> {
    let ext = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    h.world.loader_matrices.iter().rev().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

/// Splits a matrix into rotation and per-axis scale (a negative scale keeps
/// mirror matrices such as `LoaderMatrix "x", 1,0,0, 0,1,0, 0,0,-1`).
pub fn rotation_scale(m: &fyrox::core::algebra::Matrix3<f32>) -> (fyrox::core::algebra::Matrix3<f32>, Vector3<f32>) {
    let mut scale = Vector3::new(m.column(0).norm(), m.column(1).norm(), m.column(2).norm()).map(|v| v.max(1e-9));
    let mut r = fyrox::core::algebra::Matrix3::from_columns(&[m.column(0) / scale.x, m.column(1) / scale.y, m.column(2) / scale.z]);
    if r.determinant() < 0.0 {
        scale.z = -scale.z;
        let c = -r.column(2);
        r.set_column(2, &c);
    }
    (r, scale)
}

/// Creates an entity holding a new instance of a loaded model.
pub fn instantiate(
    h: &mut Host3D,
    model: &fyrox::resource::model::ModelResource,
    parent: i32,
    loader: Option<fyrox::core::algebra::Matrix3<f32>>,
) -> Result<i32, String> {
    let id = h.add_entity(pivot_node(), Kind::Mesh, parent)?;
    attach_model(h, id, model, loader)?;
    Ok(id)
}

/// Puts a new instance of a loaded model inside an existing (empty) entity.
pub fn attach_model(
    h: &mut Host3D,
    id: i32,
    model: &fyrox::resource::model::ModelResource,
    loader: Option<fyrox::core::algebra::Matrix3<f32>>,
) -> Result<(), String> {
    let instance = model.instantiate(h.scene()?);
    // models named by a saved scene file; each keeps its own animations
    // (FindChild gives them to the program)
    let scene_file = crate::scenefile::restore_models(h, instance)? > 0;
    // LoaderMatrix for this file type: applied to the model inside the entity
    if let Some(m) = loader {
        let (r, s) = rotation_scale(&m);
        let mirror = fyrox::core::algebra::Matrix3::from_diagonal(&Vector3::new(-1.0, 1.0, 1.0));
        let q = fyrox::core::algebra::UnitQuaternion::from_matrix(&(mirror * r * mirror));
        let t = h.graph()?[instance].local_transform_mut();
        t.set_rotation(q * **t.rotation());
        t.set_scale(t.scale().component_mul(&s));
    }
    let pivot = h.ent(id)?.node;
    h.graph()?.link_nodes(instance, pivot);
    let anim = match crate::anim::find_player(h.graph()?, instance).filter(|_| !scene_file) {
        Some(player) => {
            let seqs = crate::anim::take_control(h.graph()?, player);
            (!seqs.is_empty()).then(|| crate::anim::AnimState::new(player, seqs))
        }
        None => None,
    };
    let has_anim = anim.is_some();
    let e = h.ent_mut(id)?;
    // Materials belong to the shared model resource until first modified.
    e.own_materials = false;
    e.model = true;
    e.anim = anim;
    e.model_res = Some(model.clone());
    e.loader = loader;
    if has_anim {
        // show the first frame of the first sequence, like Blitz
        h.set_anim_time(id, 0.0, 0)?;
    }
    Ok(())
}

/// `LoadAnimSeq`: adds the animations of another model file (same skeleton)
/// to an animated entity; returns the first new sequence number.
pub fn load_anim_seq(h: &mut Host3D, e: i32, path: &str) -> Result<i32, String> {
    let rm = h.resources().ok_or("Graphics3D must be called before LoadAnimSeq")?;
    let player = h.ent(e)?.anim.as_ref().filter(|a| a.keys.is_none()).map(|a| a.player).ok_or_else(|| format!("Entity {e} is not an animated model"))?;
    let Some(file) = resource_path("LoadAnimSeq", path) else { return Ok(-1) };
    let model = match block_on(rm.request::<Model>(file)) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("LoadAnimSeq: cannot load {path}: {err:?}");
            return Ok(-1);
        }
    };
    let root = h.ent(e)?.node;
    let new = model.retarget_animations_to_player(root, player, h.graph()?);
    if new.is_empty() {
        return Ok(-1);
    }
    // the new animations play under program control like the others
    let all = crate::anim::take_control(h.graph()?, player);
    let st = h.ent_mut(e)?.anim.as_mut().unwrap();
    let first = st.seqs.len() as i32;
    st.seqs = all;
    Ok(first)
}

/// Copies a node subtree, leaving out nodes for which `skip` is true.
pub fn copy_tree(graph: &mut Graph, node: Handle<Node>, skip: &dyn Fn(Handle<Node>) -> bool) -> Handle<Node> {
    let copy = graph.copy_single_node(node);
    let new = graph.add_node(copy);
    let children: Vec<Handle<Node>> = graph[node].children().to_vec();
    for c in children {
        if !skip(c) {
            let cc = copy_tree(graph, c, skip);
            graph.link_nodes(cc, new);
        }
    }
    new
}

/// Collects the triangles of every mesh in a node's subtree, in the node's
/// own coordinate frame converted to Blitz space. Nodes for which `skip` is
/// true (other entities) are left out.
pub fn triangles(graph: &Graph, node: Handle<Node>, skip: &dyn Fn(Handle<Node>) -> bool) -> Vec<[Vector3<f32>; 3]> {
    let mut out = Vec::new();
    let mut stack = vec![(node, fyrox::core::algebra::Matrix4::identity())];
    while let Some((n, m)) = stack.pop() {
        let node_ref = &graph[n];
        // the root's own transform is the entity transform, which we exclude
        let m = if n == node { m } else { m * node_ref.local_transform().matrix() };
        if let Some(mesh) = node_ref.cast::<Mesh>() {
            for s in mesh.surfaces() {
                let data = s.data();
                let data = data.data_ref();
                let verts: Vec<Vector3<f32>> = data
                    .vertex_buffer
                    .iter()
                    .map(|v| v.read_3_f32(VertexAttributeUsage::Position).unwrap_or_default())
                    .map(|p| {
                        let q = m.transform_point(&p.into());
                        Vector3::new(-q.x, q.y, q.z)
                    })
                    .collect();
                for t in data.geometry_buffer.iter() {
                    let [a, b, c] = t.0;
                    if let (Some(a), Some(b), Some(c)) = (verts.get(a as usize), verts.get(b as usize), verts.get(c as usize)) {
                        out.push([*a, *b, *c]);
                    }
                }
            }
        }
        for &c in node_ref.children() {
            if !skip(c) {
                stack.push((c, m));
            }
        }
    }
    out
}

/// Axis-aligned bounds `(min, max)` of triangles, or `None` if there are none.
pub fn bounds(tris: &[[Vector3<f32>; 3]]) -> Option<(Vector3<f32>, Vector3<f32>)> {
    let mut it = tris.iter().flatten();
    let first = *it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.inf(p), hi.sup(p))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_cover_all_points() {
        let t = [[Vector3::new(-1.0, 0.0, 2.0), Vector3::new(3.0, -2.0, 0.0), Vector3::new(0.0, 5.0, 1.0)]];
        let (lo, hi) = bounds(&t).unwrap();
        assert_eq!(lo, Vector3::new(-1.0, -2.0, 0.0));
        assert_eq!(hi, Vector3::new(3.0, 5.0, 2.0));
        assert!(bounds(&[]).is_none());
    }
}
