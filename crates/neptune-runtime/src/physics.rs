//! Rigid-body physics (ext) on Fyrox's built-in Rapier engine.
//!
//! `EntityPhysics` turns an entity into a rigid body. Rapier moves a body by
//! moving its node, so the entity gets a new root node, the *body*, carrying
//! its position and rotation, with a collider child and the entity's
//! original node (its visible mesh) as another child that keeps the scale.
//! `sync` and `live_xf` in `world.rs` know about this split: programs keep
//! using `PositionEntity`, `EntityX` and friends as usual.
//!
//! Vectors cross the Blitz/Fyrox mirror like positions (`x` negated);
//! angular quantities (torque, angular velocity) are pseudovectors, so under
//! the mirror their `y` and `z` are negated instead.
//!
//! The engine steps the simulation once per frame on its own; `UpdateWorld`
//! (or its alias `UpdatePhysics`) does not need to be called for it.

use fyrox::{
    core::{
        algebra::{Point3, UnitQuaternion, Vector3},
        pool::Handle,
    },
    graph::SceneGraph,
    scene::{
        base::BaseBuilder,
        collider::{
            BallShape, CapsuleShape, Collider, ColliderBuilder, ColliderShape, ConvexPolyhedronShape, CuboidShape, CylinderShape,
            GeometrySource, InteractionGroups, TrimeshShape,
        },
        graph::physics::{Intersection, RayCastOptions},
        joint::{BallJoint, FixedJoint, JointBuilder, JointParams, RevoluteJoint},
        node::Node,
        rigidbody::{RigidBody, RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
    },
};

use crate::world::Host3D;

pub const SHAPE_AUTO: i32 = 0;
pub const SHAPE_BOX: i32 = 1;
pub const SHAPE_SPHERE: i32 = 2;
pub const SHAPE_CAPSULE: i32 = 3;
pub const SHAPE_CYLINDER: i32 = 4;
pub const SHAPE_MESH: i32 = 5;
pub const SHAPE_HULL: i32 = 6;

/// Physics nodes of an entity.
#[derive(Clone, Copy, Debug)]
pub struct PhysState {
    pub body: Handle<Node>,
    pub collider: Handle<Node>,
    pub body_type: i32,
}

/// Blitz → Fyrox for direction vectors.
pub fn vec_to_fyrox(v: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(-v.x, v.y, v.z)
}

/// Blitz → Fyrox for angular (pseudo)vectors; its own inverse.
pub fn angular_to_fyrox(v: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(v.x, -v.y, -v.z)
}

impl Host3D<'_> {
    pub fn phys(&self, e: i32) -> Result<PhysState, String> {
        self.ent(e)?.phys.ok_or_else(|| format!("Entity {e} has no physics (use EntityPhysics)"))
    }

    pub fn body_mut(&mut self, e: i32) -> Result<&mut RigidBody, String> {
        let p = self.phys(e)?;
        self.graph()?[p.body].cast_mut::<RigidBody>().ok_or_else(|| "rigid body missing".to_string())
    }

    pub fn collider_mut(&mut self, e: i32) -> Result<&mut Collider, String> {
        let p = self.phys(e)?;
        self.graph()?[p.collider].cast_mut::<Collider>().ok_or_else(|| "collider missing".to_string())
    }

    /// `EntityPhysics`: makes an entity a rigid body.
    pub fn enable_physics(&mut self, e: i32, body_type: i32, shape: i32, mass: f32) -> Result<(), String> {
        if self.ent(e)?.phys.is_some() {
            return Err(format!("Entity {e} already has physics"));
        }
        if self.ent(e)?.parent != 0 {
            return Err("EntityPhysics: the entity must not have a parent (use EntityParent entity, 0 first)".into());
        }
        if !(1..=3).contains(&body_type) {
            return Err(format!("EntityPhysics type must be 1 (dynamic), 2 (static) or 3 (kinematic), got {body_type}"));
        }
        self.flush_meshes()?;
        let (node, xf) = {
            let ent = self.ent(e)?;
            (ent.node, ent.xf)
        };
        // collision shape from the mesh, in the entity's scaled local space
        let tris = self.mesh_triangles(e)?;
        let (lo, hi) = crate::model::bounds(&tris).unwrap_or((Vector3::new(-1.0, -1.0, -1.0), Vector3::new(1.0, 1.0, 1.0)));
        let (lo, hi) = (lo.component_mul(&xf.scale), hi.component_mul(&xf.scale));
        let (lo, hi) = (lo.inf(&hi), lo.sup(&hi));
        let size = (hi - lo).map(|c| c.max(1e-3));
        let center = vec_to_fyrox((lo + hi) / 2.0);
        let shape = match shape {
            SHAPE_AUTO if body_type == 2 && !tris.is_empty() => SHAPE_MESH,
            SHAPE_AUTO => SHAPE_BOX,
            s => s,
        };
        let mesh_nodes = self.entity_mesh_nodes(e)?;
        let (collider_shape, offset) = match shape {
            SHAPE_SPHERE => (ColliderShape::Ball(BallShape { radius: size.max() / 2.0 }), center),
            SHAPE_CAPSULE => {
                let r = size.x.max(size.z) / 2.0;
                let half = (size.y / 2.0 - r).max(0.0);
                (
                    ColliderShape::Capsule(CapsuleShape { begin: Vector3::new(0.0, -half, 0.0), end: Vector3::new(0.0, half, 0.0), radius: r }),
                    center,
                )
            }
            SHAPE_CYLINDER => {
                (ColliderShape::Cylinder(CylinderShape { half_height: size.y / 2.0, radius: size.x.max(size.z) / 2.0 }), center)
            }
            SHAPE_MESH | SHAPE_HULL => {
                if mesh_nodes.is_empty() {
                    return Err(format!("Entity {e} has no mesh for a mesh/hull collider"));
                }
                let s = if shape == SHAPE_MESH {
                    ColliderShape::Trimesh(TrimeshShape { sources: mesh_nodes.iter().map(|&n| GeometrySource(n)).collect() })
                } else {
                    ColliderShape::Polyhedron(ConvexPolyhedronShape { geometry_source: GeometrySource(mesh_nodes[0]) })
                };
                (s, Vector3::zeros())
            }
            SHAPE_BOX => (ColliderShape::Cuboid(CuboidShape { half_extents: size / 2.0 }), center),
            s => return Err(format!("unknown physics shape {s}")),
        };
        let kind = match body_type {
            2 => RigidBodyType::Static,
            3 => RigidBodyType::KinematicPositionBased,
            _ => RigidBodyType::Dynamic,
        };
        let graph = self.graph()?;
        let collider = graph.add_node(
            ColliderBuilder::new(
                BaseBuilder::new().with_name("Collider").with_local_transform(TransformBuilder::new().with_local_position(offset).build()),
            )
            .with_shape(collider_shape)
            .with_friction(0.5)
            .build_node(),
        );
        let body = graph.add_node(
            RigidBodyBuilder::new(
                BaseBuilder::new().with_name("Body").with_local_transform(
                    TransformBuilder::new().with_local_position(xf.fyrox_position()).with_local_rotation(xf.fyrox_rotation()).build(),
                ),
            )
            .with_body_type(kind)
            .with_mass(mass.max(1e-4))
            .build_node(),
        );
        graph.link_nodes(collider, body);
        graph.link_nodes(node, body);
        // the visible node keeps only the scale; the body has position and rotation
        let t = graph[node].local_transform_mut();
        t.set_position(Vector3::zeros());
        t.set_rotation(UnitQuaternion::identity());
        t.set_scale(xf.scale);
        // mesh colliders read their geometry relative to the body
        self.ent_mut(e)?.phys = Some(PhysState { body, collider, body_type });
        Ok(())
    }

    /// Entity whose collider is `c`.
    pub fn entity_of_collider(&self, c: Handle<Node>) -> i32 {
        self.entities()
            .find(|(_, e)| e.phys.is_some_and(|p| p.collider.index() == c.index() && p.collider.generation() == c.generation()))
            .map_or(0, |(i, _)| i)
    }

    /// Entities whose colliders are touching `e`'s collider.
    pub fn touching(&mut self, e: i32) -> Result<Vec<i32>, String> {
        let p = self.phys(e)?;
        let others: Vec<Handle<Node>> = {
            let scene = self.scene()?;
            let Some(col) = scene.graph[p.collider].cast::<Collider>() else { return Ok(Vec::new()) };
            let me: Handle<Collider> = p.collider.transmute();
            col.contacts(&scene.graph.physics).filter(|c| c.has_any_active_contact).map(|c| c.other(me).transmute()).collect()
        };
        let mut out: Vec<i32> = others.into_iter().map(|c| self.entity_of_collider(c)).filter(|&i| i != 0).collect();
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    /// Casts a ray (Blitz space) against physics colliders.
    pub fn physics_ray(&mut self, origin: Vector3<f32>, dir: Vector3<f32>) -> Result<Option<crate::pick::PickHit>, String> {
        let len = dir.norm();
        if len < 1e-9 {
            return Ok(None);
        }
        let mut hits: Vec<Intersection> = Vec::new();
        {
            let scene = self.scene()?;
            scene.graph.physics.cast_ray(
                RayCastOptions {
                    ray_origin: Point3::from(vec_to_fyrox(origin)),
                    ray_direction: vec_to_fyrox(dir),
                    max_len: len,
                    groups: InteractionGroups::default(),
                    sort_results: true,
                },
                &mut hits,
            );
        }
        let Some(hit) = hits.first() else { return Ok(None) };
        let entity = self.entity_of_collider(hit.collider.transmute());
        let p = hit.position.coords;
        Ok(Some(crate::pick::PickHit {
            entity,
            point: Vector3::new(-p.x, p.y, p.z),
            normal: Vector3::new(-hit.normal.x, hit.normal.y, hit.normal.z),
            time: hit.toi / len,
            triangle: -1,
        }))
    }

    /// Joints: ball (kind 1), hinge (2) or fixed (3) between two bodies at a
    /// world point; hinges turn about `axis`.
    pub fn create_joint(&mut self, a: i32, b: i32, kind: i32, at: Vector3<f32>, axis: Vector3<f32>) -> Result<i32, String> {
        let (pa, pb) = (self.phys(a)?, self.phys(b)?);
        let params = match kind {
            1 => JointParams::BallJoint(BallJoint::default()),
            2 => JointParams::RevoluteJoint(RevoluteJoint::default()),
            _ => JointParams::FixedJoint(FixedJoint),
        };
        // a revolute joint turns about its local X axis
        let axis_f = vec_to_fyrox(axis).try_normalize(1e-9).unwrap_or_else(Vector3::x);
        let rot = UnitQuaternion::rotation_between(&Vector3::x(), &axis_f).unwrap_or_else(|| UnitQuaternion::from_axis_angle(&Vector3::y_axis(), std::f32::consts::PI));
        let node = JointBuilder::new(
            BaseBuilder::new()
                .with_name("Joint")
                .with_local_transform(TransformBuilder::new().with_local_position(vec_to_fyrox(at)).with_local_rotation(rot).build()),
        )
        .with_params(params)
        .with_body1(pa.body.transmute())
        .with_body2(pb.body.transmute())
        .with_contacts_enabled(false)
        .build_node();
        let h = self.graph()?.add_node(node);
        self.world.joints.push(h);
        Ok(self.world.joints.len() as i32)
    }

    pub fn free_joint(&mut self, j: i32) -> Result<(), String> {
        let h = usize::try_from(j - 1).ok().and_then(|i| self.world.joints.get(i)).copied().unwrap_or(Handle::NONE);
        if h.is_some() && self.graph()?.try_get(h).is_ok() {
            self.graph()?.remove_node(h);
        }
        if let Some(slot) = usize::try_from(j - 1).ok().and_then(|i| self.world.joints.get_mut(i)) {
            *slot = Handle::NONE;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::rotation;
    use fyrox::core::algebra::Matrix3;

    #[test]
    fn angular_vectors_mirror_as_pseudovectors() {
        // rotating about Blitz axis w then mirroring equals rotating the mirrored
        // point about the converted axis
        let w = Vector3::new(0.3, 0.8, -0.5).normalize();
        let angle = 0.7;
        let s = Matrix3::from_diagonal(&Vector3::new(-1.0, 1.0, 1.0));
        let rb = nalgebra_rotation(w, angle);
        let rf = s * rb * s;
        let wf = angular_to_fyrox(w);
        // a proper rotation about wf with the same angle in Fyrox's frame
        let expect = nalgebra_rotation(wf, angle);
        assert!((rf - expect).norm() < 1e-5, "{rf} vs {expect}");
        let _ = rotation(0.0, 0.0, 0.0);
    }

    fn nalgebra_rotation(axis: Vector3<f32>, angle: f32) -> Matrix3<f32> {
        UnitQuaternion::from_axis_angle(&fyrox::core::algebra::Unit::new_normalize(axis), angle).to_rotation_matrix().into_inner()
    }
}
