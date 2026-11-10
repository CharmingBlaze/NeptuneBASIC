//! Character controllers in the game world (ext): `CreateCharController` and
//! `CreateCharController2D`.
//!
//! The rules are in `charcontrol.rs`; this connects them to the physics
//! worlds. Each character has a kinematic body, so dynamic bodies bump into
//! it and rays hit it, and sweeps its shape with Fyrox's `cast_shape`. A 3D
//! character moves an entity whose origin is at its feet. A 2D character has
//! a 2D body whose position is the middle of its feet, in pixels.

use std::collections::HashSet;
use std::time::Instant;

use fyrox::{
    core::{
        algebra::{Isometry2, Isometry3, Matrix4, Point2, Point3, Vector2, Vector3},
        pool::Handle,
    },
    graph::SceneGraph,
    scene::{
        base::BaseBuilder,
        collider::{BitMask, CapsuleShape, ColliderBuilder, ColliderShape, InteractionGroups, QueryFilterFlags, TOIStatus},
        dim2,
        graph::physics as physics3d,
        node::Node,
        rigidbody::{RigidBody, RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
    },
};

use crate::charcontrol::{CharWorld, Character, Hit, Settings, V3};
use crate::physics::vec_to_fyrox;
use crate::transform::rotation;
use crate::world::Host3D;

pub const SHAPE_BOX: i32 = 0;
pub const SHAPE_CAPSULE: i32 = 1;
/// Triggers don't block characters. Fyrox's flag values are one bit higher
/// than Rapier's and are passed on unchanged, so its `EXCLUDE_SENSORS` would
/// exclude every solid; this one arrives at Rapier as "exclude sensors".
const SKIP_SENSORS: QueryFilterFlags = QueryFilterFlags::EXCLUDE_DYNAMIC;
/// The collision group of one-way 2D bodies.
const ONE_WAY_GROUP: u32 = 1 << 31;

#[derive(Debug, Clone, Copy)]
pub enum Link {
    Entity { entity: i32, body: Handle<Node>, collider: Handle<Node>, radius: f32 },
    Body2D { body: i32, collider: Handle<Node>, shape: i32, width: f32 },
}

#[derive(Debug, Clone)]
pub struct Slot {
    pub c: Character,
    pub link: Link,
    last: Instant,
    /// Where the controller last put its entity, to notice `PositionEntity`.
    placed: V3,
    applied_height: f32,
    pub facing: i32,
    /// Length of the last update.
    turn: f32,
    /// (ext) On a player's computer: moves now, the host checks it after.
    pub predict: Option<Box<crate::netchar::Predictor>>,
}

/// Everything the world keeps for characters.
#[derive(Debug, Default)]
pub struct Store {
    pub list: Vec<Option<Slot>>,
    /// Physics entities and 2D bodies only solid from above.
    pub one_way_3d: HashSet<i32>,
    pub one_way_2d: HashSet<i32>,
}

struct World3D<'a, 'b> {
    h: &'a mut Host3D<'b>,
    me: Handle<Node>,
    radius: f32,
}

impl CharWorld for World3D<'_, '_> {
    fn cast(&mut self, feet: V3, motion: V3, height: f32, ignore: &[i32]) -> Option<Hit> {
        let scene = self.h.scene_ref()?;
        let half = (height / 2.0 - self.radius).max(0.001);
        let shape = rapier3d::geometry::Capsule::new_y(half, self.radius);
        let centre = vec_to_fyrox(feet + V3::new(0.0, height / 2.0, 0.0));
        let pose = Isometry3::translation(centre.x, centre.y, centre.z);
        let host = &*self.h;
        let keep = |c: Handle<Node>, _: &fyrox::scene::collider::Collider| ignore.is_empty() || !ignore.contains(&host.entity_of_collider(c));
        let filter = physics3d::QueryFilter {
            flags: SKIP_SENSORS,
            exclude_collider: Some(self.me),
            predicate: Some(&keep),
            ..Default::default()
        };
        let (collider, toi) = scene.graph.physics.cast_shape(&scene.graph, &shape, &pose, &vec_to_fyrox(motion), 1.0, true, filter)?;
        let id = host.entity_of_collider(collider);
        let dynamic = host.ent(id).is_ok_and(|e| e.phys.is_some_and(|p| p.body_type == 1));
        // shape 1 is what was hit, shape 2 the character: its normal is in world space
        let n = -toi.normal2.into_inner();
        let store = &host.world.chars;
        Some(Hit {
            t: toi.toi,
            normal: V3::new(-n.x, n.y, n.z),
            id,
            one_way: store.one_way_3d.contains(&id),
            dynamic,
            penetrating: toi.status == TOIStatus::Penetrating,
        })
    }

    fn probe(&mut self, from: V3, motion: V3) -> Option<Hit> {
        let scene = self.h.scene_ref()?;
        let origin = vec_to_fyrox(from);
        let len = motion.norm();
        let mut hits: Vec<physics3d::Intersection> = Vec::new();
        scene.graph.physics.cast_ray(
            physics3d::RayCastOptions {
                ray_origin: Point3::from(origin),
                ray_direction: vec_to_fyrox(motion),
                max_len: len,
                groups: InteractionGroups::default(),
                sort_results: true,
            },
            &mut hits,
        );
        let me: Handle<fyrox::scene::collider::Collider> = self.me.transmute();
        let hit = hits.iter().find(|i| i.collider != me)?;
        Some(Hit {
            t: (hit.position.coords - origin).norm() / len,
            normal: V3::new(-hit.normal.x, hit.normal.y, hit.normal.z),
            id: self.h.entity_of_collider(hit.collider.transmute()),
            one_way: false,
            dynamic: false,
            penetrating: false,
        })
    }

    fn frame_of(&mut self, id: i32) -> Option<Matrix4<f32>> {
        self.h.world_of(id).ok().map(|w| w.matrix)
    }

    fn push(&mut self, id: i32, dir: V3, speed: f32, strength: f32) {
        let Ok(rb) = self.h.body_mut(id) else { return };
        let dir = vec_to_fyrox(dir);
        let mut v = rb.lin_vel();
        let along = v.dot(&dir);
        if along < speed {
            v += dir * (speed - along) * (strength / rb.mass().max(1e-3)).min(1.0);
            rb.set_lin_vel(v);
            rb.wake_up();
        }
    }
}

struct World2D<'a, 'b> {
    h: &'a mut Host3D<'b>,
    me: Handle<Node>,
    shape: i32,
    width: f32,
}

impl CharWorld for World2D<'_, '_> {
    fn cast(&mut self, feet: V3, motion: V3, height: f32, ignore: &[i32]) -> Option<Hit> {
        let s = self.h.world.physics2d_scale;
        let scene = self.h.scene_ref()?;
        let (hw, hh) = (self.width / 2.0 / s, height / 2.0 / s);
        let pose = Isometry2::translation(feet.x / s, feet.y / s - hh);
        let velocity = Vector2::new(motion.x / s, motion.y / s);
        let host = &*self.h;
        // Fyrox's 2D filter can't take a predicate (it looks for a 3D collider),
        // so one-way bodies have a collision group of their own to leave out
        let groups = (!ignore.is_empty()).then(|| InteractionGroups::new(BitMask(u32::MAX), BitMask(!ONE_WAY_GROUP)));
        let filter = dim2::physics::QueryFilter { flags: SKIP_SENSORS, exclude_collider: Some(self.me), groups, ..Default::default() };
        let physics = &scene.graph.physics2d;
        let found = if self.shape == SHAPE_CAPSULE {
            let c = rapier2d::geometry::Capsule::new_y((hh - hw).max(0.001), hw);
            physics.cast_shape(&scene.graph, &c, &pose, &velocity, 1.0, true, filter)
        } else {
            let c = rapier2d::geometry::Cuboid::new(rapier2d::math::Vec2::new(hw, hh));
            physics.cast_shape(&scene.graph, &c, &pose, &velocity, 1.0, true, filter)
        };
        let (collider, toi) = found?;
        let id = host.body_of_collider2d(collider);
        let dynamic = host.body2d(id).ok().and_then(|b| scene.graph[b.node].cast::<dim2::rigidbody::RigidBody>().map(|r| r.body_type() == RigidBodyType::Dynamic)).unwrap_or(false);
        // shape 1 is what was hit, shape 2 the character: its normal is in world space
        let n = -toi.normal2.into_inner();
        Some(Hit {
            t: toi.toi,
            normal: V3::new(n.x, n.y, 0.0),
            id,
            one_way: host.world.chars.one_way_2d.contains(&id),
            dynamic,
            penetrating: toi.status == TOIStatus::Penetrating,
        })
    }

    fn probe(&mut self, from: V3, motion: V3) -> Option<Hit> {
        let s = self.h.world.physics2d_scale;
        let scene = self.h.scene_ref()?;
        let origin = Point2::new(from.x / s, from.y / s);
        let direction = Vector2::new(motion.x / s, motion.y / s);
        let len = direction.norm();
        let mut hits: Vec<dim2::physics::Intersection> = Vec::new();
        scene.graph.physics2d.cast_ray(
            dim2::physics::RayCastOptions { ray_origin: origin, ray_direction: direction, max_len: len, groups: InteractionGroups::default(), sort_results: true },
            &mut hits,
        );
        let me: Handle<dim2::collider::Collider> = self.me.transmute();
        let hit = hits.iter().find(|i| i.collider != me)?;
        Some(Hit {
            t: (hit.position.coords - origin.coords).norm() / len,
            normal: V3::new(hit.normal.x, hit.normal.y, 0.0),
            id: self.h.body_of_collider2d(hit.collider.transmute()),
            one_way: false,
            dynamic: false,
            penetrating: false,
        })
    }

    fn frame_of(&mut self, id: i32) -> Option<Matrix4<f32>> {
        let (x, y, angle) = self.h.body2d_pos(id).ok()?;
        Some(Matrix4::new_translation(&V3::new(x, y, 0.0)) * Matrix4::from_axis_angle(&Vector3::z_axis(), angle.to_radians()))
    }

    fn push(&mut self, id: i32, dir: V3, speed: f32, strength: f32) {
        let _ = self.h.body2d_mut(id, |rb, s| {
            let dir = Vector2::new(dir.x, dir.y);
            let mut v = rb.lin_vel();
            let wanted = speed / s;
            let along = v.dot(&dir);
            if along < wanted {
                v += dir * (wanted - along) * (strength / rb.mass().max(1e-3)).min(1.0);
                rb.set_lin_vel(v);
            }
        });
    }
}

fn add_slot(h: &mut Host3D, slot: Slot) -> i32 {
    let list = &mut h.world.chars.list;
    match list.iter().position(Option::is_none) {
        Some(i) => {
            list[i] = Some(slot);
            i as i32 + 1
        }
        None => {
            list.push(Some(slot));
            list.len() as i32
        }
    }
}

fn capsule3(radius: f32, height: f32) -> ColliderShape {
    let half = (height / 2.0 - radius).max(0.001);
    ColliderShape::Capsule(CapsuleShape { begin: Vector3::new(0.0, -half, 0.0), end: Vector3::new(0.0, half, 0.0), radius })
}

/// `CreateCharController`: the entity's origin is where the feet are.
pub fn create_3d(h: &mut Host3D, entity: i32, radius: f32, height: f32) -> Result<i32, String> {
    h.scene()?;
    if h.ent(entity)?.phys.is_some() {
        return Err(format!("Entity {entity} has physics; a character moves by itself"));
    }
    if h.world.chars.list.iter().flatten().any(|s| matches!(s.link, Link::Entity { entity: e, .. } if e == entity)) {
        return Err(format!("Entity {entity} already has a character controller"));
    }
    let (radius, height) = (radius.max(0.01), height.max(radius * 2.0 + 0.01));
    let feet = h.world_of(entity)?.position();
    let graph = h.graph()?;
    let collider = graph.add_node(
        ColliderBuilder::new(BaseBuilder::new().with_name("CharCollider").with_local_transform(TransformBuilder::new().with_local_position(Vector3::new(0.0, height / 2.0, 0.0)).build()))
            .with_shape(capsule3(radius, height))
            .build_node(),
    );
    let body = graph.add_node(
        RigidBodyBuilder::new(BaseBuilder::new().with_name("CharBody").with_local_transform(TransformBuilder::new().with_local_position(vec_to_fyrox(feet)).build()))
            .with_body_type(RigidBodyType::KinematicPositionBased)
            .with_can_sleep(false)
            .build_node(),
    );
    graph.link_nodes(collider, body);
    let mut s = Settings::defaults_3d();
    s.crouch_height = (height * 0.55).max(radius * 2.0 + 0.01);
    let mut c = Character::new(feet, V3::y(), height, s);
    c.foot_radius = radius;
    // a controller that starts on the ground is grounded on its first update
    c.grounded = false;
    Ok(add_slot(h, Slot { c, link: Link::Entity { entity, body, collider, radius }, last: Instant::now(), placed: feet, applied_height: height, facing: 1, turn: 0.0, predict: None }))
}

fn shape2d(shape: i32, width: f32, height: f32, scale: f32) -> dim2::collider::ColliderShape {
    let (hw, hh) = (width / 2.0 / scale, height / 2.0 / scale);
    if shape == SHAPE_CAPSULE {
        let half = (hh - hw).max(0.001);
        dim2::collider::ColliderShape::Capsule(dim2::collider::CapsuleShape { begin: Vector2::new(0.0, -half), end: Vector2::new(0.0, half), radius: hw })
    } else {
        dim2::collider::ColliderShape::Cuboid(dim2::collider::CuboidShape { half_extents: Vector2::new(hw, hh) })
    }
}

/// `CreateCharController2D`: x, y is the middle of the feet.
pub fn create_2d(h: &mut Host3D, x: f32, y: f32, width: f32, height: f32, shape: i32) -> Result<i32, String> {
    if !(0..=1).contains(&shape) {
        return Err(format!("CreateCharController2D shape must be 0 (box) or 1 (capsule), got {shape}"));
    }
    let (width, height) = (width.max(1.0), height.max(1.0));
    let body = h.create_body2d(2, x, y)?;
    if shape == SHAPE_CAPSULE {
        h.body2d_capsule(body, width, height, 0.0, -height / 2.0)?;
    } else {
        h.body2d_box(body, width, height, 0.0, -height / 2.0)?;
    }
    let collider = *h.body2d(body)?.colliders.first().ok_or("collider missing")?;
    let mut c = Character::new(V3::new(x, y, 0.0), V3::new(0.0, -1.0, 0.0), height, Settings::defaults_2d());
    if shape == SHAPE_CAPSULE {
        c.foot_radius = width / 2.0;
    }
    Ok(add_slot(h, Slot { c, link: Link::Body2D { body, collider, shape, width }, last: Instant::now(), placed: V3::new(x, y, 0.0), applied_height: height, facing: 1, turn: 0.0, predict: None }))
}

pub fn slot<'a>(h: &'a Host3D, id: i32) -> Result<&'a Slot, String> {
    usize::try_from(id - 1).ok().and_then(|i| h.world.chars.list.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Character {id} does not exist"))
}

pub fn slot_mut<'a>(h: &'a mut Host3D, id: i32) -> Result<&'a mut Slot, String> {
    usize::try_from(id - 1).ok().and_then(|i| h.world.chars.list.get_mut(i)).and_then(Option::as_mut).ok_or_else(|| format!("Character {id} does not exist"))
}

/// Shows a crouch to the rest of the physics world.
fn apply_height(h: &mut Host3D, slot: &mut Slot) -> Result<(), String> {
    let height = slot.c.height;
    if (height - slot.applied_height).abs() < 1e-5 {
        return Ok(());
    }
    slot.applied_height = height;
    match slot.link {
        Link::Entity { collider, radius, .. } => {
            let graph = h.graph()?;
            graph[collider].local_transform_mut().set_position(Vector3::new(0.0, height / 2.0, 0.0));
            if let Some(c) = graph[collider].cast_mut::<fyrox::scene::collider::Collider>() {
                c.set_shape(capsule3(radius, height));
            }
        }
        Link::Body2D { collider, shape, width, .. } => {
            let s = h.world.physics2d_scale;
            let graph = h.graph()?;
            graph[collider].local_transform_mut().set_position(Vector3::new(0.0, -height / 2.0 / s, 0.0));
            if let Some(c) = graph[collider].cast_mut::<dim2::collider::Collider>() {
                c.set_shape(shape2d(shape, width, height, s));
            }
        }
    }
    Ok(())
}

fn apply_position(h: &mut Host3D, slot: &mut Slot) -> Result<(), String> {
    let pos = slot.c.pos;
    match slot.link {
        Link::Entity { entity, body, .. } => {
            let shown = pos + slot.predict.as_ref().map_or(V3::zeros(), |p| p.visual);
            crate::commands::set_world_position(h, entity, shown)?;
            slot.placed = h.world_of(entity)?.position();
            let at = vec_to_fyrox(pos);
            let graph = h.graph()?;
            graph[body].local_transform_mut().set_position(at);
            if let Some(b) = graph[body].cast_mut::<RigidBody>() {
                b.set_next_kinematic_translation(at);
            }
            let rate = slot.c.s.face_rate;
            let wish = slot.c.wish;
            if rate > 0.0 && wish.x * wish.x + wish.z * wish.z > 1e-4 {
                turn_towards(h, entity, crate::transform::vector_yaw(&V3::new(wish.x, 0.0, wish.z)), slot.c.s.face_rate, slot.turn_time())?;
            }
        }
        Link::Body2D { body, .. } => {
            let s = h.world.physics2d_scale;
            h.position_body2d(body, pos.x, pos.y)?;
            let node = h.body2d(body)?.node;
            if let Some(b) = h.graph()?[node].cast_mut::<dim2::rigidbody::RigidBody>() {
                b.set_next_kinematic_translation(Vector2::new(pos.x / s, pos.y / s));
            }
            slot.placed = pos;
        }
    }
    Ok(())
}

impl Slot {
    /// How long the last update was, for turning.
    fn turn_time(&self) -> f32 {
        self.turn
    }
}

fn turn_towards(h: &mut Host3D, entity: i32, target: f32, rate: f32, dt: f32) -> Result<(), String> {
    let (_, yaw, _) = crate::transform::angles(&h.live_xf(h.ent(entity)?).rot);
    let diff = (target - yaw + 540.0).rem_euclid(360.0) - 180.0;
    let step = diff.clamp(-rate * dt, rate * dt);
    h.set_xf(entity, |xf| xf.rot = rotation(0.0, yaw + step, 0.0))
}

fn run(h: &mut Host3D, slot: &mut Slot, dt: f32) -> Result<(), String> {
    let now = Instant::now();
    let dt = if dt > 0.0 { dt } else { (now - slot.last).as_secs_f32().min(0.1) };
    // the host refuses anything longer
    let dt = if slot.predict.is_some() { dt.min(crate::netchar::MAX_INPUT_DT) } else { dt };
    slot.last = now;
    slot.turn = dt;
    // PositionEntity between updates means "put the character here"
    if let Link::Entity { entity, .. } = slot.link {
        let p = h.world_of(entity)?.position();
        if (p - slot.placed).norm() > 1e-3 {
            slot.c.teleport(p);
        }
    }
    if slot.c.wish.x > 0.01 {
        slot.facing = 1;
    } else if slot.c.wish.x < -0.01 {
        slot.facing = -1;
    }
    let input = slot.predict.as_mut().map(|p| p.capture(&slot.c, dt)).filter(|_| dt > 0.0);
    match slot.link {
        Link::Entity { collider, radius, .. } => slot.c.update(&mut World3D { h, me: collider, radius }, dt),
        Link::Body2D { collider, shape, width, .. } => slot.c.update(&mut World2D { h, me: collider, shape, width }, dt),
    }
    if let (Some(input), Some(p)) = (input, slot.predict.as_mut()) {
        p.record(input, &slot.c);
        p.settle(dt);
    }
    apply_height(h, slot)?;
    apply_position(h, slot)?;
    crate::netsync::send_inputs(h, slot)
}

/// (ext) Runs `f` with a character and the world it moves in, then shows the
/// result. For multiplayer, which moves characters outside `UpdateChar`.
pub fn with_world(h: &mut Host3D, id: i32, f: impl FnOnce(&mut Slot, &mut dyn CharWorld)) -> Result<(), String> {
    slot(h, id)?;
    let mut s = h.world.chars.list[id as usize - 1].take().ok_or("character missing")?;
    match s.link {
        Link::Entity { collider, radius, .. } => f(&mut s, &mut World3D { h, me: collider, radius }),
        Link::Body2D { collider, shape, width, .. } => f(&mut s, &mut World2D { h, me: collider, shape, width }),
    }
    // what is turning the model is not this
    s.turn = 0.0;
    let result = apply_height(h, &mut s).and_then(|_| apply_position(h, &mut s));
    h.world.chars.list[id as usize - 1] = Some(s);
    result
}

/// (ext) The controller of a 3D entity.
pub fn of_entity(h: &Host3D, entity: i32) -> Option<i32> {
    h.world.chars.list.iter().position(|s| s.as_ref().is_some_and(|s| matches!(s.link, Link::Entity { entity: e, .. } if e == entity))).map(|i| i as i32 + 1)
}

/// (ext) How a 3D controller was made: radius and standing height.
pub fn shape_of(h: &Host3D, id: i32) -> Result<(f32, f32), String> {
    let s = slot(h, id)?;
    match s.link {
        Link::Entity { radius, .. } => Ok((radius, s.c.stand_height)),
        Link::Body2D { .. } => Err("A 2D character has no entity to share".into()),
    }
}

/// `UpdateChar`: 0 seconds means "the time since the last update".
pub fn update(h: &mut Host3D, id: i32, dt: f32) -> Result<(), String> {
    slot(h, id)?;
    // out of the table while it runs, because the world is borrowed for it
    let mut s = h.world.chars.list[id as usize - 1].take().ok_or("character missing")?;
    let result = run(h, &mut s, dt);
    h.world.chars.list[id as usize - 1] = Some(s);
    result
}

/// `PositionChar`: moves it now, keeping its speed.
pub fn teleport(h: &mut Host3D, id: i32, pos: V3) -> Result<(), String> {
    slot(h, id)?;
    let mut s = h.world.chars.list[id as usize - 1].take().ok_or("character missing")?;
    s.c.teleport(pos);
    let result = apply_position(h, &mut s);
    h.world.chars.list[id as usize - 1] = Some(s);
    result
}

pub fn free(h: &mut Host3D, id: i32) -> Result<(), String> {
    let link = slot(h, id)?.link;
    h.world.chars.list[id as usize - 1] = None;
    match link {
        Link::Entity { body, .. } => {
            if let Ok(g) = h.graph() {
                g.remove_node(body);
            }
            Ok(())
        }
        Link::Body2D { body, .. } => h.free_body2d(body),
    }
}

/// The entity is going away: its character goes with it.
pub fn forget_entity(h: &mut Host3D, entity: i32) {
    let found = h.world.chars.list.iter().position(|s| s.as_ref().is_some_and(|s| matches!(s.link, Link::Entity { entity: e, .. } if e == entity)));
    if let Some(i) = found {
        let _ = free(h, i as i32 + 1);
    }
}

/// `Body2DOneWay`: other bodies still collide with it as before.
pub fn set_one_way_2d(h: &mut Host3D, body: i32, on: bool) -> Result<(), String> {
    let cols = h.body2d(body)?.colliders.clone();
    let groups = if on { InteractionGroups::new(BitMask(ONE_WAY_GROUP), BitMask(u32::MAX)) } else { InteractionGroups::default() };
    let graph = h.graph()?;
    for c in cols {
        if let Some(col) = graph[c].cast_mut::<dim2::collider::Collider>() {
            col.set_collision_groups(groups);
        }
    }
    if on {
        h.world.chars.one_way_2d.insert(body);
    } else {
        h.world.chars.one_way_2d.remove(&body);
    }
    Ok(())
}

pub fn get(h: &Host3D, id: i32, name: &str) -> Result<f32, String> {
    let mut s = slot(h, id)?.c.s.clone();
    s.field(name).map(|v| *v).ok_or_else(|| format!("unknown character setting '{name}'"))
}

pub fn set(h: &mut Host3D, id: i32, name: &str, value: f32) -> Result<(), String> {
    let s = &mut slot_mut(h, id)?.c.s;
    *s.field(name).ok_or_else(|| format!("unknown character setting '{name}'"))? = value;
    Ok(())
}
