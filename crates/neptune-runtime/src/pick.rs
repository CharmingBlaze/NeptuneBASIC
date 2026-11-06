//! Ray and swept-sphere picking (`LinePick`, `CameraPick`, `EntityPick`, ...).

use crate::collision::{sweep_sphere, sweep_triangle, Shape, V3};

/// Result of a pick, in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickHit {
    pub entity: i32,
    pub point: V3,
    pub normal: V3,
    /// Fraction of the pick line at the hit (0..1).
    pub time: f32,
    /// Index of the triangle hit (polygon picks), or -1.
    pub triangle: i32,
}

/// Möller–Trumbore ray/triangle intersection; returns `t` along `dir`.
fn ray_triangle(o: &V3, dir: &V3, t: &[V3; 3]) -> Option<f32> {
    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
    let p = dir.cross(&e2);
    let det = e1.dot(&p);
    if det.abs() < 1e-10 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t[0];
    let u = s.dot(&p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(&e1);
    let v = dir.dot(&q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let x = e2.dot(&q) * inv;
    (x >= 0.0).then_some(x)
}

fn ray_sphere(o: &V3, dir: &V3, c: &V3, r: f32) -> Option<f32> {
    let m = o - c;
    let (a, b, cc) = (dir.norm_squared(), m.dot(dir), m.norm_squared() - r * r);
    if a < 1e-12 {
        return None;
    }
    let disc = b * b - a * cc;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    if t >= 0.0 {
        Some(t)
    } else if cc <= 0.0 {
        Some(0.0) // starts inside
    } else {
        None
    }
}

/// Finds the nearest hit along the segment `origin .. origin + line`.
/// With `radius > 0` a sphere of that radius is swept instead of a ray.
pub fn pick(origin: V3, line: V3, radius: f32, targets: &[(i32, Shape)]) -> Option<PickHit> {
    let mut best: Option<PickHit> = None;
    let limit = |b: &Option<PickHit>| b.map_or(1.0, |h| h.time);
    for (entity, shape) in targets {
        match shape {
            Shape::Sphere { center, radius: r } => {
                let hit = if radius > 0.0 {
                    let k = 1.0 / radius;
                    sweep_sphere(&(origin * k), &(line * k), &(center * k), r * k, limit(&best)).map(|(t, p)| (t, p / k))
                } else {
                    ray_sphere(&origin, &line, center, *r).filter(|t| *t <= limit(&best)).map(|t| (t, origin + line * t))
                };
                if let Some((time, point)) = hit {
                    let normal = (point - center).try_normalize(1e-9).unwrap_or(-line.normalize());
                    best = Some(PickHit { entity: *entity, point, normal, time, triangle: -1 });
                }
            }
            Shape::Triangles(tris) => {
                for (i, t) in tris.iter().enumerate() {
                    let hit = if radius > 0.0 {
                        let k = 1.0 / radius;
                        let ts = [t[0] * k, t[1] * k, t[2] * k];
                        sweep_triangle(&(origin * k), &(line * k), &ts, limit(&best)).map(|(t, p)| (t, p / k))
                    } else {
                        ray_triangle(&origin, &line, t).filter(|x| *x <= limit(&best)).map(|x| (x, origin + line * x))
                    };
                    if let Some((time, point)) = hit {
                        let mut n = (t[1] - t[0]).cross(&(t[2] - t[0])).try_normalize(1e-12).unwrap_or(V3::y());
                        if n.dot(&line) > 0.0 {
                            n = -n; // face the picker
                        }
                        best = Some(PickHit { entity: *entity, point, normal: n, time, triangle: i as i32 });
                    }
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::box_triangles;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn ray_hits_nearest_target() {
        let targets = vec![
            (1, Shape::Triangles(box_triangles([4.0, -1.0, -1.0, 2.0, 2.0, 2.0], |p| p))),
            (2, Shape::Sphere { center: V3::new(2.0, 0.0, 0.0), radius: 0.5 }),
        ];
        let h = pick(V3::zeros(), V3::new(10.0, 0.0, 0.0), 0.0, &targets).unwrap();
        assert_eq!(h.entity, 2);
        assert!(close(h.point.x, 1.5) && close(h.time, 0.15) && close(h.normal.x, -1.0));
        // aim above the sphere: hits the box face at x = 4
        let h = pick(V3::new(0.0, 0.8, 0.0), V3::new(10.0, 0.0, 0.0), 0.0, &targets).unwrap();
        assert_eq!(h.entity, 1);
        assert!(close(h.point.x, 4.0) && close(h.normal.x, -1.0) && h.triangle >= 0);
        // too short to reach anything
        assert!(pick(V3::zeros(), V3::new(1.0, 0.0, 0.0), 0.0, &targets).is_none());
    }

    #[test]
    fn swept_pick_uses_radius() {
        let targets = vec![(5, Shape::Sphere { center: V3::new(5.0, 1.2, 0.0), radius: 0.5 })];
        assert!(pick(V3::zeros(), V3::new(10.0, 0.0, 0.0), 0.0, &targets).is_none());
        let h = pick(V3::zeros(), V3::new(10.0, 0.0, 0.0), 1.0, &targets).unwrap();
        assert_eq!(h.entity, 5);
    }
}
