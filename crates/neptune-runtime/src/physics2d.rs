//! 2D physics (ext) on Fyrox's 2D Rapier world.
//!
//! Bodies are made with `CreateBody2D` and get shapes with `Body2DBox`,
//! `Body2DCircle`, `Body2DCapsule`, `Body2DPolygon` and `Body2DSegment` (or
//! `TileMapBody2D` for a whole tile map). Everything is in 2D drawing
//! coordinates: pixels, Y down, angles in degrees clockwise, so a body's
//! `Body2DX/Y/Angle` can be passed straight to `DrawImageEx` (or use
//! `DrawBody2D`). Internally one metre is `Physics2DScale` pixels (50 by
//! default), which keeps Rapier in its preferred range. The world steps once
//! per frame on its own.

use fyrox::{
    core::{
        algebra::{Point2, UnitQuaternion, Vector2, Vector3},
        pool::Handle,
    },
    graph::SceneGraph,
    scene::{
        base::BaseBuilder,
        dim2::{
            collider::{BallShape, CapsuleShape, Collider, ColliderBuilder, ColliderShape, CuboidShape, SegmentShape, TriangleShape},
            joint::{BallJoint, FixedJoint, JointBuilder, JointParams, PrismaticJoint},
            physics::{Intersection, RayCastOptions},
            rigidbody::{RigidBody, RigidBodyBuilder},
        },
        collider::InteractionGroups,
        node::Node,
        rigidbody::RigidBodyType,
        transform::TransformBuilder,
    },
};

use crate::world::Host3D;

/// Default pixels per metre.
pub const DEFAULT_SCALE: f32 = 50.0;

#[derive(Debug, Clone)]
pub struct Body2D {
    pub node: Handle<Node>,
    pub colliders: Vec<Handle<Node>>,
}

/// The last `RayCast2D` hit (pixels).
#[derive(Debug, Clone, Copy, Default)]
pub struct Hit2D {
    pub body: i32,
    pub x: f32,
    pub y: f32,
    pub nx: f32,
    pub ny: f32,
}

/// A polygon as triangles (a fan from the first point); points in order.
pub fn triangulate(points: &[(f32, f32)]) -> Vec<[(f32, f32); 3]> {
    (1..points.len().saturating_sub(1)).map(|i| [points[0], points[i], points[i + 1]]).collect()
}

/// Horizontal runs of solid cells in each row: (x, y, length).
pub fn tile_runs(map: &crate::tilemap::TileMap) -> Vec<(i32, i32, i32)> {
    let mut runs = Vec::new();
    for y in 0..map.height {
        let mut x = 0;
        while x < map.width {
            if map.get(x, y) >= 0 {
                let start = x;
                while x < map.width && map.get(x, y) >= 0 {
                    x += 1;
                }
                runs.push((start, y, x - start));
            } else {
                x += 1;
            }
        }
    }
    runs
}

impl Host3D<'_> {
    fn scale2d(&self) -> f32 {
        self.world.physics2d_scale
    }

    pub fn body2d(&self, b: i32) -> Result<&Body2D, String> {
        usize::try_from(b - 1)
            .ok()
            .and_then(|i| self.world.bodies2d.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Body {b} does not exist"))
    }

    fn rigid2d(&mut self, b: i32) -> Result<&mut RigidBody, String> {
        let node = self.body2d(b)?.node;
        self.graph()?[node].cast_mut::<RigidBody>().ok_or_else(|| "body node missing".to_string())
    }

    /// The first 2D body sets Y-down gravity in pixels.
    fn init_physics2d(&mut self) -> Result<(), String> {
        if !self.world.physics2d_ready {
            self.world.physics2d_ready = true;
            self.set_gravity2d(0.0, 9.81 * DEFAULT_SCALE)?;
        }
        Ok(())
    }

    pub fn set_gravity2d(&mut self, gx: f32, gy: f32) -> Result<(), String> {
        let s = self.scale2d();
        self.graph()?.physics2d.gravity.set_value_and_mark_modified(Vector2::new(gx / s, gy / s));
        Ok(())
    }

    /// `CreateBody2D`: 0 dynamic, 1 static, 2 kinematic.
    pub fn create_body2d(&mut self, kind: i32, x: f32, y: f32) -> Result<i32, String> {
        self.scene()?;
        self.init_physics2d()?;
        let body_type = match kind {
            1 => RigidBodyType::Static,
            2 => RigidBodyType::KinematicPositionBased,
            _ => RigidBodyType::Dynamic,
        };
        let s = self.scale2d();
        let node = RigidBodyBuilder::new(
            BaseBuilder::new()
                .with_name("Body2D")
                .with_local_transform(TransformBuilder::new().with_local_position(Vector3::new(x / s, y / s, 0.0)).build()),
        )
        .with_body_type(body_type)
        .with_can_sleep(true)
        .build(self.graph()?);
        let node: Handle<Node> = node.transmute();
        let body = Body2D { node, colliders: Vec::new() };
        match self.world.bodies2d.iter().position(Option::is_none) {
            Some(i) => {
                self.world.bodies2d[i] = Some(body);
                Ok(i as i32 + 1)
            }
            None => {
                self.world.bodies2d.push(Some(body));
                Ok(self.world.bodies2d.len() as i32)
            }
        }
    }

    pub fn free_body2d(&mut self, b: i32) -> Result<(), String> {
        let node = self.body2d(b)?.node;
        self.graph()?.remove_node(node);
        self.world.bodies2d[b as usize - 1] = None;
        Ok(())
    }

    /// Adds a shape (in metres, relative to the body) at a pixel offset.
    fn add_shape2d(&mut self, b: i32, shape: ColliderShape, ox: f32, oy: f32) -> Result<(), String> {
        let s = self.scale2d();
        let body = self.body2d(b)?.node;
        let (friction, bounce, density, sensor) = self.world.body2d_material.get(&b).copied().unwrap_or((0.5, 0.0, 1.0, false));
        let graph = self.graph()?;
        let col = ColliderBuilder::new(
            BaseBuilder::new().with_local_transform(TransformBuilder::new().with_local_position(Vector3::new(ox / s, oy / s, 0.0)).build()),
        )
        .with_shape(shape)
        .with_friction(friction)
        .with_restitution(bounce)
        .with_density(Some(density))
        .with_sensor(sensor)
        .build(graph);
        let col: Handle<Node> = col.transmute();
        graph.link_nodes(col, body);
        let bd = self.world.bodies2d[b as usize - 1].as_mut().unwrap();
        bd.colliders.push(col);
        Ok(())
    }

    pub fn body2d_box(&mut self, b: i32, w: f32, h: f32, ox: f32, oy: f32) -> Result<(), String> {
        let s = self.scale2d();
        let shape = ColliderShape::Cuboid(CuboidShape { half_extents: Vector2::new((w / 2.0 / s).max(1e-3), (h / 2.0 / s).max(1e-3)) });
        self.add_shape2d(b, shape, ox, oy)
    }

    pub fn body2d_circle(&mut self, b: i32, r: f32, ox: f32, oy: f32) -> Result<(), String> {
        let s = self.scale2d();
        self.add_shape2d(b, ColliderShape::Ball(BallShape { radius: (r / s).max(1e-3) }), ox, oy)
    }

    /// An upright capsule `w` wide and `h` tall (a pill: good for characters).
    pub fn body2d_capsule(&mut self, b: i32, w: f32, h: f32, ox: f32, oy: f32) -> Result<(), String> {
        let s = self.scale2d();
        let r = (w / 2.0).max(0.5);
        let half = ((h / 2.0) - r).max(0.0) / s;
        let shape = ColliderShape::Capsule(CapsuleShape { begin: Vector2::new(0.0, -half), end: Vector2::new(0.0, half), radius: r / s });
        self.add_shape2d(b, shape, ox, oy)
    }

    pub fn body2d_segment(&mut self, b: i32, x1: f32, y1: f32, x2: f32, y2: f32) -> Result<(), String> {
        let s = self.scale2d();
        let shape = ColliderShape::Segment(SegmentShape { begin: Vector2::new(x1 / s, y1 / s), end: Vector2::new(x2 / s, y2 / s) });
        self.add_shape2d(b, shape, 0.0, 0.0)
    }

    /// A convex or star-shaped polygon from "x1,y1,x2,y2,..." (pixels,
    /// relative to the body).
    pub fn body2d_polygon(&mut self, b: i32, points: &str) -> Result<(), String> {
        let nums: Vec<f32> = points
            .split([',', ' ', ';'])
            .filter(|t| !t.trim().is_empty())
            .map(|t| t.trim().parse::<f32>().map_err(|_| format!("Body2DPolygon: '{t}' is not a number")))
            .collect::<Result<_, _>>()?;
        if nums.len() < 6 || !nums.len().is_multiple_of(2) {
            return Err("Body2DPolygon needs at least three x,y points".into());
        }
        let s = self.scale2d();
        let pts: Vec<(f32, f32)> = nums.chunks(2).map(|c| (c[0] / s, c[1] / s)).collect();
        for t in triangulate(&pts) {
            let v = |p: (f32, f32)| Vector2::new(p.0, p.1);
            self.add_shape2d(b, ColliderShape::Triangle(TriangleShape { a: v(t[0]), b: v(t[1]), c: v(t[2]) }), 0.0, 0.0)?;
        }
        Ok(())
    }

    /// `TileMapBody2D`: a static body with a box for each row of solid
    /// tiles, the map's top-left corner at (x, y).
    pub fn tilemap_body2d(&mut self, map: i32, x: f32, y: f32, tile_w: f32, tile_h: f32) -> Result<i32, String> {
        let runs = {
            let m = self.world.tilemaps.get(map as usize - 1).and_then(Option::as_ref).ok_or_else(|| format!("Tile map {map} does not exist"))?;
            tile_runs(m)
        };
        let b = self.create_body2d(1, x, y)?;
        for (cx, cy, len) in runs {
            let w = len as f32 * tile_w;
            self.body2d_box(b, w, tile_h, cx as f32 * tile_w + w / 2.0, cy as f32 * tile_h + tile_h / 2.0)?;
        }
        Ok(b)
    }

    /// Changes friction, bounce, density or sensor on a body's shapes (and
    /// shapes added later).
    pub fn body2d_material(&mut self, b: i32, f: impl FnOnce(&mut (f32, f32, f32, bool))) -> Result<(), String> {
        self.body2d(b)?;
        let mut m = self.world.body2d_material.get(&b).copied().unwrap_or((0.5, 0.0, 1.0, false));
        f(&mut m);
        self.world.body2d_material.insert(b, m);
        let cols = self.body2d(b)?.colliders.clone();
        let graph = self.graph()?;
        for c in cols {
            if let Some(col) = graph[c].cast_mut::<Collider>() {
                col.set_friction(m.0);
                col.set_restitution(m.1);
                col.set_density(Some(m.2));
                col.set_is_sensor(m.3);
            }
        }
        Ok(())
    }

    pub fn position_body2d(&mut self, b: i32, x: f32, y: f32) -> Result<(), String> {
        let s = self.scale2d();
        let node = self.body2d(b)?.node;
        self.graph()?[node].local_transform_mut().set_position(Vector3::new(x / s, y / s, 0.0));
        Ok(())
    }

    pub fn rotate_body2d(&mut self, b: i32, angle: f32) -> Result<(), String> {
        let node = self.body2d(b)?.node;
        self.graph()?[node].local_transform_mut().set_rotation(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), angle.to_radians()));
        Ok(())
    }

    pub fn body2d_pos(&mut self, b: i32) -> Result<(f32, f32, f32), String> {
        let s = self.scale2d();
        let node = self.body2d(b)?.node;
        let t = self.graph()?[node].local_transform();
        let p = **t.position();
        let angle = t.rotation().euler_angles().2.to_degrees();
        Ok((p.x * s, p.y * s, angle))
    }

    pub fn body2d_mut(&mut self, b: i32, f: impl FnOnce(&mut RigidBody, f32)) -> Result<(), String> {
        let s = self.scale2d();
        let rb = self.rigid2d(b)?;
        f(rb, s);
        rb.wake_up();
        Ok(())
    }

    pub fn body2d_velocity(&mut self, b: i32) -> Result<(f32, f32, f32), String> {
        let s = self.scale2d();
        let rb = self.rigid2d(b)?;
        let v = rb.lin_vel();
        Ok((v.x * s, v.y * s, rb.ang_vel().to_degrees()))
    }

    fn body_of_collider2d(&self, c: Handle<Node>) -> i32 {
        self.world
            .bodies2d
            .iter()
            .position(|b| b.as_ref().is_some_and(|b| b.colliders.contains(&c)))
            .map_or(0, |i| i as i32 + 1)
    }

    /// Bodies touching `b` (all bodies when `b`'s shapes overlap them).
    pub fn touching2d(&mut self, b: i32) -> Result<Vec<i32>, String> {
        let cols = self.body2d(b)?.colliders.clone();
        let mut others: Vec<Handle<Node>> = Vec::new();
        {
            let graph = self.graph()?;
            for c in &cols {
                let Some(col) = graph[*c].cast::<Collider>() else { continue };
                let me: Handle<Collider> = c.transmute();
                others.extend(col.contacts(&graph.physics2d).filter(|p| p.has_any_active_contact).map(|p| p.other(me).transmute::<Node>()));
                others.extend(col.intersects(&graph.physics2d).filter(|p| p.has_any_active_contact).map(|p| p.other(me).transmute::<Node>()));
            }
        }
        let mut out: Vec<i32> = others.into_iter().map(|c| self.body_of_collider2d(c)).filter(|&i| i != 0 && i != b).collect();
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    /// `RayCast2D`: the first body along the line, or 0.
    pub fn raycast2d(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) -> Result<i32, String> {
        let s = self.scale2d();
        let dir = Vector2::new(x2 - x1, y2 - y1) / s;
        let len = dir.norm();
        self.world.ray2d = Hit2D::default();
        if len < 1e-9 {
            return Ok(0);
        }
        let mut hits: Vec<Intersection> = Vec::new();
        self.graph()?.physics2d.cast_ray(
            RayCastOptions {
                ray_origin: Point2::new(x1 / s, y1 / s),
                ray_direction: dir,
                max_len: len,
                groups: InteractionGroups::default(),
                sort_results: true,
            },
            &mut hits,
        );
        let Some(hit) = hits.first() else { return Ok(0) };
        let body = self.body_of_collider2d(hit.collider.transmute());
        self.world.ray2d = Hit2D { body, x: hit.position.x * s, y: hit.position.y * s, nx: hit.normal.x, ny: hit.normal.y };
        Ok(body)
    }

    /// Joints: 1 hinge (bodies turn about the point), 2 fixed, 3 slider
    /// (bodies move along the direction `angle`, degrees clockwise from +X).
    pub fn create_joint2d(&mut self, a: i32, b: i32, kind: i32, x: f32, y: f32, angle: f32) -> Result<i32, String> {
        let (na, nb) = (self.body2d(a)?.node, self.body2d(b)?.node);
        let s = self.scale2d();
        let params = match kind {
            1 => JointParams::BallJoint(BallJoint::default()),
            2 => JointParams::FixedJoint(FixedJoint),
            3 => JointParams::PrismaticJoint(PrismaticJoint::default()),
            _ => return Err(format!("joint kind must be 1 (hinge), 2 (fixed) or 3 (slider), not {kind}")),
        };
        let node = JointBuilder::new(
            BaseBuilder::new().with_name("Joint2D").with_local_transform(
                TransformBuilder::new()
                    .with_local_position(Vector3::new(x / s, y / s, 0.0))
                    .with_local_rotation(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), angle.to_radians()))
                    .build(),
            ),
        )
        .with_params(params)
        .with_body1(na.transmute())
        .with_body2(nb.transmute())
        .with_contacts_enabled(false)
        .build(self.graph()?);
        self.world.joints2d.push(Some(node.transmute()));
        Ok(self.world.joints2d.len() as i32)
    }

    pub fn free_joint2d(&mut self, j: i32) -> Result<(), String> {
        let node = usize::try_from(j - 1)
            .ok()
            .and_then(|i| self.world.joints2d.get(i).copied().flatten())
            .ok_or_else(|| format!("Joint {j} does not exist"))?;
        self.graph()?.remove_node(node);
        self.world.joints2d[j as usize - 1] = None;
        Ok(())
    }

    /// Outlines of every body's shapes in the current colour (debugging).
    pub fn draw_bodies2d(&mut self) -> Result<(), String> {
        let s = self.scale2d();
        let mut lines: Vec<(Vector2<f32>, Vector2<f32>)> = Vec::new();
        {
            let bodies: Vec<Body2D> = self.world.bodies2d.iter().flatten().cloned().collect();
            let graph = self.graph()?;
            for b in &bodies {
                let bt = graph[b.node].local_transform();
                let (bp, ba) = (**bt.position(), bt.rotation().euler_angles().2);
                for c in &b.colliders {
                    let Some(col) = graph[*c].cast::<Collider>() else { continue };
                    let off = **col.local_transform().position();
                    let to_world = |p: Vector2<f32>| {
                        let q = p + Vector2::new(off.x, off.y);
                        let (sn, cs) = ba.sin_cos();
                        Vector2::new(bp.x + q.x * cs - q.y * sn, bp.y + q.x * sn + q.y * cs) * s
                    };
                    let mut poly = |pts: &[Vector2<f32>]| {
                        for i in 0..pts.len() {
                            lines.push((to_world(pts[i]), to_world(pts[(i + 1) % pts.len()])));
                        }
                    };
                    match col.shape() {
                        ColliderShape::Cuboid(c) => {
                            let h = c.half_extents;
                            poly(&[Vector2::new(-h.x, -h.y), Vector2::new(h.x, -h.y), Vector2::new(h.x, h.y), Vector2::new(-h.x, h.y)]);
                        }
                        ColliderShape::Ball(b) => {
                            let pts: Vec<_> = (0..16).map(|i| {
                                let a = i as f32 / 16.0 * std::f32::consts::TAU;
                                Vector2::new(a.cos(), a.sin()) * b.radius
                            }).collect();
                            poly(&pts);
                        }
                        ColliderShape::Capsule(c) => {
                            // the straight sides and both round ends
                            let r = c.radius;
                            let d = (c.end - c.begin).try_normalize(1e-9).unwrap_or_else(Vector2::y);
                            let n = Vector2::new(-d.y, d.x);
                            let mut pts = Vec::new();
                            for i in 0..=8 {
                                let a = std::f32::consts::PI * i as f32 / 8.0;
                                pts.push(c.end + (n * a.cos() + d * a.sin()) * r);
                            }
                            for i in 0..=8 {
                                let a = std::f32::consts::PI * i as f32 / 8.0;
                                pts.push(c.begin - (n * a.cos() + d * a.sin()) * r);
                            }
                            poly(&pts);
                        }
                        ColliderShape::Segment(g) => lines.push((to_world(g.begin), to_world(g.end))),
                        ColliderShape::Triangle(t) => poly(&[t.a, t.b, t.c]),
                        _ => {}
                    }
                }
            }
        }
        for (a, b) in lines {
            self.world.canvas.line(a.x, a.y, b.x, b.y);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_fan() {
        let t = triangulate(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        assert_eq!(t.len(), 2);
        assert_eq!(t[1], [(0.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
    }

    #[test]
    fn tile_rows_become_runs() {
        let mut m = crate::tilemap::TileMap::new(1, 6, 2).unwrap();
        for x in [0, 1, 2, 4] {
            m.set(x, 1, 0);
        }
        m.set(5, 0, 3);
        assert_eq!(tile_runs(&m), vec![(5, 0, 1), (0, 1, 3), (4, 1, 1)]);
    }
}
