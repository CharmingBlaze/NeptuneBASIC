//! Terrains (`CreateTerrain`, `LoadTerrain`, `ModifyTerrain`, `TerrainY`, ...).
//!
//! A terrain is a mesh entity whose single surface is a regular grid built
//! from a height array (heights 0..1, one per grid vertex). Like Blitz3D,
//! a terrain of size `n` spans 0..n on X and Z and 0..1 on Y before
//! `ScaleEntity`; texture coordinates are one unit per grid square, so
//! `ScaleTexture tex, n, n` stretches a texture over the whole terrain.
//! Normals are computed for the entity's scale at build time, so a terrain
//! scaled to be tall still lights correctly; it is rebuilt when the scale
//! or the heights change.
//!
//! Painted layers (ext): `TerrainLayer` puts up to three textures over the
//! base texture (layer 0), and `PaintTerrain` or `LoadTerrainLayers` sets
//! how much of each shows at every grid vertex. The weights travel in the
//! vertex colours (red = layer 1, green = 2, blue = 3) and the classic
//! shader mixes the layers by them.
//!
//! Level of detail: `TerrainDetail` with a triangle budget splits the grid
//! into chunks of `CHUNK` squares and draws far chunks with fewer, larger
//! squares, choosing each chunk's step every render so the whole terrain
//! uses about that many triangles. Where a chunk meets a coarser one, its
//! edge follows the coarser edge, so no gaps open between them. Collisions
//! and picks use the full grid.

use fyrox::core::algebra::Vector3;

use crate::mesh::Vert;
use crate::world::{Host3D, Kind};

#[derive(Clone, Debug)]
pub struct TerrainState {
    /// Grid squares per side; there are `size + 1` vertices per side.
    pub size: usize,
    pub heights: Vec<f32>,
    pub surface: i32,
    /// Scale the geometry was last built for, `None` when stale.
    pub built_for: Option<Vector3<f32>>,
    /// Painted weights of layers 1-3 per grid vertex (empty: not painted).
    pub weights: Vec<[f32; 3]>,
    /// `TerrainDetail` triangle budget (0 = full grid).
    pub budget: i32,
    /// The step of each chunk (row by row) when `budget` is set.
    pub steps: Vec<u32>,
}

/// Grid squares per side of a level-of-detail chunk.
pub const CHUNK: usize = 16;

impl TerrainState {
    pub fn new(size: usize, surface: i32) -> Self {
        TerrainState { size, heights: vec![0.0; (size + 1) * (size + 1)], surface, built_for: None, weights: Vec::new(), budget: 0, steps: Vec::new() }
    }

    fn idx(&self, x: usize, z: usize) -> usize {
        z * (self.size + 1) + x
    }

    /// Height at a grid vertex (clamped to the grid).
    pub fn height(&self, x: i32, z: i32) -> f32 {
        let c = |v: i32| v.clamp(0, self.size as i32) as usize;
        self.heights[self.idx(c(x), c(z))]
    }

    pub fn set_height(&mut self, x: i32, z: i32, h: f32) -> Result<(), String> {
        let n = self.size as i32;
        if !(0..=n).contains(&x) || !(0..=n).contains(&z) {
            return Err(format!("terrain position {x},{z} out of range (0-{n})"));
        }
        let i = self.idx(x as usize, z as usize);
        self.heights[i] = h.clamp(0.0, 1.0);
        self.built_for = None;
        Ok(())
    }

    /// Height (0..1) at a local position, interpolated across the grid triangle.
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        let n = self.size as f32;
        let (x, z) = (x.clamp(0.0, n), z.clamp(0.0, n));
        let (x0, z0) = (x.floor().min(n - 1.0).max(0.0), z.floor().min(n - 1.0).max(0.0));
        let (fx, fz) = (x - x0, z - z0);
        let (xi, zi) = (x0 as i32, z0 as i32);
        let h00 = self.height(xi, zi);
        let h10 = self.height(xi + 1, zi);
        let h01 = self.height(xi, zi + 1);
        let h11 = self.height(xi + 1, zi + 1);
        // the two triangles of a square share the (x, z+1)-(x+1, z) diagonal
        if fx + fz <= 1.0 {
            h00 + (h10 - h00) * fx + (h01 - h00) * fz
        } else {
            h11 + (h01 - h11) * (1.0 - fx) + (h10 - h11) * (1.0 - fz)
        }
    }

    pub fn painted(&self) -> bool {
        !self.weights.is_empty()
    }

    /// One grid vertex: position, normal for `s` (the scale), UVs and painted weights.
    fn vertex(&self, x: usize, z: usize, s: Vector3<f32>) -> Vert {
        let (xi, zi) = (x as i32, z as i32);
        // world-space slope, then mapped back through the scale so the
        // shader's (non inverse-transposed) normal transform restores it
        let dx = (self.height(xi + 1, zi) - self.height(xi - 1, zi)) * s.y / (2.0 * s.x);
        let dz = (self.height(xi, zi + 1) - self.height(xi, zi - 1)) * s.y / (2.0 * s.z);
        let world_n = Vector3::new(-dx, 1.0, -dz);
        let local_n = Vector3::new(world_n.x / s.x, world_n.y / s.y, world_n.z / s.z).normalize();
        let uv = [x as f32, (self.size - z) as f32, 0.0];
        let color = match self.weights.get(self.idx(x, z)) {
            Some(w) => [w[0], w[1], w[2], 1.0],
            None => [1.0; 4],
        };
        Vert { pos: Vector3::new(x as f32, self.height(xi, zi), z as f32), normal: local_n, uv: [uv, uv], color }
    }

    /// Adds `amount` (0..1, negative removes) of `layer` (0-3) at grid
    /// vertex (x, z). Painting a layer also uncovers it from the layers
    /// above; painting layer 0 uncovers the base texture.
    pub fn paint_vertex(&mut self, layer: usize, x: usize, z: usize, amount: f32) {
        if self.weights.is_empty() {
            self.weights = vec![[0.0; 3]; self.heights.len()];
        }
        let i = self.idx(x, z);
        let w = &mut self.weights[i];
        for (j, v) in w.iter_mut().enumerate() {
            let l = j + 1;
            if l == layer {
                *v = (*v + amount).clamp(0.0, 1.0);
            } else if l > layer && amount > 0.0 {
                *v = (*v - amount).clamp(0.0, 1.0);
            }
        }
        self.built_for = None;
    }

    /// `ModifyTerrainLayer`: how much painted layer 1-3 covers grid point (x, z).
    pub fn set_layer(&mut self, x: i32, z: i32, layer: i32, weight: f32) -> Result<(), String> {
        let n = self.size as i32;
        if !(0..=n).contains(&x) || !(0..=n).contains(&z) {
            return Err(format!("terrain position {x},{z} out of range (0-{n})"));
        }
        if !(1..=3).contains(&layer) {
            return Err(format!("ModifyTerrainLayer: layer must be 1 to 3, not {layer}"));
        }
        if self.weights.is_empty() {
            self.weights = vec![[0.0; 3]; self.heights.len()];
        }
        let i = self.idx(x as usize, z as usize);
        self.weights[i][layer as usize - 1] = weight.clamp(0.0, 1.0);
        self.built_for = None;
        Ok(())
    }

    /// `PaintTerrain` in grid units: a soft round brush.
    pub fn paint(&mut self, layer: usize, cx: f32, cz: f32, radius: f32, strength: f32) {
        let r = radius.max(0.5);
        let n = self.size as i32;
        let (x0, x1) = (((cx - r).floor() as i32).max(0), ((cx + r).ceil() as i32).min(n));
        let (z0, z1) = (((cz - r).floor() as i32).max(0), ((cz + r).ceil() as i32).min(n));
        for z in z0..=z1 {
            for x in x0..=x1 {
                let d = ((x as f32 - cx).powi(2) + (z as f32 - cz).powi(2)).sqrt();
                if d < r {
                    let f = 1.0 - d / r;
                    self.paint_vertex(layer, x as usize, z as usize, strength * f * f * (3.0 - 2.0 * f));
                }
            }
        }
    }

    /// How much of `layer` (0-3) shows at grid vertex i, after the layers above it.
    fn visible(&self, i: usize, layer: usize) -> f32 {
        let w = self.weights.get(i).copied().unwrap_or([0.0; 3]);
        let mut v = if layer == 0 { 1.0 } else { w[layer - 1] };
        for &above in &w[layer.min(3)..] {
            v *= 1.0 - above;
        }
        v
    }

    /// `TerrainLayerWeight` at a local position (blended between grid vertices).
    pub fn layer_weight(&self, layer: usize, x: f32, z: f32) -> f32 {
        let n = self.size as f32;
        let (x, z) = (x.clamp(0.0, n), z.clamp(0.0, n));
        let (x0, z0) = (x.floor().min(n - 1.0).max(0.0) as usize, z.floor().min(n - 1.0).max(0.0) as usize);
        let (fx, fz) = (x - x0 as f32, z - z0 as f32);
        let v = |x: usize, z: usize| self.visible(self.idx(x.min(self.size), z.min(self.size)), layer);
        let near = v(x0, z0) * (1.0 - fx) + v(x0 + 1, z0) * fx;
        let far = v(x0, z0 + 1) * (1.0 - fx) + v(x0 + 1, z0 + 1) * fx;
        near * (1.0 - fz) + far * fz
    }

    fn chunks(&self) -> usize {
        self.size.div_ceil(CHUNK)
    }

    /// Picks each chunk's step so the terrain uses about `budget` triangles;
    /// `eye` is the camera in the terrain's grid space. True if any changed.
    pub fn choose_steps(&mut self, eye: Vector3<f32>) -> bool {
        let c = self.chunks();
        let centres: Vec<f32> = (0..c * c)
            .map(|i| {
                let (cx, cz) = (i % c, i / c);
                let mid = |k: usize| ((k * CHUNK) as f32 + ((k + 1) * CHUNK).min(self.size) as f32) / 2.0;
                let (mx, mz) = (mid(cx), mid(cz));
                let y = self.sample(mx, mz);
                let d = Vector3::new(mx - eye.x, y - eye.y, mz - eye.z).norm();
                (d - CHUNK as f32 * 0.71).max(0.0)
            })
            .collect();
        let tris = |k: f32| centres.iter().map(|&d| 2 * (CHUNK as i64 / step_for(d, k) as i64).pow(2)).sum::<i64>();
        // the largest detail distance that keeps within the budget
        let (mut lo, mut hi) = (0.01f32, 1.0e6f32);
        for _ in 0..40 {
            let mid = (lo * hi).sqrt();
            if tris(mid) <= self.budget as i64 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let steps: Vec<u32> = centres.iter().map(|&d| step_for(d, lo)).collect();
        let changed = steps != self.steps;
        if changed {
            self.steps = steps;
            self.built_for = None;
        }
        changed
    }

    /// The grid lines of chunk `k` (along one axis) with a step.
    fn chunk_line(&self, k: usize, step: usize) -> Vec<usize> {
        let (a, b) = (k * CHUNK, ((k + 1) * CHUNK).min(self.size));
        let mut v: Vec<usize> = (a..b).step_by(step.max(1)).collect();
        v.push(b);
        v
    }

    /// Height at `along` on a chunk edge as a coarser neighbour draws it:
    /// interpolated between the neighbour's points (`line`). `at` gives the
    /// grid position of a point on the edge.
    fn edge_height(&self, along: usize, line: &[usize], at: impl Fn(usize) -> (usize, usize)) -> f32 {
        let h = |p: usize| {
            let (x, z) = at(p);
            self.heights[self.idx(x, z)]
        };
        match line.windows(2).find(|w| w[0] <= along && along <= w[1]) {
            Some(w) if w[1] > w[0] => {
                let t = (along - w[0]) as f32 / (w[1] - w[0]) as f32;
                h(w[0]) + (h(w[1]) - h(w[0])) * t
            }
            _ => h(along),
        }
    }

    /// Chunked geometry with the chosen steps. Where a chunk meets a coarser
    /// neighbour, its edge follows the neighbour's edge so no gaps open.
    fn chunked_geometry(&self, s: Vector3<f32>) -> (Vec<Vert>, Vec<[u32; 3]>) {
        let c = self.chunks();
        let step_of = |cx: usize, cz: usize| self.steps.get(cz * c + cx).copied().unwrap_or(1) as usize;
        let mut verts = Vec::new();
        let mut tris = Vec::new();
        for cz in 0..c {
            for cx in 0..c {
                let step = step_of(cx, cz);
                let (xs, zs) = (self.chunk_line(cx, step), self.chunk_line(cz, step));
                // coarser neighbours' points along each shared edge
                let coarser = |n: Option<(usize, usize)>, k: usize| {
                    n.map(|(nx, nz)| step_of(nx, nz)).filter(|&ns| ns > step).map(|ns| self.chunk_line(k, ns))
                };
                let left = coarser(cx.checked_sub(1).map(|x| (x, cz)), cz);
                let right = coarser((cx + 1 < c).then_some((cx + 1, cz)), cz);
                let near = coarser(cz.checked_sub(1).map(|z| (cx, z)), cx);
                let far = coarser((cz + 1 < c).then_some((cx, cz + 1)), cx);
                let (w, rows) = (xs.len() as u32, zs.len() as u32);
                let base = verts.len() as u32;
                for (j, &z) in zs.iter().enumerate() {
                    for (i, &x) in xs.iter().enumerate() {
                        let mut v = self.vertex(x, z, s);
                        if i == 0 {
                            if let Some(l) = &left {
                                v.pos.y = self.edge_height(z, l, |p| (x, p));
                            }
                        }
                        if i == xs.len() - 1 {
                            if let Some(l) = &right {
                                v.pos.y = self.edge_height(z, l, |p| (x, p));
                            }
                        }
                        if j == 0 {
                            if let Some(l) = &near {
                                v.pos.y = self.edge_height(x, l, |p| (p, z));
                            }
                        }
                        if j == zs.len() - 1 {
                            if let Some(l) = &far {
                                v.pos.y = self.edge_height(x, l, |p| (p, z));
                            }
                        }
                        verts.push(v);
                    }
                }
                for j in 0..rows - 1 {
                    for i in 0..w - 1 {
                        let (a, b, cc, d) = (base + j * w + i, base + j * w + i + 1, base + (j + 1) * w + i, base + (j + 1) * w + i + 1);
                        // clockwise seen from above (Blitz front faces)
                        tris.push([cc, d, b]);
                        tris.push([cc, b, a]);
                    }
                }
            }
        }
        (verts, tris)
    }

    /// The full grid's triangles in local space (for collisions and picks).
    pub fn full_triangles(&self) -> Vec<[Vector3<f32>; 3]> {
        let one = Vector3::new(1.0, 1.0, 1.0);
        let (verts, tris) = self.full_geometry(one);
        tris.iter().map(|t| t.map(|i| verts[i as usize].pos)).collect()
    }

    /// Grid geometry in local Blitz space, normals adjusted for `scale`.
    pub fn geometry(&self, scale: Vector3<f32>) -> (Vec<Vert>, Vec<[u32; 3]>) {
        let s = scale.map(|c| if c.abs() < 1e-6 { 1e-6 } else { c });
        if self.budget > 0 && !self.steps.is_empty() {
            return self.chunked_geometry(s);
        }
        self.full_geometry(s)
    }

    fn full_geometry(&self, s: Vector3<f32>) -> (Vec<Vert>, Vec<[u32; 3]>) {
        let n = self.size;
        let mut verts = Vec::with_capacity((n + 1) * (n + 1));
        for z in 0..=n {
            for x in 0..=n {
                verts.push(self.vertex(x, z, s));
            }
        }
        let mut tris = Vec::with_capacity(n * n * 2);
        let w = (n + 1) as u32;
        for z in 0..n as u32 {
            for x in 0..n as u32 {
                let (a, b, c, d) = (z * w + x, z * w + x + 1, (z + 1) * w + x, (z + 1) * w + x + 1);
                // clockwise seen from above (Blitz front faces)
                tris.push([c, d, b]);
                tris.push([c, b, a]);
            }
        }
        (verts, tris)
    }
}

/// The step for a chunk `d` units away when steps double every `k` units.
fn step_for(d: f32, k: f32) -> u32 {
    let mut s = 1u32;
    while (s as usize) < CHUNK && d > k * s as f32 {
        s *= 2;
    }
    s
}

impl Host3D<'_> {
    pub fn create_terrain(&mut self, size: i32, parent: i32) -> Result<i32, String> {
        self.scene()?;
        if !(1..=4096).contains(&size) {
            return Err(format!("terrain size must be 1-4096, got {size}"));
        }
        let id = self.add_entity(crate::mesh::empty_mesh_node(), Kind::Mesh, parent)?;
        let surface = self.create_surface(id, 0)?;
        self.ent_mut(id)?.terrain = Some(TerrainState::new(size as usize, surface));
        self.rebuild_terrains()?;
        Ok(id)
    }

    /// `LoadTerrain`: a greyscale heightmap; the top of the image is the far (+Z) edge.
    pub fn load_terrain(&mut self, path: &str, parent: i32) -> Result<i32, String> {
        let img = match image::open(path) {
            Ok(i) => i.to_luma16(),
            Err(e) => {
                eprintln!("LoadTerrain: cannot load '{path}': {e}");
                return Ok(0);
            }
        };
        let (w, h) = img.dimensions();
        let size = w.max(h).max(2) as i32;
        let id = self.create_terrain(size, parent)?;
        let t = self.terrain_mut(id)?;
        for z in 0..=size {
            for x in 0..=size {
                let px = (x as u32).min(w - 1);
                let py = h - 1 - (z as u32).min(h - 1);
                let v = img.get_pixel(px, py)[0] as f32 / 65535.0;
                t.set_height(x, z, v)?;
            }
        }
        self.rebuild_terrains()?;
        Ok(id)
    }

    pub fn terrain(&self, e: i32) -> Result<&TerrainState, String> {
        self.ent(e)?.terrain.as_ref().ok_or_else(|| format!("Entity {e} is not a terrain"))
    }

    pub fn terrain_mut(&mut self, e: i32) -> Result<&mut TerrainState, String> {
        self.ent_mut(e)?.terrain.as_mut().ok_or_else(|| format!("Entity {e} is not a terrain"))
    }

    /// Rebuilds terrains whose heights or scale changed (before rendering).
    pub fn rebuild_terrains(&mut self) -> Result<(), String> {
        let stale: Vec<i32> = self
            .entities()
            .filter(|(_, e)| e.terrain.as_ref().is_some_and(|t| t.built_for != Some(e.xf.scale)))
            .map(|(i, _)| i)
            .collect();
        for e in stale {
            let scale = self.ent(e)?.xf.scale;
            let (geometry, surface) = {
                let t = self.terrain(e)?;
                (t.geometry(scale), t.surface)
            };
            self.edit_surface(surface, |s| {
                s.verts = geometry.0;
                s.tris = geometry.1;
            })?;
            self.terrain_mut(e)?.built_for = Some(scale);
        }
        Ok(())
    }

    /// The grid position (x, z) and grid-unit scale of a world point on a terrain.
    fn terrain_local(&self, e: i32, x: f32, z: f32) -> Result<(f32, f32, f32), String> {
        let w = self.world_of(e)?;
        let local = w.to_local_point(&Vector3::new(x, w.position().y, z));
        let per_unit = 2.0 / (w.scale.x.abs() + w.scale.z.abs()).max(1e-6);
        Ok((local.x, local.z, per_unit))
    }

    /// `TerrainLayer`: a texture for layer 0 (the base) to 3; layers 1-3 are painted on.
    pub fn terrain_layer(&mut self, e: i32, texture: i32, layer: i32) -> Result<(), String> {
        if !(0..=3).contains(&layer) {
            return Err(format!("terrain layer must be 0 to 3, not {layer}"));
        }
        if texture != 0 {
            self.texture(texture)?;
        }
        let t = self.terrain_mut(e)?;
        if layer > 0 && t.weights.is_empty() {
            t.weights = vec![[0.0; 3]; t.heights.len()];
            t.built_for = None;
        }
        self.ent_mut(e)?.textures[layer as usize] = texture;
        self.refresh_material(e)
    }

    /// `PaintTerrain` at a world position, radius in world units.
    pub fn paint_terrain(&mut self, e: i32, layer: i32, x: f32, z: f32, radius: f32, strength: f32) -> Result<(), String> {
        if !(0..=3).contains(&layer) {
            return Err(format!("terrain layer must be 0 to 3, not {layer}"));
        }
        let (lx, lz, per_unit) = self.terrain_local(e, x, z)?;
        let was_painted = self.terrain(e)?.painted();
        self.terrain_mut(e)?.paint(layer as usize, lx, lz, radius * per_unit, strength);
        if !was_painted {
            self.refresh_material(e)?;
        }
        Ok(())
    }

    /// `TerrainLayerWeight`: how much of a layer shows at a world position (0-1).
    pub fn terrain_layer_weight(&self, e: i32, layer: i32, x: f32, z: f32) -> Result<f32, String> {
        if !(0..=3).contains(&layer) {
            return Err(format!("terrain layer must be 0 to 3, not {layer}"));
        }
        let (lx, lz, _) = self.terrain_local(e, x, z)?;
        Ok(self.terrain(e)?.layer_weight(layer as usize, lx, lz))
    }

    /// `LoadTerrainLayers`: weights from an image's red, green and blue
    /// (layers 1, 2, 3); the top of the image is the far (+Z) edge.
    pub fn load_terrain_layers(&mut self, e: i32, path: &str) -> Result<(), String> {
        let img = image::open(path).map_err(|err| format!("cannot load '{path}': {err}"))?.to_rgb8();
        let (w, h) = img.dimensions();
        let t = self.terrain_mut(e)?;
        let n = t.size;
        let mut weights = vec![[0.0; 3]; t.heights.len()];
        for z in 0..=n {
            for x in 0..=n {
                let px = ((x as f32 / n as f32) * (w - 1) as f32).round() as u32;
                let py = h - 1 - ((z as f32 / n as f32) * (h - 1) as f32).round() as u32;
                let p = img.get_pixel(px, py);
                weights[z * (n + 1) + x] = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0];
            }
        }
        t.weights = weights;
        t.built_for = None;
        self.refresh_material(e)
    }

    /// `TerrainDetail`: a triangle budget for level of detail (0 = full grid).
    pub fn terrain_detail(&mut self, e: i32, budget: i32) -> Result<(), String> {
        let t = self.terrain_mut(e)?;
        t.budget = budget.max(0);
        t.steps.clear();
        t.built_for = None;
        Ok(())
    }

    /// Chooses level-of-detail steps for every terrain from the first active camera.
    pub fn update_terrain_lods(&mut self) -> Result<(), String> {
        let lod: Vec<i32> = self.entities().filter(|(_, e)| e.terrain.as_ref().is_some_and(|t| t.budget > 0)).map(|(i, _)| i).collect();
        if lod.is_empty() {
            return Ok(());
        }
        let Some(cam) = self.entities().find(|(_, e)| matches!(e.kind, Kind::Camera { mode, .. } if mode != 0) && !e.hidden).map(|(i, _)| i) else {
            return Ok(());
        };
        let eye = self.world_of(cam)?.position();
        for e in lod {
            let local = self.world_of(e)?.to_local_point(&eye);
            self.terrain_mut(e)?.choose_steps(local);
        }
        Ok(())
    }

    /// `TerrainY`: world height of the terrain at a world position.
    pub fn terrain_y(&mut self, e: i32, p: Vector3<f32>) -> Result<f32, String> {
        let w = self.world_of(e)?;
        let local = w.to_local_point(&p);
        let h = self.terrain(e)?.sample(local.x, local.z);
        let q = w.matrix.transform_point(&Vector3::new(local.x, h, local.z).into());
        Ok(q.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_and_sampling() {
        let mut t = TerrainState::new(2, 0);
        t.set_height(1, 1, 1.0).unwrap();
        assert_eq!(t.height(1, 1), 1.0);
        assert_eq!(t.sample(1.0, 1.0), 1.0);
        assert_eq!(t.sample(0.5, 0.5), 0.0); // on the diagonal between two zero corners
        assert!((t.sample(0.75, 0.75) - 0.5).abs() < 1e-6);
        assert_eq!(t.sample(0.0, 0.0), 0.0);
        assert!(t.set_height(3, 0, 0.5).is_err());
        // out-of-range samples clamp to the edge
        assert_eq!(t.sample(-5.0, 1.0), t.sample(0.0, 1.0));
    }

    #[test]
    fn grid_faces_up() {
        let t = TerrainState::new(3, 0);
        let (verts, tris) = t.geometry(Vector3::new(1.0, 1.0, 1.0));
        assert_eq!(verts.len(), 16);
        assert_eq!(tris.len(), 18);
        let mut v2 = verts.clone();
        crate::mesh::smooth_normals(&mut v2, &tris);
        for v in &v2 {
            assert!((v.normal - Vector3::y()).norm() < 1e-5);
        }
    }

    #[test]
    fn normals_account_for_scale() {
        let mut t = TerrainState::new(2, 0);
        t.set_height(2, 1, 1.0).unwrap();
        let (v, _) = t.geometry(Vector3::new(1.0, 10.0, 1.0));
        // after the scale is applied the world normal tilts away from the slope
        let n = v[4].normal; // vertex (1, 1)
        let world = Vector3::new(n.x, n.y * 10.0, n.z).normalize();
        assert!(world.x < -0.9, "{world:?}");
    }

    #[test]
    fn painted_layers_cover_the_ones_below() {
        let mut t = TerrainState::new(8, 0);
        assert_eq!(t.layer_weight(0, 4.0, 4.0), 1.0);
        t.paint(1, 4.0, 4.0, 3.0, 1.0);
        assert!((t.layer_weight(1, 4.0, 4.0) - 1.0).abs() < 1e-5);
        assert!(t.layer_weight(0, 4.0, 4.0) < 1e-5);
        // far from the brush the base shows
        assert_eq!(t.layer_weight(0, 0.0, 0.0), 1.0);
        // layer 2 painted over half: layer 1 shows the rest
        t.paint_vertex(2, 4, 4, 0.5);
        let (w1, w2) = (t.layer_weight(1, 4.0, 4.0), t.layer_weight(2, 4.0, 4.0));
        assert!((w1 - 0.5).abs() < 1e-5 && (w2 - 0.5).abs() < 1e-5, "{w1} {w2}");
        // painting layer 1 again uncovers it from layer 2
        t.paint_vertex(1, 4, 4, 0.5);
        assert_eq!(t.weights[t.idx(4, 4)][1], 0.0);
        let (v, _) = t.geometry(Vector3::new(1.0, 1.0, 1.0));
        assert_eq!(v[t.idx(4, 4)].color[0], 1.0);
    }

    #[test]
    fn level_of_detail_keeps_to_the_budget() {
        let mut t = TerrainState::new(128, 0);
        let full = 128 * 128 * 2;
        t.budget = 3000;
        t.choose_steps(Vector3::new(0.0, 0.5, 0.0));
        let tris: usize = t.steps.iter().map(|&s| 2 * (CHUNK / s as usize).pow(2)).sum();
        assert!(tris <= 3000 && tris < full, "{tris}");
        // the chunk under the camera is the most detailed
        assert_eq!(t.steps[0], *t.steps.iter().min().unwrap());
        assert!(t.steps.last().unwrap() > &t.steps[0]);
        let (v, tri) = t.geometry(Vector3::new(1.0, 1.0, 1.0));
        assert!(!v.is_empty() && tri.len() == tris);
        // collisions still use every square
        assert_eq!(t.full_triangles().len(), full);
    }

    #[test]
    fn finer_chunk_edges_follow_coarser_neighbours() {
        let mut t = TerrainState::new(32, 0);
        for z in 0..=32 {
            for x in 0..=32 {
                t.set_height(x, z, ((x * 7 + z * 13) % 11) as f32 / 10.0).unwrap();
            }
        }
        t.budget = 1;
        // chunk (0, 0) fine, its right neighbour coarse
        t.steps = vec![1, 4, 1, 1];
        let (v, _) = t.geometry(Vector3::new(1.0, 1.0, 1.0));
        // the fine chunk's right edge (x = 16) at z = 2 lies between the coarse points z = 0 and z = 4
        let p = v.iter().find(|p| p.pos.x == 16.0 && p.pos.z == 2.0).unwrap();
        let expect = (t.height(16, 0) + t.height(16, 4)) / 2.0;
        assert!((p.pos.y - expect).abs() < 1e-6, "{} {}", p.pos.y, expect);
        // inside the chunk heights are exact
        let q = v.iter().find(|p| p.pos.x == 5.0 && p.pos.z == 2.0).unwrap();
        assert_eq!(q.pos.y, t.height(5, 2));
    }
}
