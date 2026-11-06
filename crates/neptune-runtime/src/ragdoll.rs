//! Ragdolls (ext): `CreateRagdoll`, `RagdollEnable`, `RagdollActive`,
//! `RagdollImpulse`, `FreeRagdoll`.
//!
//! `CreateRagdoll` builds physics for the skeleton of an animated model: a
//! capsule body from each bone to its child bones, joined to its parent's
//! body by a ball joint with limited bending. Limbs do not collide with one
//! another, only with the rest of the physics world (`EntityPhysics`). The
//! ragdoll starts off: the bodies follow the animation. `RagdollEnable`
//! turns it on: the animation stops and the bones follow the bodies, so the
//! character goes limp and falls; turning it off gives the bones back to
//! `Animate`.

use std::collections::{HashMap, HashSet};

use fyrox::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        pool::Handle,
    },
    graph::SceneGraph,
    scene::{
        base::BaseBuilder,
        collider::{BitMask, CapsuleShape, ColliderBuilder, ColliderShape, InteractionGroups},
        joint::{BallJoint, JointBuilder, JointParams},
        mesh::Mesh,
        node::Node,
        ragdoll::{Limb, Ragdoll, RagdollBuilder},
        rigidbody::{RigidBody, RigidBodyBuilder, RigidBodyType},
        transform::{Transform, TransformBuilder},
    },
};

use crate::world::Host3D;

/// Collision group of ragdoll limbs (they ignore each other).
const LIMB_GROUP: u32 = 1 << 30;

#[derive(Debug, Clone)]
pub struct RagdollState {
    pub node: Handle<Node>,
    pub bodies: Vec<Handle<Node>>,
    /// Bones' local transforms when the ragdoll was made: the ragdoll
    /// overwrites them (positions too), and animations may not set all of
    /// them back.
    pub rest: Vec<(Handle<Node>, Transform)>,
}

/// One bone of the skeleton, in world space.
struct BoneInfo {
    pos: Vector3<f32>,
    rot: UnitQuaternion<f32>,
    parent: Option<Handle<Node>>,
    children: Vec<Handle<Node>>,
}

impl Host3D<'_> {
    /// `CreateRagdoll`: builds a ragdoll for an animated model; `thickness`
    /// scales the limbs' radius.
    pub fn create_ragdoll(&mut self, e: i32, thickness: f32) -> Result<(), String> {
        if self.ent(e)?.ragdoll.is_some() {
            self.free_ragdoll(e)?;
        }
        let meshes = self.entity_mesh_nodes(e)?;
        let graph = self.graph()?;
        // world transforms must be current (the model may have just loaded)
        graph.update_hierarchical_data();
        let mut bones: HashSet<Handle<Node>> = HashSet::new();
        for m in &meshes {
            if let Some(mesh) = graph[*m].cast::<Mesh>() {
                for s in mesh.surfaces() {
                    bones.extend(s.bones().iter().copied());
                }
            }
        }
        if bones.is_empty() {
            return Err(format!("Entity {e} has no skeleton for a ragdoll"));
        }
        // the skeleton as a tree of bones
        let mut info: HashMap<Handle<Node>, BoneInfo> = HashMap::new();
        for &b in &bones {
            let node = &graph[b];
            let m = node.global_transform();
            // the model's scale is in the matrix: take the rotation alone
            let mut basis = m.fixed_view::<3, 3>(0, 0).into_owned();
            for mut c in basis.column_iter_mut() {
                let n = c.norm().max(1e-9);
                c /= n;
            }
            let rot = UnitQuaternion::from_matrix(&basis).into_inner();
            let mut parent = node.parent();
            while parent.is_some() && !bones.contains(&parent) {
                parent = graph[parent].parent();
            }
            info.insert(b, BoneInfo { pos: node.global_position(), rot: UnitQuaternion::new_normalize(rot), parent: parent.is_some().then_some(parent), children: Vec::new() });
        }
        let links: Vec<(Handle<Node>, Handle<Node>)> = info.iter().filter_map(|(&b, i)| i.parent.map(|p| (p, b))).collect();
        for (p, c) in links {
            info.get_mut(&p).unwrap().children.push(c);
        }
        // size of the skeleton, for limb thickness
        let (mut lo, mut hi) = (Vector3::repeat(f32::MAX), Vector3::repeat(f32::MIN));
        for i in info.values() {
            lo = lo.inf(&i.pos);
            hi = hi.sup(&i.pos);
        }
        let size = (hi - lo).norm().max(1e-3);
        // limbs: bones with child bones (the ends of fingers and tails follow their parents)
        let limb_bones: HashSet<Handle<Node>> = info.iter().filter(|(_, i)| !i.children.is_empty()).map(|(&b, _)| b).collect();
        let ragdoll_node = graph.add_node(fyrox::scene::pivot::PivotBuilder::new(BaseBuilder::new().with_name("RagdollParts")).build_node());
        let mut bodies: HashMap<Handle<Node>, Handle<Node>> = HashMap::new();
        for &b in &limb_bones {
            let i = &info[&b];
            let end = i.children.iter().map(|c| info[c].pos).sum::<Vector3<f32>>() / i.children.len() as f32;
            let len = (end - i.pos).norm();
            let radius = (len * 0.22 * thickness).clamp(size * 0.015, size * 0.12);
            let local_end = i.rot.inverse() * (end - i.pos);
            let body = RigidBodyBuilder::new(
                BaseBuilder::new()
                    .with_name("Limb")
                    .with_local_transform(TransformBuilder::new().with_local_position(i.pos).with_local_rotation(i.rot).build()),
            )
            .with_body_type(RigidBodyType::KinematicPositionBased)
            .build_node();
            let body = graph.add_node(body);
            graph.link_nodes(body, ragdoll_node);
            let groups = InteractionGroups { memberships: BitMask(LIMB_GROUP), filter: BitMask(!LIMB_GROUP) };
            let col = ColliderBuilder::new(BaseBuilder::new())
                .with_shape(ColliderShape::Capsule(CapsuleShape { begin: Vector3::zeros(), end: local_end, radius }))
                .with_collision_groups(groups)
                .with_density(Some(1.0))
                .with_sensor(true)
                .build_node();
            let col = graph.add_node(col);
            graph.link_nodes(col, body);
            bodies.insert(b, body);
        }
        // limb tree and joints: each limb hangs from the nearest limb above it
        let limb_parent = |b: Handle<Node>| {
            let mut p = info[&b].parent;
            while let Some(h) = p {
                if limb_bones.contains(&h) {
                    return Some(h);
                }
                p = info[&h].parent;
            }
            None
        };
        let mut kids: HashMap<Handle<Node>, Vec<Handle<Node>>> = HashMap::new();
        let mut roots = Vec::new();
        for &b in &limb_bones {
            match limb_parent(b) {
                Some(p) => kids.entry(p).or_default().push(b),
                None => roots.push(b),
            }
        }
        if roots.is_empty() {
            return Err("CreateRagdoll: the skeleton has no root bone".into());
        }
        // one root: the extra roots hang from the biggest one
        let count = |r: &Handle<Node>| -> usize {
            let mut n = 0;
            let mut stack = vec![*r];
            while let Some(x) = stack.pop() {
                n += 1;
                stack.extend(kids.get(&x).into_iter().flatten().copied());
            }
            n
        };
        roots.sort_by_key(|r| std::cmp::Reverse(count(r)));
        let root = roots[0];
        for r in roots.iter().skip(1) {
            kids.entry(root).or_default().push(*r);
        }
        let limit = 50f32.to_radians();
        for (&p, cs) in &kids {
            for &c in cs {
                let joint = JointBuilder::new(BaseBuilder::new().with_name("LimbJoint").with_local_transform(
                    TransformBuilder::new().with_local_position(info[&c].pos).with_local_rotation(info[&c].rot).build(),
                ))
                .with_params(JointParams::BallJoint(BallJoint {
                    x_limits_enabled: true,
                    x_limits_angles: -limit..limit,
                    y_limits_enabled: true,
                    y_limits_angles: -limit..limit,
                    z_limits_enabled: true,
                    z_limits_angles: -limit..limit,
                }))
                .with_body1(bodies[&p].transmute())
                .with_body2(bodies[&c].transmute())
                .with_contacts_enabled(false)
                .build_node();
                let joint = graph.add_node(joint);
                graph.link_nodes(joint, ragdoll_node);
            }
        }
        fn build(b: Handle<Node>, kids: &HashMap<Handle<Node>, Vec<Handle<Node>>>, bodies: &HashMap<Handle<Node>, Handle<Node>>) -> Limb {
            Limb {
                bone: b,
                physical_bone: bodies[&b].transmute(),
                children: kids.get(&b).into_iter().flatten().map(|&c| build(c, kids, bodies)).collect(),
            }
        }
        let limb = build(root, &kids, &bodies);
        let rag = RagdollBuilder::new(BaseBuilder::new().with_name("Ragdoll"))
            .with_root_limb(limb)
            .with_active(false)
            .with_deactivate_colliders(true)
            .build_node();
        let rag = graph.add_node(rag);
        graph.link_nodes(ragdoll_node, rag);
        let body_list: Vec<Handle<Node>> = bodies.values().copied().collect();
        let rest = bones.iter().map(|&b| (b, graph[b].local_transform().clone())).collect();
        self.ent_mut(e)?.ragdoll = Some(RagdollState { node: rag, bodies: body_list, rest });
        Ok(())
    }

    fn ragdoll_state(&self, e: i32) -> Result<RagdollState, String> {
        self.ent(e)?.ragdoll.clone().ok_or_else(|| format!("Entity {e} has no ragdoll (use CreateRagdoll)"))
    }

    /// `RagdollEnable`: physics on (limp) or off (animated).
    pub fn ragdoll_enable(&mut self, e: i32, on: bool) -> Result<(), String> {
        let r = self.ragdoll_state(e)?;
        if on && self.ent(e)?.anim.is_some() {
            // the bones now follow the bodies, not the animation
            self.animate(e, 0, 1.0, 0, 0.0)?;
        }
        let graph = self.graph()?;
        if let Some(rag) = graph[r.node].cast_mut::<Ragdoll>() {
            rag.is_active.set_value_and_mark_modified(on);
        }
        if !on {
            // back to the rest pose; Animate takes it from there
            for (b, t) in &r.rest {
                if let Ok(node) = graph.try_get_mut(*b) {
                    *node.local_transform_mut() = t.clone();
                }
            }
        }
        Ok(())
    }

    pub fn ragdoll_active(&mut self, e: i32) -> Result<bool, String> {
        let r = self.ragdoll_state(e)?;
        Ok(self.graph()?[r.node].cast::<Ragdoll>().is_some_and(|rag| *rag.is_active))
    }

    /// `RagdollImpulse`: pushes every limb (Blitz space), e.g. from a hit.
    pub fn ragdoll_impulse(&mut self, e: i32, v: Vector3<f32>) -> Result<(), String> {
        let r = self.ragdoll_state(e)?;
        let n = r.bodies.len().max(1) as f32;
        let f = crate::physics::vec_to_fyrox(v) / n;
        let graph = self.graph()?;
        for b in r.bodies {
            if let Some(rb) = graph[b].cast_mut::<RigidBody>() {
                rb.apply_impulse(f);
                rb.wake_up();
            }
        }
        Ok(())
    }

    pub fn free_ragdoll(&mut self, e: i32) -> Result<(), String> {
        let r = self.ragdoll_state(e)?;
        let graph = self.graph()?;
        if graph.try_get(r.node).is_ok() {
            graph.remove_node(r.node);
        }
        self.ent_mut(e)?.ragdoll = None;
        Ok(())
    }
}
