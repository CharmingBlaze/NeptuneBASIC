//! Geometry for `CreateCube`, `CreateSphere`, `CreateCylinder`, `CreateCone`
//! and `CreatePlane`, built in Blitz space with Blitz3D's texture layout and
//! uploaded through [`crate::mesh::surface_data`] like any custom mesh.
//!
//! Every face is seen the right way round from outside: on the side facing
//! a default camera (-Z), U grows to the right and V grows downwards. Each
//! triangle is wound clockwise as seen from outside (Blitz front faces);
//! [`Builder::tri`] checks that against the outward direction so no face can
//! end up inside out.

use std::f32::consts::{PI, TAU};

use fyrox::core::algebra::Vector3;

use crate::mesh::Vert;

#[derive(Default)]
pub struct Builder {
    pub verts: Vec<Vert>,
    pub tris: Vec<[u32; 3]>,
}

impl Builder {
    pub fn vert(&mut self, pos: Vector3<f32>, normal: Vector3<f32>, u: f32, v: f32) -> u32 {
        self.verts.push(Vert { pos, normal, uv: [[u, v, 0.0], [u, v, 0.0]], ..Default::default() });
        self.verts.len() as u32 - 1
    }

    /// Adds a triangle that faces `outward`, fixing the corner order if needed.
    pub fn tri(&mut self, a: u32, b: u32, c: u32, outward: Vector3<f32>) {
        let p = |i: u32| self.verts[i as usize].pos;
        let n = (p(b) - p(a)).cross(&(p(c) - p(a)));
        if n.norm_squared() < 1e-12 {
            return; // degenerate (sphere poles)
        }
        // Blitz front faces: (b-a)x(c-a) points outward
        if n.dot(&outward) >= 0.0 {
            self.tris.push([a, b, c]);
        } else {
            self.tris.push([a, c, b]);
        }
    }

    /// A quad given clockwise from its top-left corner as seen from outside.
    pub fn quad(&mut self, corners: [Vector3<f32>; 4], normal: Vector3<f32>) {
        let uv = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let i: Vec<u32> = corners.iter().zip(uv).map(|(p, (u, v))| self.vert(*p, normal, u, v)).collect();
        self.tri(i[0], i[1], i[2], normal);
        self.tri(i[0], i[2], i[3], normal);
    }
}

fn v(x: f32, y: f32, z: f32) -> Vector3<f32> {
    Vector3::new(x, y, z)
}

/// A point on a ring of radius `r` at angle `t` (radians). `t = PI` is the
/// front (-Z) and X grows with `t` there, so U = t / TAU reads left to right.
fn ring(r: f32, t: f32, y: f32) -> Vector3<f32> {
    v(-r * t.sin(), y, r * t.cos())
}

pub fn cube() -> Builder {
    let mut b = Builder::default();
    let faces = [
        // (top-left, top-right, bottom-right, bottom-left) seen from outside
        ([v(-1., 1., -1.), v(1., 1., -1.), v(1., -1., -1.), v(-1., -1., -1.)], v(0., 0., -1.)), // front
        ([v(1., 1., -1.), v(1., 1., 1.), v(1., -1., 1.), v(1., -1., -1.)], v(1., 0., 0.)),     // right
        ([v(1., 1., 1.), v(-1., 1., 1.), v(-1., -1., 1.), v(1., -1., 1.)], v(0., 0., 1.)),     // back
        ([v(-1., 1., 1.), v(-1., 1., -1.), v(-1., -1., -1.), v(-1., -1., 1.)], v(-1., 0., 0.)), // left
        ([v(-1., 1., 1.), v(1., 1., 1.), v(1., 1., -1.), v(-1., 1., -1.)], v(0., 1., 0.)),     // top
        ([v(-1., -1., -1.), v(1., -1., -1.), v(1., -1., 1.), v(-1., -1., 1.)], v(0., -1., 0.)), // bottom
    ];
    for (corners, n) in faces {
        b.quad(corners, n);
    }
    b
}

pub fn sphere(segments: usize) -> Builder {
    let (rings, around) = (segments.max(2), segments.max(2) * 2);
    let mut b = Builder::default();
    let w = around as u32 + 1;
    for i in 0..=rings {
        let phi = i as f32 / rings as f32 * PI;
        for j in 0..=around {
            let u = j as f32 / around as f32;
            let p = ring(phi.sin(), u * TAU, phi.cos());
            b.vert(p, p.normalize(), u, i as f32 / rings as f32);
        }
    }
    for i in 0..rings as u32 {
        for j in 0..around as u32 {
            let (a, c) = (i * w + j, (i + 1) * w + j);
            let out = b.verts[a as usize].pos + b.verts[c as usize + 1].pos;
            b.tri(a, a + 1, c + 1, out);
            b.tri(a, c + 1, c, out);
        }
    }
    b
}

/// Cylinder (`cone == false`) or cone, height -1..1 and radius 1.
fn round(segments: usize, solid: bool, cone: bool) -> Builder {
    let n = segments.max(3);
    let mut b = Builder::default();
    for j in 0..n as u32 {
        let (u0, u1) = (j as f32 / n as f32, (j + 1) as f32 / n as f32);
        let (b0, b1) = (ring(1.0, u0 * TAU, -1.0), ring(1.0, u1 * TAU, -1.0));
        // side normals: radial for a cylinder, tilted up for a cone (height 2, radius 1)
        let side_n = |p: Vector3<f32>| if cone { v(p.x * 2.0, 1.0, p.z * 2.0).normalize() } else { v(p.x, 0.0, p.z) };
        let out = v(b0.x + b1.x, 0.0, b0.z + b1.z);
        if cone {
            let tip = b.vert(v(0., 1., 0.), side_n(ring(1.0, (u0 + u1) * PI, 0.0)), (u0 + u1) / 2.0, 0.0);
            let i0 = b.vert(b0, side_n(b0), u0, 1.0);
            let i1 = b.vert(b1, side_n(b1), u1, 1.0);
            b.tri(tip, i1, i0, out);
        } else {
            let t0 = b.vert(v(b0.x, 1., b0.z), side_n(b0), u0, 0.0);
            let t1 = b.vert(v(b1.x, 1., b1.z), side_n(b1), u1, 0.0);
            let i0 = b.vert(b0, side_n(b0), u0, 1.0);
            let i1 = b.vert(b1, side_n(b1), u1, 1.0);
            b.tri(t0, t1, i1, out);
            b.tri(t0, i1, i0, out);
        }
    }
    if solid {
        // caps, mapped flat: top seen from above (+Z up), bottom seen from below (-Z up)
        let caps: &[(f32, f32)] = if cone { &[(-1.0, -1.0)] } else { &[(1.0, 1.0), (-1.0, -1.0)] };
        for &(y, dir) in caps {
            let normal = v(0., dir, 0.);
            let uv = |p: Vector3<f32>| ((p.x + 1.0) / 2.0, (1.0 - dir * p.z) / 2.0);
            let c = b.vert(v(0., y, 0.), normal, 0.5, 0.5);
            for j in 0..n as u32 {
                let p0 = ring(1.0, j as f32 / n as f32 * TAU, y);
                let p1 = ring(1.0, (j + 1) as f32 / n as f32 * TAU, y);
                let (u0, v0) = uv(p0);
                let (u1, v1) = uv(p1);
                let i0 = b.vert(p0, normal, u0, v0);
                let i1 = b.vert(p1, normal, u1, v1);
                b.tri(c, i0, i1, normal);
            }
        }
    }
    b
}

pub fn cylinder(segments: usize, solid: bool) -> Builder {
    round(segments, solid, false)
}

pub fn cone(segments: usize, solid: bool) -> Builder {
    round(segments, solid, true)
}

/// Blitz planes are infinite; this is a large quad facing +Y. Texture
/// coordinates are in world units so a texture repeats once per unit.
pub fn plane(size: f32) -> Builder {
    let h = size / 2.0;
    let mut b = Builder::default();
    let n = v(0., 1., 0.);
    let c = [v(-h, 0., h), v(h, 0., h), v(h, 0., -h), v(-h, 0., -h)];
    let i: Vec<u32> = c.iter().map(|p| b.vert(*p, n, p.x, -p.z)).collect();
    b.tri(i[0], i[1], i[2], n);
    b.tri(i[0], i[2], i[3], n);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every triangle faces away from the shape's centre line.
    fn outward(b: &Builder, axis_only: bool) {
        for t in &b.tris {
            let [a, bb, c] = t.map(|i| b.verts[i as usize].pos);
            let n = (bb - a).cross(&(c - a));
            let mut centre = (a + bb + c) / 3.0;
            if axis_only && centre.y.abs() < 0.999 {
                centre.y = 0.0; // sides of round shapes: measure from the axis
            }
            assert!(n.dot(&centre) > 0.0, "triangle {t:?} faces inward");
        }
    }

    #[test]
    fn shapes_face_outward() {
        outward(&cube(), false);
        outward(&sphere(8), false);
        outward(&cylinder(8, true), true);
        outward(&cone(8, true), false);
        assert_eq!(cube().tris.len(), 12);
        assert_eq!(cylinder(8, false).tris.len(), 16);
    }

    #[test]
    fn textures_read_left_to_right_on_the_front() {
        // the front face of the cube: top-left vertex has UV 0,0 and is at -X,+Y
        let c = cube();
        let tl = c.verts.iter().find(|v| v.normal.z < -0.5 && v.uv[0][0] == 0.0 && v.uv[0][1] == 0.0).unwrap();
        assert_eq!((tl.pos.x, tl.pos.y), (-1.0, 1.0));
        // round shapes: at the front, U grows towards +X
        let p0 = ring(1.0, PI - 0.1, 0.0);
        let p1 = ring(1.0, PI + 0.1, 0.0);
        assert!(p0.z < -0.9 && p1.x > p0.x);
        // the sphere's top row has V = 0
        let s = sphere(8);
        assert!(s.verts.iter().filter(|v| v.pos.y > 0.99).all(|v| v.uv[0][1] == 0.0));
    }
}
