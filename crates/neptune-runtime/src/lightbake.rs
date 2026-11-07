//! Baked lighting (ext): `BakeLightmaps`, `ClearLightmaps`.
//!
//! A CPU ray tracer works out light for every texel of a lightmap per
//! surface: sky light (the `AmbientLight` colour) reaching it past the
//! scenery around (ambient occlusion), light bounced once off nearby
//! surfaces (coloured by them), and, if asked, the direct light of the
//! scene's lights with shadows. The result is stored in Fyrox's standard
//! material slot `lightmapTexture`, which is added to the lighting of the
//! surface, so only meshes with the standard (PBR) material are baked;
//! everything that casts shadows in real time blocks light while baking.
//!
//! Without direct light (the default) the lights keep working in real time
//! (moving shadows, highlights) and nothing is lit twice; with it, the
//! lights can be turned off afterwards and static scenery keeps its light.
//!
//! The second texture coordinates the lightmaps use are made by Fyrox's
//! UV generator (it may split vertices along seams). Changing a baked
//! mesh's geometry afterwards removes its lightmap; terrains (rebuilt for
//! level of detail) are never baked.

use std::collections::HashMap;

use fyrox::{
    asset::untyped::ResourceKind,
    core::{
        algebra::{Matrix3, Point3, Vector2, Vector3},
        pool::Handle,
        uuid::Uuid,
    },
    graph::SceneGraph,
    material::{MaterialProperty, MaterialResourceBinding, MaterialResourceExtension, MaterialTextureBinding},
    resource::texture::{Texture, TextureKind, TexturePixelKind, TextureResource},
    scene::{
        light::{directional::DirectionalLight, point::PointLight, spot::SpotLight, BaseLight},
        mesh::{
            buffer::{VertexAttributeUsage, VertexReadTrait},
            Mesh,
        },
        node::Node,
    },
    utils::{lightmap::apply_surface_data_patch, uvgen::generate_uvs},
};

use crate::world::Host3D;

type V3 = Vector3<f32>;

const LIGHTMAP: &str = "lightmapTexture";
/// Location of the second texture coordinates in NeptuneBASIC's vertex layout.
const UV2_LOCATION: u8 = 6;

// ---- ray tracing -------------------------------------------------------

#[derive(Clone, Copy)]
struct Tri {
    a: V3,
    e1: V3,
    e2: V3,
    n: V3,
    albedo: V3,
}

impl Tri {
    fn new(a: V3, b: V3, c: V3, albedo: V3) -> Tri {
        let (e1, e2) = (b - a, c - a);
        let n = e1.cross(&e2).try_normalize(1e-12).unwrap_or_else(V3::y);
        Tri { a, e1, e2, n, albedo }
    }

    /// Möller–Trumbore: distance along the ray, if it hits.
    fn hit(&self, o: &V3, d: &V3) -> Option<f32> {
        let p = d.cross(&self.e2);
        let det = self.e1.dot(&p);
        if det.abs() < 1e-10 {
            return None;
        }
        let inv = 1.0 / det;
        let s = o - self.a;
        let u = s.dot(&p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = s.cross(&self.e1);
        let v = d.dot(&q) * inv;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = self.e2.dot(&q) * inv;
        (t > 1e-5).then_some(t)
    }

    fn bounds(&self) -> (V3, V3) {
        let (b, c) = (self.a + self.e1, self.a + self.e2);
        (self.a.inf(&b).inf(&c), self.a.sup(&b).sup(&c))
    }
}

struct Node3 {
    min: V3,
    max: V3,
    /// leaf: first triangle; branch: index of the left child (right = left + 1)
    first: u32,
    /// triangles in a leaf, 0 for a branch
    count: u32,
}

/// Bounding volume hierarchy over the scene's triangles.
struct Bvh {
    tris: Vec<Tri>,
    nodes: Vec<Node3>,
}

impl Bvh {
    fn new(mut tris: Vec<Tri>) -> Bvh {
        let mut nodes = Vec::with_capacity(tris.len() / 2 + 1);
        nodes.push(Node3 { min: V3::zeros(), max: V3::zeros(), first: 0, count: tris.len() as u32 });
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let (first, count) = (nodes[i].first as usize, nodes[i].count as usize);
            let slice = &mut tris[first..first + count];
            let (mut min, mut max) = (V3::repeat(f32::MAX), V3::repeat(f32::MIN));
            for t in slice.iter() {
                let (a, b) = t.bounds();
                min = min.inf(&a);
                max = max.sup(&b);
            }
            nodes[i].min = min;
            nodes[i].max = max;
            if count <= 4 {
                continue;
            }
            // split at the median of the longest axis
            let ext = max - min;
            let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
            let mid = count / 2;
            slice.select_nth_unstable_by(mid, |p, q| {
                let c = |t: &Tri| t.a[axis] + (t.e1[axis] + t.e2[axis]) / 3.0;
                c(p).total_cmp(&c(q))
            });
            let left = nodes.len();
            nodes.push(Node3 { min: V3::zeros(), max: V3::zeros(), first: first as u32, count: mid as u32 });
            nodes.push(Node3 { min: V3::zeros(), max: V3::zeros(), first: (first + mid) as u32, count: (count - mid) as u32 });
            nodes[i].first = left as u32;
            nodes[i].count = 0;
            stack.push(left);
            stack.push(left + 1);
        }
        Bvh { tris, nodes }
    }

    fn slab(n: &Node3, o: &V3, inv: &V3, tmax: f32) -> bool {
        let (mut t0, mut t1) = (0.0f32, tmax);
        for k in 0..3 {
            let a = (n.min[k] - o[k]) * inv[k];
            let b = (n.max[k] - o[k]) * inv[k];
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
        t0 <= t1
    }

    /// Nearest hit within `tmax`: (distance, triangle).
    fn closest(&self, o: &V3, d: &V3, mut tmax: f32) -> Option<(f32, usize)> {
        if self.tris.is_empty() {
            return None;
        }
        let inv = d.map(|c| 1.0 / if c.abs() < 1e-12 { 1e-12f32.copysign(c) } else { c });
        let mut best = None;
        let mut stack = [0u32; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let n = &self.nodes[stack[sp] as usize];
            if !Self::slab(n, o, &inv, tmax) {
                continue;
            }
            if n.count > 0 {
                for i in n.first as usize..(n.first + n.count) as usize {
                    if let Some(t) = self.tris[i].hit(o, d) {
                        if t < tmax {
                            tmax = t;
                            best = Some((t, i));
                        }
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = n.first;
                stack[sp + 1] = n.first + 1;
                sp += 2;
            }
        }
        best
    }

    fn blocked(&self, o: &V3, d: &V3, tmax: f32) -> bool {
        if self.tris.is_empty() {
            return false;
        }
        let inv = d.map(|c| 1.0 / if c.abs() < 1e-12 { 1e-12f32.copysign(c) } else { c });
        let mut stack = [0u32; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let n = &self.nodes[stack[sp] as usize];
            if !Self::slab(n, o, &inv, tmax) {
                continue;
            }
            if n.count > 0 {
                for i in n.first as usize..(n.first + n.count) as usize {
                    if self.tris[i].hit(o, d).is_some_and(|t| t < tmax) {
                        return true;
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = n.first;
                stack[sp + 1] = n.first + 1;
                sp += 2;
            }
        }
        false
    }
}

// ---- lights ------------------------------------------------------------

#[derive(Clone, Copy)]
enum Light {
    /// `l` points towards the light
    Directional { l: V3, color: V3 },
    Point { p: V3, color: V3, radius: f32 },
    Spot { p: V3, color: V3, radius: f32, axis: V3, edge0: f32, edge1: f32 },
}

fn linear(c: fyrox::core::color::Color) -> V3 {
    let f = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    V3::new(f(c.r), f(c.g), f(c.b))
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

const EPS: f32 = 0.01;

/// Light arriving at `p` (normal `n`) straight from the lights, with shadows.
fn direct(bvh: &Bvh, lights: &[Light], p: &V3, n: &V3) -> V3 {
    let mut sum = V3::zeros();
    let o = p + n * EPS * 2.0;
    for light in lights {
        let (l, dist, k) = match *light {
            Light::Directional { l, color } => (l, 1.0e5, color),
            Light::Point { p: lp, color, radius } => {
                let d = lp - p;
                let dist = d.norm().max(1e-4);
                let att = (1.0 - dist * dist / (radius * radius)).clamp(0.0, 1.0);
                (d / dist, dist, color * att)
            }
            Light::Spot { p: lp, color, radius, axis, edge0, edge1 } => {
                let d = lp - p;
                let dist = d.norm().max(1e-4);
                let l = d / dist;
                let att = (1.0 - dist * dist / (radius * radius)).clamp(0.0, 1.0) * smoothstep(edge0, edge1, l.dot(&axis));
                (l, dist, color * att)
            }
        };
        let ndl = n.dot(&l);
        if ndl <= 0.0 || k.max() < 1e-4 {
            continue;
        }
        if !bvh.blocked(&o, &l, dist - EPS * 2.0) {
            sum += k * ndl;
        }
    }
    sum
}

/// A cheap repeatable random number in [0, 1).
fn hash(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// A cosine-weighted direction around `n`.
fn hemisphere(n: &V3, u1: f32, u2: f32) -> V3 {
    let r = u1.sqrt();
    let phi = std::f32::consts::TAU * u2;
    let t = if n.x.abs() < 0.9 { V3::x() } else { V3::y() };
    let tx = n.cross(&t).normalize();
    let ty = n.cross(&tx);
    (tx * (r * phi.cos()) + ty * (r * phi.sin()) + n * (1.0 - u1).max(0.0).sqrt()).normalize()
}

pub struct Settings {
    pub sky: V3,
    pub samples: u32,
    pub bounce: bool,
    pub direct: bool,
}

/// Light for one texel at `p` with normal `n`.
fn texel_light(bvh: &Bvh, lights: &[Light], s: &Settings, p: &V3, n: &V3, seed: u32) -> V3 {
    let mut sky = 0.0;
    let mut bounced = V3::zeros();
    let o = p + n * EPS;
    let (r1, r2) = (hash(seed), hash(seed ^ 0x9e37_79b9));
    for i in 0..s.samples {
        let u1 = ((i as f32 + r1) / s.samples as f32).fract();
        let u2 = (i as f32 * 0.618_034 + r2).fract();
        let d = hemisphere(n, u1, u2);
        match bvh.closest(&o, &d, 1.0e5) {
            None => sky += 1.0,
            Some((t, tri)) if s.bounce => {
                let tri = &bvh.tris[tri];
                let hn = if tri.n.dot(&d) > 0.0 { -tri.n } else { tri.n };
                bounced += direct(bvh, lights, &(o + d * t), &hn).component_mul(&tri.albedo);
            }
            Some(_) => {}
        }
    }
    let k = 1.0 / s.samples.max(1) as f32;
    let mut c = s.sky * (sky * k) + bounced * k;
    if s.direct {
        c += direct(bvh, lights, p, n);
    }
    c
}

/// One surface to bake: world-space vertices, second UVs and triangles.
struct Target {
    node: Handle<Node>,
    surface: usize,
    pos: Vec<V3>,
    nrm: Vec<V3>,
    uv: Vec<Vector2<f32>>,
    tris: Vec<[u32; 3]>,
}

/// Bakes one surface into an RGB float image of `size` x `size`.
fn bake_target(t: &Target, size: usize, texels_per_unit: f32, bvh: &Bvh, lights: &[Light], s: &Settings) -> Vec<f32> {
    // which triangle (and where in it) each texel shows
    let mut cover: Vec<Option<(V3, V3)>> = vec![None; size * size];
    for tri in &t.tris {
        let [a, b, c] = tri.map(|i| i as usize);
        let (ua, ub, uc) = (t.uv[a] * size as f32, t.uv[b] * size as f32, t.uv[c] * size as f32);
        let area = (ub - ua).perp(&(uc - ua));
        if area.abs() < 1e-12 {
            continue;
        }
        let lo = ua.inf(&ub).inf(&uc).map(|v| (v.floor() as i64 - 1).clamp(0, size as i64 - 1) as usize);
        let hi = ua.sup(&ub).sup(&uc).map(|v| (v.ceil() as i64 + 1).clamp(0, size as i64 - 1) as usize);
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let q = Vector2::new(x as f32 + 0.5, y as f32 + 0.5);
                let w0 = (ub - q).perp(&(uc - q)) / area;
                let w1 = (uc - q).perp(&(ua - q)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -0.01 || w1 < -0.01 || w2 < -0.01 {
                    continue;
                }
                let p = t.pos[a] * w0 + t.pos[b] * w1 + t.pos[c] * w2;
                let n = (t.nrm[a] * w0 + t.nrm[b] * w1 + t.nrm[c] * w2).try_normalize(1e-8).unwrap_or_else(V3::y);
                cover[y * size + x] = Some((p, n));
            }
        }
    }
    // light the covered texels, rows shared among the processor's cores
    let mut out = vec![0.0f32; size * size * 3];
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).max(1);
    let rows_per = size.div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (chunk_i, chunk) in out.chunks_mut(rows_per * size * 3).enumerate() {
            let cover = &cover;
            scope.spawn(move || {
                for (j, px) in chunk.chunks_mut(3).enumerate() {
                    let i = chunk_i * rows_per * size + j;
                    if let Some((p, n)) = cover[i] {
                        let c = texel_light(bvh, lights, s, &p, &n, i as u32 ^ (t.node.index().wrapping_mul(2_654_435_761)));
                        px.copy_from_slice(c.as_slice());
                    }
                }
            });
        }
    });
    // smooth out the noise of the random rays: average with neighbours that
    // show nearby points facing the same way (so edges and seams stay sharp)
    let texel = 1.0 / texels_per_unit.max(0.01);
    for _ in 0..2 {
        let prev = out.clone();
        for y in 0..size {
            for x in 0..size {
                let i = y * size + x;
                let Some((p, n)) = cover[i] else { continue };
                let (mut sum, mut wsum) = (V3::zeros(), 0.0);
                for dy in -2i64..=2 {
                    for dx in -2i64..=2 {
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        if nx < 0 || ny < 0 || nx >= size as i64 || ny >= size as i64 {
                            continue;
                        }
                        let j = ny as usize * size + nx as usize;
                        let Some((q, m)) = cover[j] else { continue };
                        let dist = (q - p).norm() / (texel * 2.5);
                        let w = (-dist * dist).exp() * n.dot(&m).max(0.0).powi(8);
                        if w > 1e-4 {
                            sum += V3::new(prev[j * 3], prev[j * 3 + 1], prev[j * 3 + 2]) * w;
                            wsum += w;
                        }
                    }
                }
                if wsum > 0.0 {
                    out[i * 3..i * 3 + 3].copy_from_slice((sum / wsum).as_slice());
                }
            }
        }
    }
    // spread the edges outwards so filtering never reads unlit texels
    let mut filled: Vec<bool> = cover.iter().map(Option::is_some).collect();
    for _ in 0..3 {
        let prev = filled.clone();
        for y in 0..size {
            for x in 0..size {
                let i = y * size + x;
                if prev[i] {
                    continue;
                }
                let (mut sum, mut k) = ([0.0f32; 3], 0.0);
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= size as i64 || ny >= size as i64 {
                        continue;
                    }
                    let ni = ny as usize * size + nx as usize;
                    if prev[ni] {
                        for c in 0..3 {
                            sum[c] += out[ni * 3 + c];
                        }
                        k += 1.0;
                    }
                }
                if k > 0.0 {
                    for c in 0..3 {
                        out[i * 3 + c] = sum[c] / k;
                    }
                    filled[i] = true;
                }
            }
        }
    }
    out
}

/// Lightmap size for a surface of `area` square units.
fn texture_size(area: f32, texels_per_unit: f32) -> usize {
    // small objects are cut into many pieces by the UV generator: give them room
    ((area.max(0.0).sqrt() * texels_per_unit * 2.2).ceil() as usize).clamp(32, 1024)
}

// ---- the commands ------------------------------------------------------

impl Host3D<'_> {
    /// `BakeLightmaps`: bakes the meshes under `entity` (0 = all). Returns
    /// how many surfaces were baked.
    pub fn bake_lightmaps(&mut self, texels_per_unit: f32, quality: i32, with_direct: bool, entity: i32) -> Result<i32, String> {
        if !(0..=2).contains(&quality) {
            return Err(format!("BakeLightmaps: quality must be 0 to 2, not {quality}"));
        }
        self.flush_meshes()?;
        let root = if entity != 0 { Some(self.ent(entity)?.node) } else { None };
        let terrains: Vec<Handle<Node>> = self.entities().filter(|(_, e)| e.terrain.is_some()).map(|(_, e)| e.node).collect();
        let scene_id = self.world.current_scene;
        let scene = self.scene()?;
        let sky = linear(scene.rendering_options.ambient_lighting_color);
        let graph = &mut scene.graph;
        graph.update_hierarchical_data();

        let inside: Vec<Handle<Node>> = match root {
            Some(r) => graph.traverse_handle_iter(r).collect(),
            None => graph.pair_iter().map(|(h, _)| h).collect(),
        };

        // lights
        let mut lights = Vec::new();
        for (_, node) in graph.pair_iter() {
            if !node.is_globally_enabled() || !node.global_visibility() {
                continue;
            }
            let base: &BaseLight = if let Some(l) = node.cast::<DirectionalLight>() {
                l.base_light_ref()
            } else if let Some(l) = node.cast::<PointLight>() {
                l.base_light_ref()
            } else if let Some(l) = node.cast::<SpotLight>() {
                l.base_light_ref()
            } else {
                continue;
            };
            let color = linear(base.color()) * base.intensity();
            let up = node.up_vector().try_normalize(1e-6).unwrap_or_else(V3::y);
            if node.cast::<DirectionalLight>().is_some() {
                lights.push(Light::Directional { l: up, color });
            } else if let Some(p) = node.cast::<PointLight>() {
                lights.push(Light::Point { p: node.global_position(), color, radius: p.radius().max(1e-3) });
            } else if let Some(sp) = node.cast::<SpotLight>() {
                lights.push(Light::Spot {
                    p: node.global_position(),
                    color,
                    radius: sp.distance().max(1e-3),
                    axis: up,
                    edge0: ((sp.hotspot_cone_angle() + sp.falloff_angle_delta()) * 0.5).cos(),
                    edge1: (sp.hotspot_cone_angle() * 0.5).cos(),
                });
            }
        }

        // what blocks light: everything that casts shadows in real time
        let mut tris = Vec::new();
        for (_, node) in graph.pair_iter() {
            let Some(mesh) = node.cast::<Mesh>() else { continue };
            if !mesh.global_visibility() || !mesh.is_globally_enabled() || !mesh.cast_shadows() {
                continue;
            }
            let m = mesh.global_transform();
            for surface in mesh.surfaces() {
                let mat = surface.material().data_ref();
                let casts = mat.shader().data_ref().as_loaded_ref().is_some_and(|s| {
                    s.definition.passes.iter().any(|p| p.name == "DirectionalShadow") && !s.definition.disabled_passes.iter().any(|p| p == "DirectionalShadow")
                });
                if !casts {
                    continue;
                }
                let albedo = match mat.as_loaded_ref().and_then(|m| m.property_group_ref("properties")).and_then(|g| g.property_ref("diffuseColor")) {
                    Some(MaterialProperty::Color(c)) => linear(*c),
                    _ => V3::repeat(0.6),
                };
                drop(mat);
                let data = surface.data();
                let data = data.data_ref();
                let pos: Vec<V3> = data
                    .vertex_buffer
                    .iter()
                    .map(|v| m.transform_point(&Point3::from(v.read_3_f32(VertexAttributeUsage::Position).unwrap_or_default())).coords)
                    .collect();
                for t in data.geometry_buffer.iter() {
                    let [a, b, c] = t.0.map(|i| i as usize);
                    if a < pos.len() && b < pos.len() && c < pos.len() {
                        tris.push(Tri::new(pos[a], pos[b], pos[c], albedo));
                    }
                }
            }
        }

        // the surfaces to bake: standard material, inside the chosen entity
        let mut targets = Vec::new();
        let mut patched = std::collections::HashSet::new();
        for h in inside {
            if terrains.contains(&h) {
                continue;
            }
            let Some(mesh) = graph[h].cast::<Mesh>() else { continue };
            if !mesh.global_visibility() || !mesh.is_globally_enabled() {
                continue;
            }
            let m = mesh.global_transform();
            let nm: Matrix3<f32> = m.fixed_view::<3, 3>(0, 0).try_inverse().map(|i| i.transpose()).unwrap_or_else(Matrix3::identity);
            for (si, surface) in mesh.surfaces().iter().enumerate() {
                let lightmapped = surface.material().data_ref().shader().data_ref().as_loaded_ref().is_some_and(|s| s.has_texture_resource(LIGHTMAP));
                if !lightmapped {
                    continue;
                }
                let resource = surface.data();
                let mut data = resource.data_ref();
                // second UVs, once per shared geometry
                if patched.insert(resource.key()) {
                    let positions: Vec<V3> = data.vertex_buffer.iter().map(|v| v.read_3_f32(VertexAttributeUsage::Position).unwrap_or_default()).collect();
                    let triangles: Vec<[u32; 3]> = data.geometry_buffer.iter().map(|t| t.0).collect();
                    let Some(patch) = generate_uvs(positions.into_iter(), triangles.into_iter(), 0.005) else { continue };
                    apply_surface_data_patch(&mut data, &patch, UV2_LOCATION);
                }
                let mut pos = Vec::new();
                let mut nrm = Vec::new();
                let mut uv = Vec::new();
                for v in data.vertex_buffer.iter() {
                    pos.push(m.transform_point(&Point3::from(v.read_3_f32(VertexAttributeUsage::Position).unwrap_or_default())).coords);
                    nrm.push((nm * v.read_3_f32(VertexAttributeUsage::Normal).unwrap_or_else(|_| V3::y())).try_normalize(1e-8).unwrap_or_else(V3::y));
                    uv.push(v.read_2_f32(VertexAttributeUsage::TexCoord1).unwrap_or_default());
                }
                let tris = data.geometry_buffer.iter().map(|t| t.0).collect();
                targets.push(Target { node: h, surface: si, pos, nrm, uv, tris });
            }
        }

        let bvh = Bvh::new(tris);
        let settings = Settings { sky, samples: [16, 64, 256][quality as usize], bounce: quality > 0, direct: with_direct };
        let mut baked: Vec<(Handle<Node>, usize, TextureResource)> = Vec::new();
        for t in &targets {
            let area: f32 = t.tris.iter().map(|tri| (t.pos[tri[1] as usize] - t.pos[tri[0] as usize]).cross(&(t.pos[tri[2] as usize] - t.pos[tri[0] as usize])).norm() * 0.5).sum();
            let size = texture_size(area, texels_per_unit);
            let image = bake_target(t, size, texels_per_unit, &bvh, &lights, &settings);
            let bytes: Vec<u8> = image.iter().flat_map(|f| f.to_le_bytes()).collect();
            let kind = TextureKind::Rectangle { width: size as u32, height: size as u32 };
            let texture = Texture::from_bytes(kind, TexturePixelKind::RGB32F, bytes).ok_or("BakeLightmaps: could not make a lightmap texture")?;
            baked.push((t.node, t.surface, TextureResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, texture)));
        }

        // give each surface its own material with its lightmap
        for (h, si, tex) in &baked {
            let entry = self.world.lightmaps.entry((scene_id, *h)).or_default();
            if entry.len() <= *si {
                entry.resize(si + 1, None);
            }
            entry[*si] = Some(tex.clone());
        }
        let nodes: Vec<Handle<Node>> = baked.iter().map(|(h, _, _)| *h).collect();
        for h in nodes {
            self.bind_lightmaps(h)?;
        }
        Ok(baked.len() as i32)
    }

    /// Binds a node's baked lightmaps to its surfaces (each gets its own
    /// copy of its material). Done again after the entity's materials are
    /// rebuilt.
    pub fn bind_lightmaps(&mut self, node: Handle<Node>) -> Result<(), String> {
        let key = (self.world.current_scene, node);
        let Some(maps) = self.world.lightmaps.get(&key).cloned() else { return Ok(()) };
        let graph = self.graph()?;
        let Some(mesh) = graph.try_get_mut(node).ok().and_then(|n| n.cast_mut::<Mesh>()) else { return Ok(()) };
        for (surface, tex) in mesh.surfaces_mut().iter_mut().zip(maps) {
            let Some(tex) = tex else { continue };
            let material = surface.material().deep_copy();
            material.data_ref().bind(LIGHTMAP, MaterialResourceBinding::Texture(MaterialTextureBinding { value: Some(tex) }));
            surface.set_material(material);
        }
        Ok(())
    }

    /// Rebinds the lightmaps of all of an entity's meshes.
    pub fn rebind_entity_lightmaps(&mut self, e: i32) -> Result<(), String> {
        if self.world.lightmaps.is_empty() {
            return Ok(());
        }
        for n in self.entity_mesh_nodes(e)? {
            self.bind_lightmaps(n)?;
        }
        Ok(())
    }

    /// The geometry of a baked surface changed: its lightmap no longer fits.
    pub fn drop_lightmap(&mut self, node: Handle<Node>, surface: usize) -> Result<(), String> {
        let key = (self.world.current_scene, node);
        let Some(entry) = self.world.lightmaps.get_mut(&key) else { return Ok(()) };
        if entry.get(surface).is_none_or(Option::is_none) {
            return Ok(());
        }
        entry[surface] = None;
        let graph = self.graph()?;
        if let Some(s) = graph[node].cast_mut::<Mesh>().and_then(|m| m.surfaces_mut().get_mut(surface)) {
            let material = s.material().deep_copy();
            material.data_ref().unbind(LIGHTMAP);
            s.set_material(material);
        }
        Ok(())
    }

    /// `ClearLightmaps`: removes baked light from the meshes under `entity` (0 = all).
    pub fn clear_lightmaps(&mut self, entity: i32) -> Result<(), String> {
        let scene_id = self.world.current_scene;
        let keep: Option<Vec<Handle<Node>>> = if entity != 0 {
            let root = self.ent(entity)?.node;
            Some(self.graph()?.traverse_handle_iter(root).collect())
        } else {
            None
        };
        let keys: Vec<(usize, Handle<Node>)> =
            self.world.lightmaps.keys().filter(|(s, h)| *s == scene_id && keep.as_ref().is_none_or(|k| k.contains(h))).copied().collect();
        for (s, h) in keys {
            let count = self.world.lightmaps.get(&(s, h)).map_or(0, Vec::len);
            for i in 0..count {
                self.drop_lightmap(h, i)?;
            }
            self.world.lightmaps.remove(&(s, h));
        }
        Ok(())
    }
}

/// Baked lightmaps per (scene, mesh node), one per surface.
pub type Lightmaps = HashMap<(usize, Handle<Node>), Vec<Option<TextureResource>>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(y: f32, size: f32) -> Vec<Tri> {
        let (a, b, c, d) = (V3::new(-size, y, -size), V3::new(size, y, -size), V3::new(size, y, size), V3::new(-size, y, size));
        vec![Tri::new(a, b, c, V3::repeat(0.5)), Tri::new(a, c, d, V3::repeat(0.5))]
    }

    #[test]
    fn rays_hit_the_nearest_triangle() {
        let mut tris = quad(0.0, 1.0);
        tris.extend(quad(2.0, 1.0));
        tris.extend((0..50).flat_map(|i| quad(10.0 + i as f32, 0.1)));
        let bvh = Bvh::new(tris);
        let (t, _) = bvh.closest(&V3::new(0.2, 5.0, 0.3), &V3::new(0.0, -1.0, 0.0), 100.0).unwrap();
        assert!((t - 3.0).abs() < 1e-4);
        assert!(bvh.blocked(&V3::new(0.0, 1.0, 0.0), &V3::y(), 10.0));
        assert!(!bvh.blocked(&V3::new(5.0, 1.0, 0.0), &V3::y(), 10.0));
    }

    #[test]
    fn sky_light_and_shadows() {
        // a floor under a roof: no sky, and the sun is blocked
        // (wide enough that grazing rays cannot slip out at the sides)
        let mut tris = quad(0.0, 200.0);
        tris.extend(quad(3.0, 200.0));
        let bvh = Bvh::new(tris);
        let lights = [Light::Directional { l: V3::y(), color: V3::repeat(1.0) }];
        let s = Settings { sky: V3::repeat(1.0), samples: 32, bounce: false, direct: true };
        let under = texel_light(&bvh, &lights, &s, &V3::new(0.0, 0.0, 0.0), &V3::y(), 1);
        assert!(under.max() < 0.05, "{under}");
        // on top of the roof: full sky and sun
        let top = texel_light(&bvh, &lights, &s, &V3::new(0.0, 3.0, 0.0), &V3::y(), 1);
        assert!((top.x - 2.0).abs() < 0.05, "{top}");
    }

    #[test]
    fn hemisphere_samples_face_the_normal() {
        let n = V3::new(0.3, 0.8, -0.2).normalize();
        for i in 0..64 {
            let d = hemisphere(&n, hash(i), hash(i + 99));
            assert!(d.dot(&n) >= -1e-4 && (d.norm() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn sizes() {
        assert_eq!(texture_size(0.0, 8.0), 32);
        assert_eq!(texture_size(1.0e9, 8.0), 1024);
        assert!(texture_size(100.0, 4.0) >= 60);
    }
}
