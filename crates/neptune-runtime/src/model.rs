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
    if !std::path::Path::new(path).is_file() {
        eprintln!("LoadMesh: file not found: {path}");
        return Ok(0);
    }
    let resource = rm.request::<Model>(path);
    let model = match block_on(resource) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("LoadMesh: cannot load {path}: {e:?}");
            return Ok(0);
        }
    };
    let instance = model.instantiate(h.scene()?);
    let id = h.add_entity(pivot_node(), Kind::Mesh, parent)?;
    let pivot = h.ent(id)?.node;
    h.graph()?.link_nodes(instance, pivot);
    // Materials belong to the shared model resource until first modified.
    h.ent_mut(id)?.own_materials = false;
    h.ent_mut(id)?.model = true;
    Ok(id)
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
