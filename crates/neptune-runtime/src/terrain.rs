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
}

impl TerrainState {
    pub fn new(size: usize, surface: i32) -> Self {
        TerrainState { size, heights: vec![0.0; (size + 1) * (size + 1)], surface, built_for: None }
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

    /// Grid geometry in local Blitz space, normals adjusted for `scale`.
    pub fn geometry(&self, scale: Vector3<f32>) -> (Vec<Vert>, Vec<[u32; 3]>) {
        let n = self.size;
        let s = scale.map(|c| if c.abs() < 1e-6 { 1e-6 } else { c });
        let mut verts = Vec::with_capacity((n + 1) * (n + 1));
        for z in 0..=n {
            for x in 0..=n {
                let (xi, zi) = (x as i32, z as i32);
                // world-space slope, then mapped back through the scale so the
                // shader's (non inverse-transposed) normal transform restores it
                let dx = (self.height(xi + 1, zi) - self.height(xi - 1, zi)) * s.y / (2.0 * s.x);
                let dz = (self.height(xi, zi + 1) - self.height(xi, zi - 1)) * s.y / (2.0 * s.z);
                let world_n = Vector3::new(-dx, 1.0, -dz);
                let local_n = Vector3::new(world_n.x / s.x, world_n.y / s.y, world_n.z / s.z).normalize();
                let uv = [x as f32, (n - z) as f32, 0.0];
                verts.push(Vert {
                    pos: Vector3::new(x as f32, self.height(xi, zi), z as f32),
                    normal: local_n,
                    uv: [uv, uv],
                    ..Default::default()
                });
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
}
