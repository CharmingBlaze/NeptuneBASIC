//! Cloth and ropes (ext): `CreateCloth`, `CreateRope`, pins, colliders,
//! wind and tearing.
//!
//! Both are simulated as points joined by distance constraints (Verlet
//! integration with position-based constraints): a cloth is a grid of
//! points with stretch, shear and bend links, a rope a chain with stretch
//! and bend links. They fall under gravity, flap in the global wind
//! (`SetWindDirection`, `SetWindStrength`), rest on a floor height and slide
//! off the spheres of entities they collide with, and hang from pins: points
//! held in place, or held to an entity so they follow it (a cape on a
//! character, a lantern on a chain).
//!
//! A cloth or rope is a mesh entity. Its points start where its entity puts
//! them on the first frame (move and turn it first, then pin points), then
//! live in the world: moving the entity afterwards does not drag them
//! (`ResetCloth` places them again). The simulation runs at a fixed 120
//! steps a second by real time, when the world is rendered.

use std::time::Instant;

use fyrox::core::algebra::{Matrix4, Point3, Vector3};

use crate::mesh::Vert;
use crate::world::Host3D;

type V3 = Vector3<f32>;

const STEP: f32 = 1.0 / 120.0;
const MAX_STEPS: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// columns and rows of squares (points are one more each way)
    Cloth { cols: usize, rows: usize },
    /// segments, tube radius
    Rope { segs: usize, radius: f32 },
}

#[derive(Clone, Copy, Debug)]
struct Link {
    a: usize,
    b: usize,
    rest: f32,
    /// 0 stretch, 1 shear, 2 bend
    kind: u8,
    broken: bool,
}

#[derive(Clone, Copy, Debug)]
struct Pin {
    point: usize,
    /// 0 = held where it is
    entity: i32,
    /// world position (entity 0) or offset in the entity's space, once known
    at: Option<V3>,
}

#[derive(Clone, Debug)]
pub struct Soft {
    pub entity: i32,
    surface: i32,
    shape: Shape,
    /// starting layout in the entity's space
    local: Vec<V3>,
    p: Vec<V3>,
    prev: Vec<V3>,
    links: Vec<Link>,
    tris: Vec<[u32; 3]>,
    pins: Vec<Pin>,
    started: bool,
    last: Option<Instant>,
    owed: f32,
    /// simulated seconds (for the wind's gusts)
    clock: f32,
    pub stretch: f32,
    pub bend: f32,
    pub damping: f32,
    pub gravity: V3,
    pub wind: f32,
    pub tear: f32,
    pub floor: Option<f32>,
    /// entity, radius (0 = its EntityRadius)
    pub colliders: Vec<(i32, f32)>,
    seed: u32,
}

impl Soft {
    fn new(entity: i32, surface: i32, shape: Shape, local: Vec<V3>) -> Soft {
        let n = local.len();
        Soft {
            entity,
            surface,
            shape,
            local,
            p: vec![V3::zeros(); n],
            prev: vec![V3::zeros(); n],
            links: Vec::new(),
            tris: Vec::new(),
            pins: Vec::new(),
            started: false,
            last: None,
            owed: 0.0,
            clock: 0.0,
            stretch: 1.0,
            bend: 0.15,
            damping: 0.01,
            gravity: V3::new(0.0, -9.8, 0.0),
            wind: 1.0,
            tear: 0.0,
            floor: None,
            colliders: Vec::new(),
            seed: entity as u32 * 2_654_435_761,
        }
    }

    fn link(&mut self, a: usize, b: usize, kind: u8) {
        let rest = (self.local[a] - self.local[b]).norm();
        self.links.push(Link { a, b, rest, kind, broken: false });
    }

    /// A cloth `width` x `height` hanging in its entity's XY plane from the
    /// top edge, centred on the origin.
    pub fn cloth(entity: i32, surface: i32, width: f32, height: f32, cols: usize, rows: usize) -> Soft {
        let idx = |c: usize, r: usize| r * (cols + 1) + c;
        let mut local = Vec::new();
        for r in 0..=rows {
            for c in 0..=cols {
                local.push(V3::new(width * (c as f32 / cols as f32 - 0.5), -height * r as f32 / rows as f32, 0.0));
            }
        }
        let mut s = Soft::new(entity, surface, Shape::Cloth { cols, rows }, local);
        for r in 0..=rows {
            for c in 0..=cols {
                if c < cols {
                    s.link(idx(c, r), idx(c + 1, r), 0);
                }
                if r < rows {
                    s.link(idx(c, r), idx(c, r + 1), 0);
                }
                if c < cols && r < rows {
                    s.link(idx(c, r), idx(c + 1, r + 1), 1);
                    s.link(idx(c + 1, r), idx(c, r + 1), 1);
                }
                if c + 2 <= cols {
                    s.link(idx(c, r), idx(c + 2, r), 2);
                }
                if r + 2 <= rows {
                    s.link(idx(c, r), idx(c, r + 2), 2);
                }
            }
        }
        for r in 0..rows {
            for c in 0..cols {
                let (a, b, d, e) = (idx(c, r) as u32, idx(c + 1, r) as u32, idx(c, r + 1) as u32, idx(c + 1, r + 1) as u32);
                s.tris.push([a, d, b]);
                s.tris.push([b, d, e]);
            }
        }
        s
    }

    /// A rope from `a` to `b` (its entity's space) of `length` (at least the
    /// distance) in `segs` pieces.
    pub fn rope(entity: i32, surface: i32, a: V3, b: V3, segs: usize, radius: f32, length: f32) -> Soft {
        let local: Vec<V3> = (0..=segs).map(|i| a + (b - a) * (i as f32 / segs as f32)).collect();
        let mut s = Soft::new(entity, surface, Shape::Rope { segs, radius }, local);
        let piece = length.max((b - a).norm()) / segs as f32;
        for i in 0..segs {
            s.links.push(Link { a: i, b: i + 1, rest: piece, kind: 0, broken: false });
        }
        for i in 0..segs.saturating_sub(1) {
            s.links.push(Link { a: i, b: i + 2, rest: piece * 2.0, kind: 2, broken: false });
        }
        s.bend = 0.05;
        s
    }

    pub fn point_index(&self, col: i32, row: i32) -> Result<usize, String> {
        match self.shape {
            Shape::Cloth { cols, rows } => {
                if !(0..=cols as i32).contains(&col) || !(0..=rows as i32).contains(&row) {
                    return Err(format!("cloth point {col},{row} is outside 0-{cols}, 0-{rows}"));
                }
                Ok(row as usize * (cols + 1) + col as usize)
            }
            Shape::Rope { segs, .. } => {
                let i = if col < 0 { segs as i32 + 1 + col } else { col };
                if !(0..=segs as i32).contains(&i) {
                    return Err(format!("rope point {col} is outside 0-{segs} (-1 is the last)"));
                }
                Ok(i as usize)
            }
        }
    }

    pub fn pin(&mut self, point: usize, entity: i32) {
        self.pins.retain(|p| p.point != point);
        self.pins.push(Pin { point, entity, at: None });
    }

    pub fn unpin(&mut self, point: usize) {
        self.pins.retain(|p| p.point != point);
    }

    pub fn edge_points(&self, edge: i32) -> Vec<usize> {
        match self.shape {
            Shape::Cloth { cols, rows } => {
                let idx = |c: usize, r: usize| r * (cols + 1) + c;
                match edge {
                    1 => (0..=cols).map(|c| idx(c, rows)).collect(),
                    2 => (0..=rows).map(|r| idx(0, r)).collect(),
                    3 => (0..=rows).map(|r| idx(cols, r)).collect(),
                    _ => (0..=cols).map(|c| idx(c, 0)).collect(),
                }
            }
            Shape::Rope { segs, .. } => vec![if edge == 1 { segs } else { 0 }],
        }
    }

    pub fn point(&self, i: usize) -> V3 {
        if self.started {
            self.p[i]
        } else {
            self.local[i]
        }
    }

    /// Puts the points where the entity's transform `m` places the layout.
    fn place(&mut self, m: &Matrix4<f32>) {
        for (i, l) in self.local.iter().enumerate() {
            let w = m.transform_point(&Point3::from(*l)).coords;
            self.p[i] = w;
            self.prev[i] = w;
        }
        for pin in &mut self.pins {
            pin.at = None;
        }
        self.started = true;
    }

    /// The vertex normal of a cloth point from its neighbours.
    fn cloth_normal(&self, i: usize, cols: usize, rows: usize) -> V3 {
        let (c, r) = (i % (cols + 1), i / (cols + 1));
        let at = |c: usize, r: usize| self.p[r * (cols + 1) + c];
        let dx = at((c + 1).min(cols), r) - at(c.saturating_sub(1), r);
        let dy = at(c, (r + 1).min(rows)) - at(c, r.saturating_sub(1));
        dy.cross(&dx).try_normalize(1e-9).unwrap_or_else(V3::z)
    }

    /// One fixed step. `targets` gives each pin's world position now,
    /// `spheres` the colliders (centre, radius), `wind` the wind force.
    fn step(&mut self, targets: &[(usize, V3)], spheres: &[(V3, f32)], wind: V3, time: f32) {
        let n = self.p.len();
        let pinned: Vec<bool> = (0..n).map(|i| targets.iter().any(|(p, _)| *p == i)).collect();
        let normals: Vec<V3> = match self.shape {
            Shape::Cloth { cols, rows } => (0..n).map(|i| self.cloth_normal(i, cols, rows)).collect(),
            Shape::Rope { .. } => Vec::new(),
        };
        let keep = 1.0 - self.damping.clamp(0.0, 1.0);
        for i in 0..n {
            if pinned[i] {
                continue;
            }
            // gusts: the wind rises and falls, a little differently at each point
            let g = 0.75 + 0.35 * (time * 1.7 + i as f32 * 0.37 + self.seed as f32 * 1e-9).sin() + 0.2 * (time * 4.3 + i as f32 * 1.13).sin();
            let w = wind * g * self.wind;
            let force = match self.shape {
                // cloth catches the wind it faces
                Shape::Cloth { .. } => normals[i] * normals[i].dot(&w) + w * 0.15,
                Shape::Rope { .. } => w * 0.3,
            };
            let v = (self.p[i] - self.prev[i]) * keep;
            self.prev[i] = self.p[i];
            self.p[i] += v + (self.gravity + force) * STEP * STEP;
        }
        for (i, t) in targets {
            self.prev[*i] = self.p[*i];
            self.p[*i] = *t;
        }
        let iterations = 6;
        let mut tore = false;
        for it in 0..iterations {
            // alternate the order, so solving does not push everything one way
            let count = self.links.len();
            for j in 0..count {
                let k = if it % 2 == 0 { j } else { count - 1 - j };
                let l = self.links[k];
                if l.broken {
                    continue;
                }
                let s = match l.kind {
                    0 => self.stretch,
                    1 => self.stretch * 0.8,
                    _ => self.bend,
                }
                .clamp(0.0, 1.0);
                if s <= 0.0 {
                    continue;
                }
                let d = self.p[l.b] - self.p[l.a];
                let len = d.norm();
                if len < 1e-9 {
                    continue;
                }
                if self.tear > 0.0 && l.kind == 0 && len > l.rest * self.tear {
                    self.links[k].broken = true;
                    tore = true;
                    continue;
                }
                let (wa, wb) = (if pinned[l.a] { 0.0 } else { 1.0 }, if pinned[l.b] { 0.0 } else { 1.0 });
                if wa + wb == 0.0 {
                    continue;
                }
                let corr = d * ((len - l.rest) / len * s / (wa + wb));
                self.p[l.a] += corr * wa;
                self.p[l.b] -= corr * wb;
            }
            // collisions
            for (i, &held) in pinned.iter().enumerate().take(n) {
                if held {
                    continue;
                }
                for (c, r) in spheres {
                    let d = self.p[i] - c;
                    let dist = d.norm();
                    if dist < *r {
                        self.p[i] = c + if dist > 1e-6 { d / dist } else { V3::y() } * *r;
                        // friction: cloth lying on something does not slide off freely
                        self.prev[i] = self.p[i] * 0.5 + self.prev[i] * 0.5;
                    }
                }
                if let Some(f) = self.floor {
                    if self.p[i].y < f {
                        self.p[i].y = f;
                        // friction on the floor
                        self.prev[i].x = self.p[i].x * 0.6 + self.prev[i].x * 0.4;
                        self.prev[i].z = self.p[i].z * 0.6 + self.prev[i].z * 0.4;
                    }
                }
            }
        }
        if tore {
            self.drop_torn_triangles();
        }
    }

    /// Removes cloth triangles that have a broken edge.
    fn drop_torn_triangles(&mut self) {
        let broken: std::collections::HashSet<(usize, usize)> =
            self.links.iter().filter(|l| l.broken).map(|l| (l.a.min(l.b), l.a.max(l.b))).collect();
        self.tris.retain(|t| {
            let e = |a: u32, b: u32| (a.min(b) as usize, a.max(b) as usize);
            !(broken.contains(&e(t[0], t[1])) || broken.contains(&e(t[1], t[2])) || broken.contains(&e(t[2], t[0])))
        });
    }

    /// Vertices and triangles in the entity's space (`inv` = world to entity).
    fn mesh(&self, inv: &Matrix4<f32>) -> (Vec<Vert>, Vec<[u32; 3]>) {
        let to_local = |w: V3| inv.transform_point(&Point3::from(w)).coords;
        let to_local_dir = |d: V3| inv.transform_vector(&d).try_normalize(1e-9).unwrap_or(d);
        match self.shape {
            Shape::Cloth { cols, rows } => {
                let verts = (0..self.p.len())
                    .map(|i| {
                        let (c, r) = (i % (cols + 1), i / (cols + 1));
                        let uv = [c as f32 / cols as f32, r as f32 / rows as f32, 0.0];
                        Vert { pos: to_local(self.p[i]), normal: to_local_dir(self.cloth_normal(i, cols, rows)), color: [1.0; 4], uv: [uv, uv] }
                    })
                    .collect();
                (verts, self.tris.clone())
            }
            Shape::Rope { segs, radius } => {
                const SIDES: usize = 6;
                let mut verts = Vec::with_capacity((segs + 1) * (SIDES + 1));
                let mut tris = Vec::new();
                for i in 0..=segs {
                    let t = (self.p[(i + 1).min(segs)] - self.p[i.saturating_sub(1)]).try_normalize(1e-9).unwrap_or_else(V3::y);
                    let side = if t.y.abs() < 0.9 { V3::y() } else { V3::x() };
                    let u = t.cross(&side).normalize();
                    let w = t.cross(&u);
                    for k in 0..=SIDES {
                        let a = k as f32 / SIDES as f32 * std::f32::consts::TAU;
                        let n = u * a.cos() + w * a.sin();
                        let uv = [k as f32 / SIDES as f32, i as f32 / segs as f32, 0.0];
                        verts.push(Vert { pos: to_local(self.p[i] + n * radius), normal: to_local_dir(n), color: [1.0; 4], uv: [uv, uv] });
                    }
                }
                let ring = (SIDES + 1) as u32;
                for i in 0..segs as u32 {
                    for k in 0..SIDES as u32 {
                        let (a, b, c, d) = (i * ring + k, i * ring + k + 1, (i + 1) * ring + k, (i + 1) * ring + k + 1);
                        tris.push([a, b, c]);
                        tris.push([b, d, c]);
                    }
                }
                (verts, tris)
            }
        }
    }
}

impl Host3D<'_> {
    pub fn soft(&mut self, e: i32) -> Result<&mut Soft, String> {
        self.ent(e)?;
        self.world.softs.iter_mut().find(|s| s.entity == e).ok_or_else(|| format!("Entity {e} is not a cloth or rope"))
    }

    fn new_soft_mesh(&mut self) -> Result<(i32, i32), String> {
        let id = self.add_entity(crate::mesh::empty_mesh_node(), crate::world::Kind::Mesh, 0)?;
        let s = self.create_surface(id, 0)?;
        Ok((id, s))
    }

    /// `CreateCloth`
    pub fn create_cloth(&mut self, width: f32, height: f32, cols: i32, rows: i32) -> Result<i32, String> {
        let (cols, rows) = (cols.clamp(1, 64) as usize, rows.clamp(1, 64) as usize);
        let (id, s) = self.new_soft_mesh()?;
        // both sides show
        self.ent_mut(id)?.fx |= 16;
        self.refresh_material(id)?;
        let soft = Soft::cloth(id, s, width.abs().max(0.01), height.abs().max(0.01), cols, rows);
        self.world.softs.push(soft);
        self.write_soft_mesh(id)?;
        Ok(id)
    }

    /// `CreateRope` between two world points.
    pub fn create_rope(&mut self, a: V3, b: V3, segs: i32, radius: f32, length: f32) -> Result<i32, String> {
        let (id, s) = self.new_soft_mesh()?;
        let soft = Soft::rope(id, s, a, b, segs.clamp(1, 200) as usize, radius.abs().max(0.001), length);
        self.world.softs.push(soft);
        self.write_soft_mesh(id)?;
        Ok(id)
    }

    fn write_soft_mesh(&mut self, e: i32) -> Result<(), String> {
        let w = self.world_of(e)?;
        let inv = w.matrix.try_inverse().unwrap_or_else(Matrix4::identity);
        let soft = self.soft(e)?;
        let (verts, tris) = if soft.started { soft.mesh(&inv) } else { soft.mesh_unstarted() };
        let s = soft.surface;
        self.edit_surface(s, |info| {
            info.verts = verts;
            info.tris = tris;
        })
    }

    /// `ResetCloth`: back to the starting layout where the entity is now.
    pub fn reset_soft(&mut self, e: i32) -> Result<(), String> {
        let soft = self.soft(e)?;
        soft.started = false;
        soft.last = None;
        for l in &mut soft.links {
            l.broken = false;
        }
        if let Shape::Cloth { cols, rows } = soft.shape {
            let fresh = Soft::cloth(0, 0, 1.0, 1.0, cols, rows);
            soft.tris = fresh.tris;
        }
        Ok(())
    }

    /// Steps every cloth and rope in the current scene (before rendering).
    pub fn update_softs(&mut self) -> Result<(), String> {
        if self.world.softs.is_empty() {
            return Ok(());
        }
        // forget the ones whose entity was freed
        let alive: Vec<bool> = self.world.softs.iter().map(|s| self.ent_any(s.entity).is_some()).collect();
        let mut i = 0;
        self.world.softs.retain(|_| {
            i += 1;
            alive[i - 1]
        });
        let now = Instant::now();
        let ids: Vec<i32> = self.world.softs.iter().map(|s| s.entity).collect();
        let wind = V3::new(self.world.wind_dir.0, 0.0, self.world.wind_dir.1).try_normalize(1e-6).unwrap_or_else(V3::zeros) * (self.world.wind_strength * 30.0);
        for e in ids {
            if self.elsewhere(e) {
                continue;
            }
            let m = self.world_of(e)?.matrix;
            // where pins and colliders are now
            let (pins, colliders, started) = {
                let s = self.soft(e)?;
                (s.pins.clone(), s.colliders.clone(), s.started)
            };
            if !started {
                self.soft(e)?.place(&m);
            }
            let mut targets = Vec::new();
            let mut fixed_pins = pins.clone();
            for (k, pin) in pins.iter().enumerate() {
                let here = self.soft(e)?.p[pin.point];
                let target = if pin.entity == 0 {
                    *fixed_pins[k].at.get_or_insert(here)
                } else if let Ok(w) = self.world_of(pin.entity) {
                    let inv = w.matrix.try_inverse().unwrap_or_else(Matrix4::identity);
                    let off = *fixed_pins[k].at.get_or_insert_with(|| inv.transform_point(&Point3::from(here)).coords);
                    w.matrix.transform_point(&Point3::from(off)).coords
                } else {
                    here
                };
                targets.push((pin.point, target));
            }
            let mut spheres = Vec::new();
            for (ce, r) in colliders {
                let Ok(w) = self.world_of(ce) else { continue };
                let r = if r > 0.0 { r } else { self.ent(ce).map(|x| x.radius.0).unwrap_or(1.0) };
                spheres.push((w.position(), r + 0.02));
            }
            let s = self.soft(e)?;
            s.pins = fixed_pins;
            let dt = s.last.map_or(STEP, |l| now.duration_since(l).as_secs_f32()).min(MAX_STEPS as f32 * STEP);
            s.last = Some(now);
            s.owed += dt;
            let mut steps = 0;
            while s.owed >= STEP && steps < MAX_STEPS {
                s.clock += STEP;
                let clock = s.clock;
                s.step(&targets, &spheres, wind, clock);
                s.owed -= STEP;
                steps += 1;
            }
            if s.owed > STEP {
                s.owed = 0.0;
            }
            self.write_soft_mesh(e)?;
        }
        Ok(())
    }
}

impl Soft {
    /// The mesh before the first frame: the layout itself.
    fn mesh_unstarted(&self) -> (Vec<Vert>, Vec<[u32; 3]>) {
        let mut copy = self.clone();
        copy.p = self.local.clone();
        copy.started = true;
        copy.mesh(&Matrix4::identity())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &mut Soft, steps: usize, spheres: &[(V3, f32)]) {
        if !s.started {
            s.place(&Matrix4::identity());
        }
        let targets: Vec<(usize, V3)> = s.pins.iter().map(|p| (p.point, s.local[p.point])).collect();
        for k in 0..steps {
            s.step(&targets, spheres, V3::zeros(), k as f32 * STEP);
        }
    }

    #[test]
    fn a_pinned_cloth_hangs_and_keeps_its_size() {
        let mut s = Soft::cloth(1, 1, 2.0, 2.0, 8, 8);
        for p in s.edge_points(0) {
            s.pin(p, 0);
        }
        run(&mut s, 600, &[]);
        // the top stays, the bottom hangs below it, about as long as the cloth
        let top = s.p[s.point_index(4, 0).unwrap()];
        let bottom = s.p[s.point_index(4, 8).unwrap()];
        assert!((top - V3::new(0.0, 0.0, 0.0)).norm() < 1e-4, "{top}");
        assert!(bottom.y < -1.8 && bottom.y > -2.3, "{bottom}");
        // the mesh has every triangle and a normal for each point
        let (v, t) = s.mesh(&Matrix4::identity());
        assert_eq!((v.len(), t.len()), (81, 128));
    }

    #[test]
    fn cloth_drapes_over_a_sphere_and_rests_on_the_floor() {
        let mut s = Soft::cloth(1, 1, 3.0, 3.0, 12, 12);
        // lay it flat above a ball
        for l in &mut s.local {
            *l = V3::new(l.x, 2.0, -l.y - 1.5);
        }
        s.floor = Some(0.0);
        let ball = (V3::new(0.0, 1.0, 0.0), 0.8);
        // (balanced on top it is unstable, like a real towel on a ball: check it
        // after it has settled, not seconds later)
        run(&mut s, 300, &[ball]);
        for p in &s.p {
            assert!((p - ball.0).norm() >= 0.8 - 0.05, "inside the ball: {p}");
            assert!(p.y >= -1e-4, "below the floor: {p}");
        }
        // the middle rests on top of the ball
        let mid = s.p[s.point_index(6, 6).unwrap()];
        assert!(mid.y > 1.5, "{mid}");
    }

    #[test]
    fn ropes_hang_and_cloth_tears() {
        let mut r = Soft::rope(1, 1, V3::zeros(), V3::new(4.0, 0.0, 0.0), 16, 0.05, 6.0);
        r.pin(0, 0);
        r.pin(16, 0);
        run(&mut r, 600, &[]);
        let mid = r.p[8];
        assert!(mid.y < -1.0, "a slack rope sags: {mid}");
        assert_eq!(r.point_index(-1, 0).unwrap(), 16);
        let (v, t) = r.mesh(&Matrix4::identity());
        assert_eq!((v.len(), t.len()), (17 * 7, 16 * 12));

        let mut c = Soft::cloth(1, 1, 2.0, 2.0, 6, 6);
        c.pin(c.point_index(0, 0).unwrap(), 0);
        c.pin(c.point_index(6, 0).unwrap(), 0);
        c.tear = 1.01;
        c.gravity = V3::new(0.0, -200.0, 0.0);
        run(&mut c, 120, &[]);
        assert!(c.tris.len() < 72, "a heavily pulled cloth tears");
    }
}
