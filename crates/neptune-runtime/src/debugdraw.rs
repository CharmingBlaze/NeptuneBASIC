//! Debug drawing (ext): lines and shapes in the 3D world for finding
//! problems — `DebugLine`, `DebugBox`, `DebugSphere`, `DebugEntity`,
//! `DebugAxes`, `DebugNavMesh`, and every frame `DebugPhysics` and
//! `DebugCollisions`.
//!
//! Shapes are drawn by the next `RenderWorld` and then forgotten, so a
//! program draws them again every frame for as long as it wants them (like
//! `Text`). They are thin lines of one colour, unaffected by light and fog,
//! hidden behind nearer surfaces. Coordinates are world (Blitz) space.

use fyrox::core::{
    algebra::{Matrix4, Vector3},
    color::Color,
    math::aabb::AxisAlignedBoundingBox,
};
use fyrox::scene::debug::Line;

use crate::world::Host3D;

type V3 = Vector3<f32>;

/// Lines waiting for the next `RenderWorld` and the every-frame switches.
#[derive(Debug, Default)]
pub struct DebugDraw {
    /// In Fyrox space.
    lines: Vec<Line>,
    pub physics: bool,
    pub collisions: bool,
}

/// Blitz space to Fyrox space.
fn fy(v: V3) -> V3 {
    V3::new(-v.x, v.y, v.z)
}

pub fn color(r: f32, g: f32, b: f32) -> Color {
    let c = |v: f32| v.clamp(0.0, 255.0) as u8;
    Color::opaque(c(r), c(g), c(b))
}

impl DebugDraw {
    /// A line between two Blitz-space points.
    pub fn line(&mut self, a: V3, b: V3, color: Color) {
        self.lines.push(Line { begin: fy(a), end: fy(b), color });
    }

    /// The twelve edges of a box given by its eight corners (Blitz space),
    /// ordered by (x, y, z) bits.
    fn box_edges(&mut self, c: [V3; 8], color: Color) {
        for (i, j) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
            self.line(c[i], c[j], color);
        }
    }

    /// An axis-aligned box: centre and full size.
    pub fn aabb(&mut self, centre: V3, size: V3, color: Color) {
        let h = size * 0.5;
        let c = std::array::from_fn(|k| centre + V3::new(if k & 1 != 0 { h.x } else { -h.x }, if k & 2 != 0 { h.y } else { -h.y }, if k & 4 != 0 { h.z } else { -h.z }));
        self.box_edges(c, color);
    }

    /// An ellipsoid as three rings: radius `rx` across, `ry` up.
    pub fn ellipsoid(&mut self, centre: V3, rx: f32, ry: f32, color: Color) {
        const N: usize = 24;
        let ring = |k: usize, plane: usize| {
            let a = k as f32 / N as f32 * std::f32::consts::TAU;
            let (s, c) = a.sin_cos();
            centre
                + match plane {
                    0 => V3::new(c * rx, s * ry, 0.0),
                    1 => V3::new(0.0, s * ry, c * rx),
                    _ => V3::new(c * rx, 0.0, s * rx),
                }
        };
        for plane in 0..3 {
            for k in 0..N {
                let (a, b) = (ring(k, plane), ring(k + 1, plane));
                self.line(a, b, color);
            }
        }
    }
}

impl Host3D<'_> {
    /// `DebugEntity`: the entity's bounding box, turned and scaled with it.
    pub fn debug_entity(&mut self, e: i32, color: Color) -> Result<(), String> {
        let node = self.ent(e)?.node;
        let graph = self.graph()?;
        let n = &graph[node];
        // a mesh's box from its geometry; anything else a unit box
        let (aabb, m): (AxisAlignedBoundingBox, Matrix4<f32>) = (n.local_bounding_box(), n.global_transform());
        let corners: [V3; 8] = std::array::from_fn(|k| {
            let p = V3::new(
                if k & 1 != 0 { aabb.max.x } else { aabb.min.x },
                if k & 2 != 0 { aabb.max.y } else { aabb.min.y },
                if k & 4 != 0 { aabb.max.z } else { aabb.min.z },
            );
            // Fyrox space back to Blitz space (the lines flip it again)
            let w = m.transform_point(&p.into()).coords;
            V3::new(-w.x, w.y, w.z)
        });
        self.world.debug.box_edges(corners, color);
        Ok(())
    }

    /// `DebugAxes`: the entity's own X (red), Y (green) and Z (blue) axes.
    pub fn debug_axes(&mut self, e: i32, size: f32) -> Result<(), String> {
        let w = self.world_of(e)?;
        let p = w.position();
        let m = w.matrix;
        for (k, c) in [(0, Color::RED), (1, Color::GREEN), (2, Color::BLUE)] {
            let axis = V3::new(m[(0, k)], m[(1, k)], m[(2, k)]).try_normalize(1e-6).unwrap_or_else(V3::zeros);
            self.world.debug.line(p, p + axis * size, c);
        }
        Ok(())
    }

    /// `DebugNavMesh`: the edges of a navigation mesh's triangles.
    pub fn debug_navmesh(&mut self, n: i32, color: Color) -> Result<(), String> {
        let lift = V3::new(0.0, 0.05, 0.0);
        let tris: Vec<[V3; 3]> = self.navmesh(n)?.index.triangles().collect();
        for [a, b, c] in tris {
            self.world.debug.line(a + lift, b + lift, color);
            self.world.debug.line(b + lift, c + lift, color);
            self.world.debug.line(c + lift, a + lift, color);
        }
        let links: Vec<(V3, V3)> = self.navmesh(n)?.links.iter().flatten().filter(|l| l.enabled).map(|l| (l.a, l.b)).collect();
        for (a, b) in links {
            self.world.debug.line(a + lift, b + lift, color);
        }
        Ok(())
    }

    /// Collision spheres, ellipsoids and boxes (`EntityType`, `EntityRadius`,
    /// `EntityBox`) of every entity that has a collision type.
    fn draw_collision_shapes(&mut self) -> Result<(), String> {
        let ids: Vec<i32> = self.entities().filter(|(_, e)| e.col_type != 0 && !e.hidden).map(|(i, _)| i).collect();
        let yellow = Color::opaque(255, 220, 0);
        let cyan = Color::opaque(0, 220, 255);
        for id in ids {
            let w = self.world_of(id)?;
            let (rx, ry, bx) = {
                let e = self.ent(id)?;
                (e.radius.0, e.radius.1, e.col_box)
            };
            if let Some([x, y, z, bw, bh, bd]) = bx {
                let corners: [V3; 8] = std::array::from_fn(|k| {
                    let p = V3::new(x + if k & 1 != 0 { bw } else { 0.0 }, y + if k & 2 != 0 { bh } else { 0.0 }, z + if k & 4 != 0 { bd } else { 0.0 });
                    w.matrix.transform_point(&p.into()).coords
                });
                self.world.debug.box_edges(corners, cyan);
            } else {
                self.world.debug.ellipsoid(w.position(), rx.max(0.01), ry.max(0.01), yellow);
            }
        }
        Ok(())
    }

    /// Hands this frame's debug lines to the scene (called by `RenderWorld`).
    pub fn flush_debug_draw(&mut self) -> Result<(), String> {
        if self.world.debug.collisions {
            self.draw_collision_shapes()?;
        }
        let lines = std::mem::take(&mut self.world.debug.lines);
        let physics = self.world.debug.physics;
        let scene = self.scene()?;
        scene.drawing_context.clear_lines();
        if physics {
            scene.graph.physics.draw(&mut scene.drawing_context);
            scene.graph.physics2d.draw(&mut scene.drawing_context);
        }
        for l in lines {
            scene.drawing_context.add_line(l);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_make_lines() {
        let mut d = DebugDraw::default();
        d.line(V3::new(1.0, 2.0, 3.0), V3::zeros(), Color::WHITE);
        // Blitz +X is Fyrox -X
        assert_eq!(d.lines[0].begin, V3::new(-1.0, 2.0, 3.0));
        d.aabb(V3::zeros(), V3::repeat(2.0), Color::WHITE);
        assert_eq!(d.lines.len(), 1 + 12);
        d.ellipsoid(V3::zeros(), 1.0, 2.0, Color::WHITE);
        assert_eq!(d.lines.len(), 13 + 72);
        // every ring point is on the ellipsoid
        for l in &d.lines[13..] {
            let p = V3::new(-l.begin.x, l.begin.y, l.begin.z);
            let r = (p.x * p.x + p.z * p.z) + p.y * p.y / 4.0;
            assert!((r - 1.0).abs() < 1e-4, "{p}");
        }
    }
}
