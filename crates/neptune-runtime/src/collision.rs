//! Blitz3D-style collisions: moving ellipsoids swept against spheres,
//! boxes and triangle meshes, with stop / slide responses.
//!
//! The sweep uses the swept-sphere algorithm from Kasper Fauerby's
//! "Improved Collision detection and Response": everything is scaled into
//! the source's *ellipsoid space* (radii become 1), the unit sphere is
//! swept along its motion, the earliest contact is found, and the
//! remaining motion is projected onto the contact plane.
//!
//! This module is pure geometry; [`crate::commands`] connects it to
//! entities in `UpdateWorld`.

use fyrox::core::algebra::Vector3;

pub type V3 = Vector3<f32>;

/// Collision response modes (Blitz `Collisions` 4th parameter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// 1: stop at the contact point.
    Stop,
    /// 2: slide along the surface.
    Slide,
    /// 3: slide, but don't slide down slopes (for gravity).
    SlideNoSlopes,
}

impl Response {
    pub fn from_blitz(r: i32) -> Response {
        match r {
            1 => Response::Stop,
            3 => Response::SlideNoSlopes,
            _ => Response::Slide,
        }
    }
}

/// Something a moving ellipsoid can hit, in world space.
#[derive(Clone, Debug)]
pub enum Shape {
    Sphere { center: V3, radius: f32 },
    Triangles(Vec<[V3; 3]>),
}

#[derive(Clone, Debug)]
pub struct Obstacle {
    pub entity: i32,
    pub response: Response,
    pub shape: Shape,
}

/// One contact, in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub entity: i32,
    pub point: V3,
    pub normal: V3,
    /// Fraction of the motion step at which the contact happened.
    pub time: f32,
    /// Index of the triangle hit (polygon and box collisions), or -1.
    pub triangle: i32,
}

/// Triangles of an axis-aligned box `(x, y, z, width, height, depth)`, transformed by `f`.
pub fn box_triangles(b: [f32; 6], f: impl Fn(V3) -> V3) -> Vec<[V3; 3]> {
    let (lo, hi) = (V3::new(b[0], b[1], b[2]), V3::new(b[0] + b[3], b[1] + b[4], b[2] + b[5]));
    let c = |i: usize| f(V3::new(if i & 1 != 0 { hi.x } else { lo.x }, if i & 2 != 0 { hi.y } else { lo.y }, if i & 4 != 0 { hi.z } else { lo.z }));
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [[c(q[0]), c(q[1]), c(q[2])], [c(q[0]), c(q[2]), c(q[3])]]).collect()
}

/// Smallest root of `a x² + b x + c = 0` in `[0, max]`.
fn lowest_root(a: f32, b: f32, c: f32, max: f32) -> Option<f32> {
    if a.abs() < 1e-12 {
        if b.abs() < 1e-12 {
            return None;
        }
        let x = -c / b;
        return (0.0..=max).contains(&x).then_some(x);
    }
    let det = b * b - 4.0 * a * c;
    if det < 0.0 {
        return None;
    }
    let s = det.sqrt();
    let (mut r1, mut r2) = ((-b - s) / (2.0 * a), (-b + s) / (2.0 * a));
    if r1 > r2 {
        std::mem::swap(&mut r1, &mut r2);
    }
    if (0.0..=max).contains(&r1) {
        Some(r1)
    } else if (0.0..=max).contains(&r2) {
        Some(r2)
    } else {
        None
    }
}

fn point_in_triangle(p: &V3, a: &V3, b: &V3, c: &V3) -> bool {
    // barycentric test
    let (v0, v1, v2) = (c - a, b - a, p - a);
    let (d00, d01, d02, d11, d12) = (v0.dot(&v0), v0.dot(&v1), v0.dot(&v2), v1.dot(&v1), v1.dot(&v2));
    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < 1e-12 {
        return false;
    }
    let u = (d11 * d02 - d01 * d12) / denom;
    let v = (d00 * d12 - d01 * d02) / denom;
    u >= -1e-5 && v >= -1e-5 && u + v <= 1.0 + 1e-5
}

/// Sweeps a unit sphere from `p` along `v` against triangle `t`.
/// Returns the earliest contact time in `[0, max_t]` and the contact point.
pub fn sweep_triangle(p: &V3, v: &V3, t: &[V3; 3], max_t: f32) -> Option<(f32, V3)> {
    let [a, b, c] = t;
    let mut n = (b - a).cross(&(c - a));
    let len = n.norm();
    if len < 1e-12 {
        return None;
    }
    n /= len;
    // two-sided: use the face the sphere starts on
    let mut dist = n.dot(&(p - a));
    if dist < 0.0 {
        n = -n;
        dist = -dist;
    }
    let ndv = n.dot(v);
    if ndv >= 0.0 && dist >= 1.0 {
        return None; // moving away (or parallel) and not touching
    }
    let (t0, embedded) = if ndv.abs() < 1e-9 {
        if dist >= 1.0 {
            return None;
        }
        (0.0, true)
    } else {
        let t0 = ((1.0 - dist) / ndv).max(0.0);
        let t1 = (-1.0 - dist) / ndv;
        if t0 > max_t || t1.max(t0) < 0.0 {
            return None;
        }
        (t0, dist < 1.0)
    };
    if !embedded {
        let plane_point = p - n + v * t0;
        if point_in_triangle(&plane_point, a, b, c) {
            return Some((t0, plane_point));
        }
    }
    // vertices and edges
    let mut best: Option<(f32, V3)> = None;
    let mut limit = max_t;
    let vv = v.norm_squared();
    for q in [a, b, c] {
        if let Some(x) = lowest_root(vv, 2.0 * v.dot(&(p - q)), (q - p).norm_squared() - 1.0, limit) {
            limit = x;
            best = Some((x, *q));
        }
    }
    for (p1, p2) in [(a, b), (b, c), (c, a)] {
        let edge = p2 - p1;
        let btv = p1 - p;
        let (el2, edv, edbtv) = (edge.norm_squared(), edge.dot(v), edge.dot(&btv));
        let qa = el2 * -vv + edv * edv;
        let qb = el2 * (2.0 * v.dot(&btv)) - 2.0 * edv * edbtv;
        let qc = el2 * (1.0 - btv.norm_squared()) + edbtv * edbtv;
        if let Some(x) = lowest_root(qa, qb, qc, limit) {
            let f = (edv * x - edbtv) / el2;
            if (0.0..=1.0).contains(&f) {
                limit = x;
                best = Some((x, p1 + edge * f));
            }
        }
    }
    if best.is_none() && embedded {
        // already overlapping the face: contact at the start
        let plane_point = p - n * dist;
        if point_in_triangle(&plane_point, a, b, c) {
            return Some((0.0, plane_point));
        }
    }
    best
}

/// Sweeps a unit sphere from `p` along `v` against a sphere.
pub fn sweep_sphere(p: &V3, v: &V3, center: &V3, radius: f32, max_t: f32) -> Option<(f32, V3)> {
    let r = 1.0 + radius;
    let d = p - center;
    let x = lowest_root(v.norm_squared(), 2.0 * v.dot(&d), d.norm_squared() - r * r, max_t)?;
    let at = p + v * x;
    Some((x, center + (at - center) * (radius / r)))
}

/// Moves an ellipsoid with radii `(rx, ry, rx)` from `start` to `end`,
/// returning the final position and the contacts made on the way.
pub fn collide_and_slide(start: V3, end: V3, radii: V3, obstacles: &[Obstacle]) -> (V3, Vec<Contact>) {
    const VERY_CLOSE: f32 = 0.001;
    let to_e = |w: &V3| w.component_div(&radii);
    let to_w = |e: &V3| e.component_mul(&radii);
    let shapes: Vec<(usize, Shape)> = obstacles
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let s = match &o.shape {
                Shape::Sphere { center, radius } => Shape::Sphere { center: to_e(center), radius: radius / radii.x },
                Shape::Triangles(t) => Shape::Triangles(t.iter().map(|t| [to_e(&t[0]), to_e(&t[1]), to_e(&t[2])]).collect()),
            };
            (i, s)
        })
        .collect();

    let mut pos = to_e(&start);
    let mut dest = to_e(&end);
    let mut contacts = Vec::new();
    for _ in 0..8 {
        let vel = dest - pos;
        if vel.norm_squared() < 1e-12 {
            break;
        }
        let mut hit: Option<(f32, V3, usize, i32)> = None;
        for (i, s) in &shapes {
            let limit = hit.map_or(1.0, |h| h.0);
            let r = match s {
                Shape::Sphere { center, radius } => sweep_sphere(&pos, &vel, center, *radius, limit).map(|(t, p)| (t, p, -1)),
                Shape::Triangles(tris) => {
                    let mut best: Option<(f32, V3, i32)> = None;
                    for (ti, t) in tris.iter().enumerate() {
                        if let Some((ht, hp)) = sweep_triangle(&pos, &vel, t, best.map_or(limit, |b| b.0)) {
                            best = Some((ht, hp, ti as i32));
                        }
                    }
                    best
                }
            };
            if let Some((t, p, tri)) = r {
                if hit.is_none_or(|h| t < h.0) {
                    hit = Some((t, p, *i, tri));
                }
            }
        }
        let Some((t, point, idx, tri)) = hit else {
            pos = dest;
            break;
        };
        // move up to (just short of) the contact
        let dir = vel.normalize();
        let travel = vel.norm() * t;
        let new_pos = if travel >= VERY_CLOSE { pos + dir * (travel - VERY_CLOSE) } else { pos };
        let mut normal = new_pos - point;
        if normal.norm_squared() < 1e-12 {
            normal = -dir;
        }
        let normal = normal.normalize();
        let ob = &obstacles[idx];
        let world_normal = V3::new(normal.x / radii.x, normal.y / radii.y, normal.z / radii.z).normalize();
        contacts.push(Contact { entity: ob.entity, point: to_w(&point), normal: world_normal, time: t, triangle: tri });
        pos = new_pos;
        // remaining motion projected onto the contact plane
        let remaining = dest - pos;
        let slide = |m: V3| m - normal * m.dot(&normal);
        dest = match ob.response {
            Response::Stop => pos,
            Response::Slide => pos + slide(remaining),
            Response::SlideNoSlopes => {
                if normal.y > 0.0 {
                    // standing on a floor or slope: gravity doesn't drag us downhill
                    let horizontal = V3::new(remaining.x, 0.0, remaining.z);
                    pos + slide(horizontal)
                } else {
                    pos + slide(remaining)
                }
            }
        };
        if ob.response == Response::Stop {
            break;
        }
    }
    (to_w(&pos), contacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> Obstacle {
        // large floor at y = 0
        let s = 100.0;
        Obstacle {
            entity: 7,
            response: Response::Slide,
            shape: Shape::Triangles(vec![
                [V3::new(-s, 0.0, -s), V3::new(s, 0.0, -s), V3::new(s, 0.0, s)],
                [V3::new(-s, 0.0, -s), V3::new(s, 0.0, s), V3::new(-s, 0.0, s)],
            ]),
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn falling_sphere_lands_on_floor() {
        let (p, hits) = collide_and_slide(V3::new(0.0, 3.0, 0.0), V3::new(0.0, -5.0, 0.0), V3::new(1.0, 1.0, 1.0), &[floor()]);
        assert!(close(p.y, 1.0), "{p:?}");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entity, 7);
        assert!(close(hits[0].normal.y, 1.0));
        assert!(close(hits[0].point.y, 0.0));
    }

    #[test]
    fn diagonal_motion_slides_along_floor() {
        let (p, _) = collide_and_slide(V3::new(0.0, 1.5, 0.0), V3::new(4.0, -1.5, 0.0), V3::new(1.0, 1.0, 1.0), &[floor()]);
        assert!(close(p.y, 1.0), "{p:?}");
        assert!(p.x > 3.5, "slid only to {p:?}");
    }

    #[test]
    fn stop_response_does_not_slide() {
        let mut f = floor();
        f.response = Response::Stop;
        let (p, _) = collide_and_slide(V3::new(0.0, 1.5, 0.0), V3::new(4.0, -1.5, 0.0), V3::new(1.0, 1.0, 1.0), &[f]);
        assert!(close(p.y, 1.0) && p.x < 2.0, "{p:?}");
    }

    #[test]
    fn ellipsoid_radius_is_respected() {
        let (p, _) = collide_and_slide(V3::new(0.0, 5.0, 0.0), V3::new(0.0, -5.0, 0.0), V3::new(0.5, 2.0, 0.5), &[floor()]);
        assert!(close(p.y, 2.0), "{p:?}");
    }

    #[test]
    fn spheres_collide() {
        let ob = Obstacle { entity: 3, response: Response::Stop, shape: Shape::Sphere { center: V3::new(5.0, 0.0, 0.0), radius: 1.0 } };
        let (p, hits) = collide_and_slide(V3::zeros(), V3::new(10.0, 0.0, 0.0), V3::new(1.0, 1.0, 1.0), &[ob]);
        assert!(close(p.x, 3.0), "{p:?}");
        assert!(close(hits[0].point.x, 4.0) && close(hits[0].normal.x, -1.0));
    }

    #[test]
    fn box_walls_block_and_edges_are_hit() {
        let wall = Obstacle {
            entity: 1,
            response: Response::Slide,
            shape: Shape::Triangles(box_triangles([2.0, -1.0, -1.0, 1.0, 2.0, 2.0], |p| p)),
        };
        let (p, _) = collide_and_slide(V3::zeros(), V3::new(5.0, 0.0, 0.0), V3::new(0.5, 0.5, 0.5), &[wall.clone()]);
        assert!(close(p.x, 1.5), "{p:?}");
        // grazing past the box corner pushes the sphere around it
        let (p, hits) = collide_and_slide(V3::new(0.0, 0.0, -1.4), V3::new(5.0, 0.0, -1.4), V3::new(0.5, 0.5, 0.5), &[wall]);
        assert!(!hits.is_empty());
        assert!(p.x > 3.0, "{p:?}"); // past the box
    }

    #[test]
    fn slide_no_slopes_keeps_position_on_slope() {
        // 30° slope rising along +X
        let k = 0.577;
        let slope = Obstacle {
            entity: 1,
            response: Response::SlideNoSlopes,
            shape: Shape::Triangles(vec![
                [V3::new(-50.0, -50.0 * k, -50.0), V3::new(50.0, 50.0 * k, 50.0), V3::new(50.0, 50.0 * k, -50.0)],
                [V3::new(-50.0, -50.0 * k, -50.0), V3::new(-50.0, -50.0 * k, 50.0), V3::new(50.0, 50.0 * k, 50.0)],
            ]),
        };
        // rest on the slope, then apply "gravity" a few times
        let mut p = V3::new(0.0, 3.0, 0.0);
        for _ in 0..10 {
            p = collide_and_slide(p, p - V3::new(0.0, 0.5, 0.0), V3::new(1.0, 1.0, 1.0), &[slope.clone()]).0;
        }
        assert!(p.x.abs() < 0.05, "slid downhill to {p:?}");
        let mut s = slope.clone();
        s.response = Response::Slide;
        let mut q = V3::new(0.0, 3.0, 0.0);
        for _ in 0..10 {
            q = collide_and_slide(q, q - V3::new(0.0, 0.5, 0.0), V3::new(1.0, 1.0, 1.0), &[s.clone()]).0;
        }
        assert!(q.x < -0.5, "plain slide should go downhill: {q:?}");
    }
}
