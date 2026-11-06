//! Custom meshes and surfaces (`CreateMesh`, `AddVertex`, `AddTriangle`, ...)
//! and brushes (`CreateBrush`, `PaintSurface`, ...).
//!
//! Every surface a program touches gets a CPU copy in Blitz space
//! ([`SurfaceInfo`]). Edits change the copy and mark it dirty; dirty surfaces
//! are rebuilt into Fyrox surface data before the next render, collision or
//! pick ([`Host3D::flush_meshes`]), so building a mesh vertex by vertex costs
//! one upload rather than one per call. Surfaces of primitives and loaded
//! models are imported the first time `GetSurface` names them.
//!
//! Blitz space is mirrored on X relative to Fyrox (see `transform.rs`), so
//! what Blitz sees as clockwise appears clockwise on screen too. Blitz front
//! faces are clockwise while Fyrox's are counter-clockwise, so triangles are
//! stored with their last two corners swapped on the GPU side.

use fyrox::{
    asset::untyped::ResourceKind,
    core::{
        algebra::{Matrix3, Vector3},
        color::Color,
        math::TriangleDefinition,
        pool::Handle,
    },
    scene::{
        base::BaseBuilder,
        mesh::{
            buffer::{
                BytesStorage, TriangleBuffer, VertexAttributeDataType, VertexAttributeDescriptor, VertexAttributeUsage,
                VertexBuffer, VertexReadTrait,
            },
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            Mesh, MeshBuilder,
        },
        node::Node,
    },
};

use crate::world::{Host3D, Kind};

/// One vertex in Blitz space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vert {
    pub pos: Vector3<f32>,
    pub normal: Vector3<f32>,
    /// RGBA, 0..1.
    pub color: [f32; 4],
    /// Two UV(W) sets.
    pub uv: [[f32; 3]; 2],
}

impl Default for Vert {
    fn default() -> Self {
        Vert { pos: Vector3::zeros(), normal: Vector3::zeros(), color: [1.0; 4], uv: [[0.0; 3]; 2] }
    }
}

/// A surface's geometry in Blitz space.
#[derive(Clone, Debug, Default)]
pub struct SurfaceInfo {
    pub entity: i32,
    /// Index of the surface within the entity's mesh node.
    pub index: usize,
    pub verts: Vec<Vert>,
    pub tris: Vec<[u32; 3]>,
    pub dirty: bool,
    /// Brush applied with `PaintSurface`/`CreateSurface` (0 = none).
    pub brush: i32,
}

/// A brush: a reusable set of surface properties.
#[derive(Clone, Debug)]
pub struct Brush {
    pub color: Color,
    pub shininess: f32,
    pub textures: [i32; crate::classic::MAX_LAYERS],
    pub blend: i32,
    pub fx: i32,
}

impl Default for Brush {
    fn default() -> Self {
        Brush { color: Color::WHITE, shininess: 0.0, textures: [0; crate::classic::MAX_LAYERS], blend: 1, fx: 0 }
    }
}

/// GPU vertex: the standard attributes plus a second UV set and a colour.
#[repr(C)]
#[derive(Clone, Copy)]
struct GpuVertex {
    position: [f32; 3],
    tex_coord: [f32; 2],
    normal: [f32; 3],
    tangent: [f32; 4],
    second_tex_coord: [f32; 2],
    color: [u8; 4],
}

const fn attr(usage: VertexAttributeUsage, data_type: VertexAttributeDataType, size: u8, loc: u8, normalized: bool) -> VertexAttributeDescriptor {
    VertexAttributeDescriptor { usage, data_type, size, divisor: 0, shader_location: loc, normalized }
}

static LAYOUT: [VertexAttributeDescriptor; 6] = [
    attr(VertexAttributeUsage::Position, VertexAttributeDataType::F32, 3, 0, false),
    attr(VertexAttributeUsage::TexCoord0, VertexAttributeDataType::F32, 2, 1, false),
    attr(VertexAttributeUsage::Normal, VertexAttributeDataType::F32, 3, 2, false),
    attr(VertexAttributeUsage::Tangent, VertexAttributeDataType::F32, 4, 3, false),
    attr(VertexAttributeUsage::TexCoord1, VertexAttributeDataType::F32, 2, 6, false),
    attr(VertexAttributeUsage::Color, VertexAttributeDataType::U8, 4, 7, true),
];

/// Builds Fyrox surface data (Fyrox space) from Blitz-space geometry.
pub fn surface_data(verts: &[Vert], tris: &[[u32; 3]]) -> SurfaceData {
    let gpu: Vec<GpuVertex> = verts
        .iter()
        .map(|v| GpuVertex {
            position: [-v.pos.x, v.pos.y, v.pos.z],
            tex_coord: [v.uv[0][0], v.uv[0][1]],
            normal: [-v.normal.x, v.normal.y, v.normal.z],
            tangent: [0.0; 4],
            second_tex_coord: [v.uv[1][0], v.uv[1][1]],
            color: v.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8),
        })
        .collect();
    let count = gpu.len();
    let vb = VertexBuffer::new_with_layout(&LAYOUT, count, BytesStorage::new(gpu)).expect("valid vertex layout");
    let n = verts.len() as u32;
    let tb = TriangleBuffer::new(
        tris.iter().filter(|t| t.iter().all(|&i| i < n)).map(|t| TriangleDefinition([t[0], t[2], t[1]])).collect(),
    );
    let mut data = SurfaceData::new(vb, tb);
    if !tris.is_empty() {
        let _ = data.calculate_tangents();
    }
    data
}

/// Reads Fyrox surface data back into Blitz space.
pub fn read_surface(data: &SurfaceData) -> (Vec<Vert>, Vec<[u32; 3]>) {
    let verts = data
        .vertex_buffer
        .iter()
        .map(|v| {
            let p = v.read_3_f32(VertexAttributeUsage::Position).unwrap_or_default();
            let n = v.read_3_f32(VertexAttributeUsage::Normal).unwrap_or_default();
            let uv0 = v.read_2_f32(VertexAttributeUsage::TexCoord0).unwrap_or_default();
            let uv1 = v.read_2_f32(VertexAttributeUsage::TexCoord1).unwrap_or_default();
            let color = v.read_4_u8(VertexAttributeUsage::Color).map(|c| [c.x, c.y, c.z, c.w].map(|x| x as f32 / 255.0)).unwrap_or([1.0; 4]);
            Vert {
                pos: Vector3::new(-p.x, p.y, p.z),
                normal: Vector3::new(-n.x, n.y, n.z),
                color,
                uv: [[uv0.x, uv0.y, 0.0], [uv1.x, uv1.y, 0.0]],
            }
        })
        .collect();
    let tris = data.geometry_buffer.iter().map(|t| [t.0[0], t.0[2], t.0[1]]).collect();
    (verts, tris)
}

/// Smooth normals: each vertex gets the area-weighted average of the faces
/// around every vertex at the same position (Blitz `UpdateNormals`).
pub fn smooth_normals(verts: &mut [Vert], tris: &[[u32; 3]]) {
    use std::collections::HashMap;
    let key = |p: &Vector3<f32>| [p.x, p.y, p.z].map(|c| (c * 1e4).round() as i64);
    let mut acc: HashMap<[i64; 3], Vector3<f32>> = HashMap::new();
    for t in tris {
        let [a, b, c] = t.map(|i| verts.get(i as usize).map(|v| v.pos));
        let (Some(a), Some(b), Some(c)) = (a, b, c) else { continue };
        // Blitz faces are clockwise seen from the front in a left-handed space
        let n = (b - a).cross(&(c - a));
        for p in [a, b, c] {
            *acc.entry(key(&p)).or_insert_with(Vector3::zeros) += n;
        }
    }
    for v in verts.iter_mut() {
        if let Some(n) = acc.get(&key(&v.pos)) {
            v.normal = n.try_normalize(1e-12).unwrap_or_else(Vector3::y);
        }
    }
}

/// Axis-aligned bounds `(min, max)`.
pub type Bounds = (Vector3<f32>, Vector3<f32>);

/// Bounds of all vertices, or `None` if there are none.
pub fn vertex_bounds<'a>(it: impl Iterator<Item = &'a Vert>) -> Option<Bounds> {
    let mut it = it.map(|v| v.pos);
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.inf(&p), hi.sup(&p))))
}

pub fn empty_mesh_node() -> Node {
    MeshBuilder::new(BaseBuilder::new().with_name("Mesh")).with_surfaces(Vec::new()).build_node()
}

impl Host3D<'_> {
    fn mesh_node_of(&mut self, mesh: i32) -> Result<Handle<Node>, String> {
        let e = self.ent(mesh)?;
        if !matches!(e.kind, Kind::Mesh) {
            return Err(format!("Entity {mesh} is not a mesh"));
        }
        let node = e.node;
        if self.graph()?[node].cast::<Mesh>().is_none() {
            return Err(format!("Entity {mesh} is a loaded model; use FindChild to reach its meshes"));
        }
        Ok(node)
    }

    pub fn surface(&self, s: i32) -> Result<&SurfaceInfo, String> {
        usize::try_from(s - 1)
            .ok()
            .and_then(|i| self.world.surfaces.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Surface {s} does not exist"))
    }

    pub fn surface_mut(&mut self, s: i32) -> Result<&mut SurfaceInfo, String> {
        usize::try_from(s - 1)
            .ok()
            .and_then(|i| self.world.surfaces.get_mut(i))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("Surface {s} does not exist"))
    }

    /// Edits a surface and marks it for upload.
    pub fn edit_surface<R>(&mut self, s: i32, f: impl FnOnce(&mut SurfaceInfo) -> R) -> Result<R, String> {
        let info = self.surface_mut(s)?;
        info.dirty = true;
        Ok(f(info))
    }

    pub fn vertex(&self, s: i32, i: i32) -> Result<Vert, String> {
        let info = self.surface(s)?;
        usize::try_from(i)
            .ok()
            .and_then(|i| info.verts.get(i))
            .copied()
            .ok_or_else(|| format!("vertex index {i} out of range (surface has {} vertices)", info.verts.len()))
    }

    pub fn edit_vertex(&mut self, s: i32, i: i32, f: impl FnOnce(&mut Vert)) -> Result<(), String> {
        self.vertex(s, i)?;
        self.edit_surface(s, |info| f(&mut info.verts[i as usize]))
    }

    /// Number of Fyrox surfaces in a mesh entity.
    pub fn count_surfaces(&mut self, mesh: i32) -> Result<usize, String> {
        let node = self.mesh_node_of(mesh)?;
        Ok(self.graph()?[node].cast::<Mesh>().map_or(0, |m| m.surfaces().len()))
    }

    /// Handle of surface `index` (0-based) of a mesh, importing it if needed.
    pub fn surface_handle(&mut self, mesh: i32, index: usize) -> Result<i32, String> {
        if let Some(pos) =
            self.world.surfaces.iter().position(|s| s.as_ref().is_some_and(|s| s.entity == mesh && s.index == index))
        {
            return Ok(pos as i32 + 1);
        }
        let node = self.mesh_node_of(mesh)?;
        let (verts, tris) = {
            let graph = self.graph()?;
            let m = graph[node].cast::<Mesh>().ok_or("not a mesh")?;
            let s = m.surfaces().get(index).ok_or_else(|| format!("mesh {mesh} has no surface {}", index + 1))?;
            let data = s.data();
            let data = data.data_ref();
            read_surface(&data)
        };
        Ok(self.add_surface_info(SurfaceInfo { entity: mesh, index, verts, tris, dirty: false, brush: 0 }))
    }

    fn add_surface_info(&mut self, info: SurfaceInfo) -> i32 {
        match self.world.surfaces.iter().position(Option::is_none) {
            Some(i) => {
                self.world.surfaces[i] = Some(info);
                i as i32 + 1
            }
            None => {
                self.world.surfaces.push(Some(info));
                self.world.surfaces.len() as i32
            }
        }
    }

    /// Adds an empty surface to a mesh.
    pub fn create_surface(&mut self, mesh: i32, brush: i32) -> Result<i32, String> {
        let node = self.mesh_node_of(mesh)?;
        let material = crate::material::standard(Color::WHITE);
        let index = {
            let graph = self.graph()?;
            let m = graph[node].cast_mut::<Mesh>().ok_or("not a mesh")?;
            let data = surface_data(&[], &[]);
            m.add_surface(SurfaceBuilder::new(SurfaceResource::new_embedded(data)).with_material(material).build());
            m.surfaces().len() - 1
        };
        let s = self.add_surface_info(SurfaceInfo { entity: mesh, index, verts: Vec::new(), tris: Vec::new(), dirty: false, brush });
        // a painted surface, or a mesh already on the classic material, needs
        // the new surface's material set up the same way
        if brush != 0 || self.material_is_classic(mesh)? {
            self.refresh_material(mesh)?;
        }
        Ok(s)
    }

    fn material_is_classic(&mut self, mesh: i32) -> Result<bool, String> {
        let node = self.ent(mesh)?.node;
        let graph = self.graph()?;
        Ok(graph[node]
            .cast::<Mesh>()
            .and_then(|m| m.surfaces().first())
            .is_some_and(|s| crate::classic::is_classic(&s.material().data_ref())))
    }

    /// Imports every surface of a mesh (for whole-mesh operations).
    pub fn all_surfaces(&mut self, mesh: i32) -> Result<Vec<i32>, String> {
        let n = self.count_surfaces(mesh)?;
        (0..n).map(|i| self.surface_handle(mesh, i)).collect()
    }

    /// Uploads every changed surface to Fyrox.
    pub fn flush_meshes(&mut self) -> Result<(), String> {
        if self.scene_opt().is_none() {
            return Ok(());
        }
        self.rebuild_terrains()?;
        let dirty: Vec<usize> =
            self.world.surfaces.iter().enumerate().filter(|(_, s)| s.as_ref().is_some_and(|s| s.dirty)).map(|(i, _)| i).collect();
        for i in dirty {
            let Some(info) = self.world.surfaces[i].as_mut() else { continue };
            info.dirty = false;
            let (entity, index) = (info.entity, info.index);
            let data = surface_data(&info.verts, &info.tris);
            let Ok(node) = self.ent(entity).map(|e| e.node) else {
                self.world.surfaces[i] = None;
                continue;
            };
            let graph = self.graph()?;
            if let Some(m) = graph[node].cast_mut::<Mesh>() {
                if let Some(s) = m.surfaces_mut().get_mut(index) {
                    let material = s.material().clone();
                    *s = SurfaceBuilder::new(SurfaceResource::new_ok(fyrox::core::uuid::Uuid::new_v4(), ResourceKind::Embedded, data))
                        .with_material(material)
                        .build();
                }
            }
        }
        Ok(())
    }

    /// Forgets surfaces of entities that no longer exist.
    pub fn prune_surfaces(&mut self) {
        let alive: Vec<bool> = self.world.surfaces.iter().map(|s| s.as_ref().is_some_and(|s| self.ent(s.entity).is_ok())).collect();
        for (s, ok) in self.world.surfaces.iter_mut().zip(alive) {
            if !ok {
                *s = None;
            }
        }
    }

    /// Applies `f` to every vertex of every surface of a mesh.
    pub fn transform_mesh(&mut self, mesh: i32, f: impl Fn(&mut Vert)) -> Result<(), String> {
        for s in self.all_surfaces(mesh)? {
            self.edit_surface(s, |info| info.verts.iter_mut().for_each(&f))?;
        }
        Ok(())
    }

    pub fn mesh_bounds(&mut self, mesh: i32) -> Result<Option<Bounds>, String> {
        let surfaces = self.all_surfaces(mesh)?;
        let infos: Vec<&SurfaceInfo> = surfaces.iter().filter_map(|&s| self.surface(s).ok()).collect();
        Ok(vertex_bounds(infos.iter().flat_map(|i| i.verts.iter())))
    }

    /// `RotateMesh`: rotates vertices and normals.
    pub fn rotate_mesh(&mut self, mesh: i32, r: Matrix3<f32>) -> Result<(), String> {
        self.transform_mesh(mesh, |v| {
            v.pos = r * v.pos;
            v.normal = r * v.normal;
        })
    }

    pub fn brush(&self, b: i32) -> Result<&Brush, String> {
        usize::try_from(b - 1)
            .ok()
            .and_then(|i| self.world.brushes.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Brush {b} does not exist"))
    }

    pub fn brush_mut(&mut self, b: i32) -> Result<&mut Brush, String> {
        usize::try_from(b - 1)
            .ok()
            .and_then(|i| self.world.brushes.get_mut(i))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("Brush {b} does not exist"))
    }

    /// Entities with a surface painted by brush `b` (refreshed when it changes).
    pub fn brush_users(&self, b: i32) -> Vec<i32> {
        let mut v: Vec<i32> = self.world.surfaces.iter().flatten().filter(|s| s.brush == b).map(|s| s.entity).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Brush painted on surface `index` of an entity, if any.
    pub fn surface_brush(&self, entity: i32, index: usize) -> Option<Brush> {
        self.world
            .surfaces
            .iter()
            .flatten()
            .find(|s| s.entity == entity && s.index == index && s.brush != 0)
            .and_then(|s| self.brush(s.brush).ok().cloned())
    }

    pub fn has_painted_surfaces(&self, entity: i32) -> bool {
        self.world.surfaces.iter().flatten().any(|s| s.entity == entity && s.brush != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> (Vec<Vert>, Vec<[u32; 3]>) {
        let v = |x: f32, y: f32| Vert { pos: Vector3::new(x, y, 0.0), ..Default::default() };
        (vec![v(-1.0, 1.0), v(1.0, 1.0), v(1.0, -1.0), v(-1.0, -1.0)], vec![[0, 1, 2], [0, 2, 3]])
    }

    #[test]
    fn surface_round_trip() {
        let (mut verts, tris) = quad();
        verts[1].color = [1.0, 0.0, 0.5, 1.0];
        verts[2].uv[1] = [0.25, 0.75, 0.0];
        let data = surface_data(&verts, &tris);
        let (back, tris2) = read_surface(&data);
        assert_eq!(tris, tris2);
        assert_eq!(back.len(), 4);
        assert!((back[1].pos - verts[1].pos).norm() < 1e-6);
        assert!((back[1].color[2] - 0.5).abs() < 0.01);
        assert!((back[2].uv[1][1] - 0.75).abs() < 1e-6);
        // the GPU copy is mirrored on X
        let p = data.vertex_buffer.get(1).unwrap().read_3_f32(VertexAttributeUsage::Position).unwrap();
        assert_eq!(p.x, -1.0);
    }

    #[test]
    fn normals_face_the_viewer_of_clockwise_triangles() {
        // Blitz: a quad wound clockwise as seen from -Z faces -Z (towards a default camera)
        let (mut verts, tris) = quad();
        smooth_normals(&mut verts, &tris);
        for v in &verts {
            assert!((v.normal - Vector3::new(0.0, 0.0, -1.0)).norm() < 1e-5, "{:?}", v.normal);
        }
    }

    #[test]
    fn out_of_range_triangles_are_dropped() {
        let (verts, _) = quad();
        let data = surface_data(&verts, &[[0, 1, 9]]);
        assert_eq!(data.geometry_buffer.len(), 0);
    }
}
