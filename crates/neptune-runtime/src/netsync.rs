//! (ext) Entity sync for multiplayer games: `NetTemplate`, `NetSpawn`, ...
//!
//! A computer owns the entities it spawns and moves them as usual. Each
//! `RenderWorld` their state goes out a set number of times a second, and
//! every other computer shows a copy, drawn slightly in the past so it can
//! glide between the states it has received.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use fyrox::core::algebra::{Quaternion, UnitQuaternion, Vector3};
use neptune_vm::netgame::{Event, Pose, Spawn, SyncIn};

use crate::character::{self, Slot};
use crate::charcontrol::Settings;
use crate::netchar::{HostSim, Predictor};
use crate::transform::quat;
use crate::world::Host3D;

const DEFAULT_RATE: f32 = 20.0;
/// How long the host remembers where things were, in milliseconds.
const HISTORY_MS: u32 = neptune_vm::netgame::MAX_REWIND_MS + 200;
/// How far past the newest state a copy keeps going, in seconds.
const MAX_EXTRAPOLATE: f64 = 0.25;
/// A move this long between two states is a teleport, not a journey.
const SNAP_DISTANCE: f32 = 50.0;
const MAX_SAMPLES: usize = 32;

/// The states received for one entity, and how to replay them smoothly.
#[derive(Debug, Default)]
struct Track {
    samples: VecDeque<(f64, Pose)>,
    /// Local time minus sender's time, smoothed.
    offset: Option<f64>,
}

impl Track {
    fn push(&mut self, sent: f64, now: f64, pose: Pose) {
        let off = now - sent;
        self.offset = Some(match self.offset {
            Some(o) if (off - o).abs() < 1.0 => o + (off - o) * 0.05,
            _ => off,
        });
        // datagrams can arrive late or out of order: only newer states count
        if self.samples.back().is_some_and(|(t, _)| sent <= *t) {
            return;
        }
        self.samples.push_back((sent, pose));
        if self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    /// Where the entity should appear at local time `now`.
    fn sample(&self, now: f64, delay: f64) -> Option<Pose> {
        let t = now - self.offset? - delay;
        let (first, last) = (self.samples.front()?, self.samples.back()?);
        if t <= first.0 {
            return Some(first.1);
        }
        if t >= last.0 {
            let Some(before) = self.samples.iter().rev().nth(1) else { return Some(last.1) };
            let ahead = (t - last.0).min(MAX_EXTRAPOLATE);
            let span = (last.0 - before.0).max(1e-3);
            if distance(&before.1, &last.1) > SNAP_DISTANCE {
                return Some(last.1);
            }
            let mut p = last.1;
            for (v, (n, b)) in p.iter_mut().zip(last.1.iter().zip(&before.1)).take(3) {
                *v += (n - b) * (ahead / span) as f32;
            }
            return Some(p);
        }
        let after = self.samples.iter().position(|(s, _)| *s > t)?;
        let (a, b) = (&self.samples[after - 1], &self.samples[after]);
        Some(blend(&a.1, &b.1, ((t - a.0) / (b.0 - a.0)) as f32))
    }
}

/// Where an entity was lately, for judging shots.
#[derive(Debug, Default)]
struct History(VecDeque<(u32, Pose)>);

impl History {
    fn record(&mut self, ms: u32, pose: Pose) {
        self.0.push_back((ms, pose));
        while self.0.front().is_some_and(|(t, _)| ms.saturating_sub(*t) > HISTORY_MS) {
            self.0.pop_front();
        }
    }

    /// Where it was at `ms`, between the two nearest records.
    fn at(&self, ms: u32) -> Option<Pose> {
        let (first, last) = (self.0.front()?, self.0.back()?);
        if ms <= first.0 {
            return Some(first.1);
        }
        if ms >= last.0 {
            return Some(last.1);
        }
        let after = self.0.iter().position(|(t, _)| *t > ms)?;
        let (a, b) = (&self.0[after - 1], &self.0[after]);
        Some(blend(&a.1, &b.1, (ms - a.0) as f32 / (b.0 - a.0).max(1) as f32))
    }
}

fn distance(a: &Pose, b: &Pose) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn blend(a: &Pose, b: &Pose, t: f32) -> Pose {
    if distance(a, b) > SNAP_DISTANCE {
        return *b;
    }
    let lerp = |i: usize| a[i] + (b[i] - a[i]) * t;
    // the short way round
    let dot: f32 = (3..7).map(|i| a[i] * b[i]).sum();
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let q: [f32; 4] = std::array::from_fn(|i| a[3 + i] + (b[3 + i] * sign - a[3 + i]) * t);
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    [lerp(0), lerp(1), lerp(2), q[0] / n, q[1] / n, q[2] / n, q[3] / n, lerp(7), lerp(8), lerp(9)]
}

#[derive(Debug)]
struct Remote {
    entity: i32,
    track: Track,
    /// Its character controller, if it has one (0 if not): solid here, too.
    character: i32,
    /// On the host: this copy is the real one, moved by its owner's inputs.
    sim: Option<HostSim>,
}

#[derive(Debug)]
pub struct NetSync {
    active: bool,
    templates: HashMap<String, i32>,
    /// The controllers templates had: radius, height and settings.
    char_templates: HashMap<String, (f32, f32, Settings)>,
    /// A controller by entity, for `NetCharacter`.
    chars: HashMap<i32, i32>,
    /// Characters here that the host checks: network id -> controller.
    predicted: HashMap<i32, i32>,
    /// On the host: where each networked entity was lately.
    history: HashMap<i32, History>,
    /// Entities spawned here: entity -> network id.
    mine: HashMap<i32, i32>,
    /// Copies of other computers' entities, by network id.
    remote: HashMap<i32, Remote>,
    /// Every networked entity: entity -> (network id, owner player).
    info: HashMap<i32, (i32, i32)>,
    /// Spawns whose template this program has not registered yet.
    waiting: Vec<Spawn>,
    rate: f32,
    start: Instant,
    last_sent: f64,
    last_update: f64,
}

impl Default for NetSync {
    fn default() -> Self {
        NetSync {
            active: false,
            templates: HashMap::new(),
            char_templates: HashMap::new(),
            chars: HashMap::new(),
            predicted: HashMap::new(),
            history: HashMap::new(),
            mine: HashMap::new(),
            remote: HashMap::new(),
            info: HashMap::new(),
            waiting: Vec::new(),
            rate: DEFAULT_RATE,
            start: Instant::now(),
            last_sent: f64::NEG_INFINITY,
            last_update: 0.0,
        }
    }
}

fn pose_of(h: &Host3D, e: i32) -> Result<Pose, String> {
    let w = h.world_of(e)?;
    let (p, q, s) = (w.position(), quat(&w.rot), w.scale);
    Ok([p.x, p.y, p.z, q.i, q.j, q.k, q.w, s.x, s.y, s.z])
}

fn set_pose(h: &mut Host3D, e: i32, p: &Pose) -> Result<(), String> {
    let q = UnitQuaternion::from_quaternion(Quaternion::new(p[6], p[3], p[4], p[5]));
    let rot = q.to_rotation_matrix().into_inner();
    h.set_xf(e, |xf| {
        xf.pos = Vector3::new(p[0], p[1], p[2]);
        xf.rot = rot;
        xf.scale = Vector3::new(p[7], p[8], p[9]);
    })
}

fn show(h: &mut Host3D, e: i32) -> Result<(), String> {
    h.ent_mut(e)?.hidden = false;
    let n = h.ent(e)?.node;
    h.graph()?[n].set_visibility(true);
    Ok(())
}

fn event(h: &Host3D, e: Event) {
    h.world.net.borrow().with((), |s| s.push_event(e));
}

/// `NetTemplate`: what copies of `name` look like, on every computer.
pub fn template(h: &mut Host3D, name: &str, entity: i32) -> Result<(), String> {
    // a controller on the template is the pattern for the controllers of its copies
    if let Some(c) = character::of_entity(h, entity) {
        let (radius, height) = character::shape_of(h, c)?;
        let settings = character::slot(h, c)?.c.s.clone();
        h.world.netsync.char_templates.insert(name.to_string(), (radius, height, settings));
        character::free(h, c)?;
    }
    h.ent_mut(entity)?.hidden = true;
    let n = h.ent(entity)?.node;
    h.graph()?[n].set_visibility(false);
    h.world.netsync.templates.insert(name.to_string(), entity);
    h.world.netsync.active = true;
    h.world.net.borrow().with((), |s| s.enable_sync());
    Ok(())
}

/// `NetSpawn`: a new entity that this computer owns.
pub fn spawn(h: &mut Host3D, name: &str, pos: Vector3<f32>) -> Result<i32, String> {
    let &tpl = h.world.netsync.templates.get(name).ok_or("Net template does not exist")?;
    if h.world.net.borrow().with(0, |s| s.me) == 0 {
        return Err("no net game: use NetPlay first".into());
    }
    let e = h.copy_entity(tpl, 0)?;
    show(h, e)?;
    h.set_xf(e, |xf| xf.pos = pos)?;
    let pose = pose_of(h, e)?;
    let net = h.world.net.clone();
    let Some(id) = net.borrow().with(None, |s| s.spawn(name, pose).map(|id| (id, s.me))) else {
        h.free_entity(e)?;
        return Err("no net game: use NetPlay first".into());
    };
    h.world.netsync.mine.insert(e, id.0);
    h.world.netsync.info.insert(e, id);
    h.world.net.borrow().with((), |s| s.enable_sync());
    if let Some(c) = add_character(h, name, e)? {
        // the host's own characters are the real ones already
        if !net.borrow().with(false, |s| s.is_host()) {
            character::slot_mut(h, c)?.predict = Some(Box::new(Predictor::new(id.0)));
            h.world.netsync.predicted.insert(id.0, c);
        }
    }
    Ok(e)
}

/// Gives a copy of a template the controller the template had.
fn add_character(h: &mut Host3D, template: &str, e: i32) -> Result<Option<i32>, String> {
    let Some((radius, height, settings)) = h.world.netsync.char_templates.get(template).cloned() else { return Ok(None) };
    let c = character::create_3d(h, e, radius, height)?;
    character::slot_mut(h, c)?.c.s = settings;
    h.world.netsync.chars.insert(e, c);
    Ok(Some(c))
}

/// `NetCharacter`: the controller of a networked entity, or 0.
pub fn character_of(h: &Host3D, e: i32) -> i32 {
    h.world.netsync.chars.get(&e).copied().unwrap_or(0)
}

/// `NetCorrections`: how many times the host has corrected this character.
pub fn corrections(h: &Host3D, c: i32) -> Result<i32, String> {
    Ok(character::slot(h, c)?.predict.as_ref().map_or(0, |p| p.corrections))
}

/// Sends a player's buttons after each `UpdateChar`.
pub fn send_inputs(h: &mut Host3D, slot: &Slot) -> Result<(), String> {
    let (Some(p), character::Link::Entity { entity, .. }) = (slot.predict.as_ref(), slot.link) else { return Ok(()) };
    let q = quat(&h.world_of(entity)?.rot);
    let inputs = p.unconfirmed();
    h.world.net.borrow().with((), |s| s.send_inputs(p.net_id, [q.i, q.j, q.k, q.w], &inputs));
    Ok(())
}

pub fn is_mine(h: &Host3D, e: i32) -> bool {
    h.world.netsync.mine.contains_key(&e)
}

/// The player an entity belongs to, or 0 if it is not networked.
pub fn owner(h: &Host3D, e: i32) -> i32 {
    h.world.netsync.info.get(&e).map_or(0, |i| i.1)
}

/// `NetRate`.
pub fn set_rate(h: &mut Host3D, hz: f32) {
    h.world.netsync.rate = hz.clamp(1.0, 60.0);
}

/// Called when an entity is freed: other computers lose their copies of it.
pub fn forget_entity(h: &mut Host3D, e: i32) {
    let sync = &mut h.world.netsync;
    sync.chars.remove(&e);
    sync.history.remove(&e);
    if let Some((id, _)) = sync.info.remove(&e) {
        sync.predicted.remove(&id);
        if sync.mine.remove(&e).is_some() {
            h.world.net.borrow().with((), |s| s.despawn(id));
        } else {
            sync.remote.remove(&id);
        }
    }
    sync.templates.retain(|_, t| *t != e);
}

/// Makes the copy of another computer's entity. False if its template is
/// not registered yet.
fn add_remote(h: &mut Host3D, s: &Spawn) -> Result<bool, String> {
    let Some(&tpl) = h.world.netsync.templates.get(&s.template) else { return Ok(false) };
    if h.world.netsync.remote.contains_key(&s.id) {
        return Ok(true);
    }
    let e = h.copy_entity(tpl, 0)?;
    show(h, e)?;
    set_pose(h, e, &s.pose)?;
    let character = add_character(h, &s.template, e)?.unwrap_or(0);
    let hosting = h.world.net.borrow().with(false, |n| n.is_host());
    let sim = if character != 0 && hosting {
        h.world.net.borrow().with((), |n| n.mark_host_moved(s.id));
        Some(HostSim::new(character::slot(h, character)?.c.s.clone()))
    } else {
        None
    };
    h.world.netsync.remote.insert(s.id, Remote { entity: e, track: Track::default(), character, sim });
    h.world.netsync.info.insert(e, (s.id, s.owner));
    event(h, Event { name: "spawn".into(), from: s.owner, data: s.template.clone(), entity: e, ..Default::default() });
    Ok(true)
}

fn remove_remote(h: &mut Host3D, id: i32) -> Result<(), String> {
    h.world.netsync.waiting.retain(|s| s.id != id);
    if let Some(r) = h.world.netsync.remote.remove(&id) {
        let owner = owner(h, r.entity);
        h.world.netsync.info.remove(&r.entity);
        h.free_entity(r.entity)?;
        event(h, Event { name: "despawn".into(), from: owner, entity: r.entity, ..Default::default() });
    }
    Ok(())
}

/// `StopNetGame`: copies of other computers' entities go, and this
/// computer's stay as ordinary entities.
fn end_of_game(h: &mut Host3D) -> Result<(), String> {
    let ids: Vec<i32> = h.world.netsync.remote.keys().copied().collect();
    for id in ids {
        remove_remote(h, id)?;
    }
    let predicted: Vec<i32> = h.world.netsync.predicted.drain().map(|(_, c)| c).collect();
    for c in predicted {
        if let Ok(slot) = character::slot_mut(h, c) {
            slot.predict = None;
        }
    }
    h.world.netsync.history.clear();
    h.world.netsync.mine.clear();
    h.world.netsync.info.clear();
    h.world.netsync.waiting.clear();
    Ok(())
}

/// Runs once per `RenderWorld`.
pub fn update(h: &mut Host3D) -> Result<(), String> {
    if !h.world.netsync.active {
        return Ok(());
    }
    let net = h.world.net.clone();
    if net.borrow().game.is_none() {
        return end_of_game(h);
    }
    let incoming = net.borrow().with(Vec::new(), |s| s.take_sync());
    let now = h.world.netsync.start.elapsed().as_secs_f64();

    let waiting = std::mem::take(&mut h.world.netsync.waiting);
    for s in waiting {
        if !add_remote(h, &s)? {
            h.world.netsync.waiting.push(s);
        }
    }
    for item in incoming {
        match item {
            SyncIn::Spawn(s) => {
                if !add_remote(h, &s)? {
                    h.world.netsync.waiting.push(s);
                }
            }
            SyncIn::Despawn(id) => remove_remote(h, id)?,
            SyncIn::States { time_ms, entries } => {
                for (id, pose) in entries {
                    if let Some(r) = h.world.netsync.remote.get_mut(&id) {
                        r.track.push(time_ms as f64 / 1000.0, now, pose);
                    }
                }
            }
            SyncIn::Inputs { id, look, inputs } => {
                if let Some(sim) = h.world.netsync.remote.get_mut(&id).and_then(|r| r.sim.as_mut()) {
                    sim.receive(&inputs);
                    sim.look = Some(look);
                }
            }
            SyncIn::CharState { id, ack, state } => {
                if let Some(&c) = h.world.netsync.predicted.get(&id) {
                    character::with_world(h, c, |slot, w| {
                        if let Some(p) = slot.predict.as_mut() {
                            p.reconcile(&mut slot.c, w, ack, &state);
                        }
                    })?;
                }
            }
            SyncIn::CharSettings { id, values } => {
                if let Some(&c) = h.world.netsync.predicted.get(&id) {
                    character::slot_mut(h, c)?.c.s.set_values(&values);
                }
            }
        }
    }

    run_host_sims(h, now)?;

    let delay = 2.0 / h.world.netsync.rate as f64;
    let poses: Vec<(i32, i32, bool, Pose)> = h.world.netsync.remote.values().filter_map(|r| r.track.sample(now, delay).map(|p| (r.entity, r.character, r.sim.is_some(), p))).collect();
    for (e, c, hosted, p) in poses {
        set_pose(h, e, &p)?;
        if c != 0 && !hosted {
            // solid here too, or this computer's characters would walk through it
            character::teleport(h, c, Vector3::new(p[0], p[1], p[2]))?;
        }
    }

    record_history(h)?;

    if now - h.world.netsync.last_sent >= 1.0 / h.world.netsync.rate as f64 {
        h.world.netsync.last_sent = now;
        // freeing a parent frees its children without saying so
        let gone: Vec<i32> = h.world.netsync.mine.keys().copied().filter(|e| h.ent(*e).is_err()).collect();
        for e in gone {
            forget_entity(h, e);
        }
        let sync = &h.world.netsync;
        // what the host checks, the host reports; and it reports the copies it moves
        let mut moving: Vec<(i32, i32, i32)> = sync.mine.iter().filter(|(_, id)| !sync.predicted.contains_key(id)).map(|(e, id)| (*e, *id, 0)).collect();
        moving.extend(sync.remote.iter().filter(|(_, r)| r.sim.is_some()).map(|(id, r)| (r.entity, *id, r.character)));
        if !moving.is_empty() {
            let entries = moving.iter().map(|(e, id, c)| Ok((*id, if *c != 0 { real_pose(h, *e, *c)? } else { pose_of(h, *e)? }))).collect::<Result<Vec<_>, String>>()?;
            net.borrow().with((), |s| s.send_states((now * 1000.0) as u32, &entries));
        }
        answer_players(h)?;
    }
    Ok(())
}

/// The host remembers where everything really was, not where it is drawn:
/// that is what a player was sent, and so what the player saw.
fn record_history(h: &mut Host3D) -> Result<(), String> {
    let net = h.world.net.clone();
    let Some(ms) = net.borrow().with(None, |s| s.is_host().then(|| s.clock_ms())) else { return Ok(()) };
    let mut poses: Vec<(i32, Pose)> = Vec::new();
    for e in h.world.netsync.mine.keys().copied().collect::<Vec<_>>() {
        if h.ent(e).is_ok() {
            poses.push((e, pose_of(h, e)?));
        }
    }
    for r in h.world.netsync.remote.values() {
        if r.sim.is_some() {
            poses.push((r.entity, real_pose(h, r.entity, r.character)?));
        } else if let Some((_, p)) = r.track.samples.back() {
            poses.push((r.entity, *p));
        }
    }
    for (e, p) in poses {
        h.world.netsync.history.entry(e).or_default().record(ms, p);
    }
    Ok(())
}

/// `NetFire`: asks the host whether a shot along `source`'s forward hit.
pub fn fire(h: &mut Host3D, name: &str, source: i32, range: f32, data: &str) -> Result<bool, String> {
    let w = h.world_of(source)?;
    let (p, d) = (w.position(), w.rot * Vector3::z() * range);
    let delay = (2000.0 / h.world.netsync.rate) as u32;
    Ok(h.world.net.borrow().with(false, |s| s.fire(name, data, [p.x, p.y, p.z], [d.x, d.y, d.z], delay)))
}

/// `NetHit`: on the host, what the shot from the last `NetRecv` hit. Puts
/// everything back where the shooter saw it, picks as `LinePick` does, and
/// returns everything to the present.
pub fn hit(h: &mut Host3D, radius: f32) -> Result<i32, String> {
    let net = h.world.net.clone();
    let (shot, from) = {
        let n = net.borrow();
        (n.event.shot.filter(|_| n.with(false, |s| s.is_host())), n.event.from)
    };
    let Some(shot) = shot else { return Ok(0) };
    let exclude: Vec<i32> = h.world.netsync.info.iter().filter(|(_, i)| i.1 == from).map(|(e, _)| *e).collect();
    let rewound: Vec<(i32, Pose)> = h.world.netsync.history.iter().filter(|(e, _)| !exclude.contains(e)).filter_map(|(e, hist)| hist.at(shot.view_ms).map(|p| (*e, p))).collect();
    let mut saved = Vec::with_capacity(rewound.len());
    for (e, pose) in rewound {
        // a child moves with its parent, so only whole entities go back
        if h.ent(e)?.parent == 0 {
            saved.push((e, h.live_xf(h.ent(e)?)));
            set_pose(h, e, &pose)?;
        }
    }
    h.world.pick_gen += 1;
    let [ox, oy, oz] = shot.origin;
    let [lx, ly, lz] = shot.line;
    let result = crate::commands::pick_line(h, Vector3::new(ox, oy, oz), Vector3::new(lx, ly, lz), radius, &exclude, false);
    for (e, xf) in saved {
        h.set_xf(e, |x| *x = xf)?;
    }
    h.world.pick_gen += 1;
    let found = result?;
    h.world.picked = found;
    Ok(found.map_or(0, |p| p.entity))
}

/// The host runs the buttons players have sent.
fn run_host_sims(h: &mut Host3D, now: f64) -> Result<(), String> {
    let seconds = (now - h.world.netsync.last_update) as f32;
    h.world.netsync.last_update = now;
    let ids: Vec<i32> = h.world.netsync.remote.iter().filter(|(_, r)| r.sim.is_some()).map(|(id, _)| *id).collect();
    for id in ids {
        let Some(r) = h.world.netsync.remote.get_mut(&id) else { continue };
        let (c, e) = (r.character, r.entity);
        let Some(mut sim) = r.sim.take() else { continue };
        sim.refill(seconds);
        let ran = sim.has_work();
        let result = if ran { character::with_world(h, c, |slot, w| sim.run(&mut slot.c, w)) } else { Ok(()) };
        // which way the model faces is only for show
        let look = sim.look.take();
        if let Some(r) = h.world.netsync.remote.get_mut(&id) {
            r.sim = Some(sim);
        }
        result?;
        if let Some(q) = look.filter(|q| q.iter().all(|v| v.is_finite())) {
            let rot = UnitQuaternion::from_quaternion(Quaternion::new(q[3], q[0], q[1], q[2])).to_rotation_matrix().into_inner();
            h.set_xf(e, |xf| xf.rot = rot)?;
        }
        // inputs arrive in bursts: the host shows this copy smoothly as well,
        // while its collision shape stays exactly where the rules put it
        let pose = real_pose(h, e, c)?;
        if let Some(r) = h.world.netsync.remote.get_mut(&id) {
            if let Some(clock) = r.sim.as_ref().map(|s| s.clock).filter(|_| ran) {
                r.track.push(clock, now, pose);
            }
        }
    }
    Ok(())
}

/// An entity's pose, but with the position its character controller really has.
fn real_pose(h: &Host3D, e: i32, c: i32) -> Result<Pose, String> {
    let mut pose = pose_of(h, e)?;
    let at = character::slot(h, c)?.c.pos;
    pose[..3].copy_from_slice(&[at.x, at.y, at.z]);
    Ok(pose)
}

/// The host tells each player what became of their inputs.
fn answer_players(h: &mut Host3D) -> Result<(), String> {
    let ids: Vec<i32> = h.world.netsync.remote.iter().filter(|(_, r)| r.sim.as_ref().is_some_and(|s| s.ack != 0)).map(|(id, _)| *id).collect();
    for id in ids {
        let Some(r) = h.world.netsync.remote.get(&id) else { continue };
        let (c, owner) = (r.character, owner(h, r.entity));
        let Some(ack) = r.sim.as_ref().map(|s| s.ack) else { continue };
        let slot = character::slot(h, c)?;
        let (state, settings) = (slot.c.snapshot(), slot.c.s.clone());
        h.world.net.borrow().with((), |n| n.send_char_state(id, owner, ack, &state));
        if let Some(sim) = h.world.netsync.remote.get_mut(&id).and_then(|r| r.sim.as_mut()) {
            if sim.told != settings {
                sim.told = settings.clone();
                h.world.net.borrow().with((), |n| n.send_char_settings(id, owner, &settings.values()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32) -> Pose {
        [x, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// States sent every 50 ms, each arriving 30 ms after it was sent.
    fn track(n: usize) -> Track {
        let mut t = Track::default();
        for i in 0..n {
            let sent = i as f64 * 0.05;
            t.push(sent, sent + 0.03, at(i as f32));
        }
        t
    }

    #[test]
    fn history_answers_for_any_recent_moment() {
        let mut h = History::default();
        for i in 0..10 {
            h.record(100 + i * 10, at(i as f32));
        }
        assert!(close(h.at(125).unwrap()[0], 2.5));
        assert_eq!(h.at(1).unwrap()[0], 0.0, "older than it knows");
        assert_eq!(h.at(9999).unwrap()[0], 9.0, "newer than it knows");
        assert!(History::default().at(5).is_none());
    }

    #[test]
    fn history_forgets_what_nobody_can_claim_to_have_seen() {
        let mut h = History::default();
        h.record(100, at(1.0));
        h.record(100 + HISTORY_MS + 50, at(2.0));
        assert_eq!(h.0.len(), 1);
    }

    #[test]
    fn glides_between_states() {
        let t = track(10);
        // half way between the states sent at 0.10 and 0.15
        let p = t.sample(0.125 + 0.03 + 0.1, 0.1).unwrap();
        assert!(close(p[0], 2.5), "{}", p[0]);
    }

    #[test]
    fn nothing_before_the_first_state() {
        assert!(Track::default().sample(1.0, 0.1).is_none());
    }

    #[test]
    fn late_states_are_ignored() {
        let mut t = track(5);
        t.push(0.01, 0.5, at(99.0));
        assert_eq!(t.samples.len(), 5);
        assert!(t.samples.iter().all(|(_, p)| p[0] != 99.0));
    }

    #[test]
    fn keeps_going_briefly_when_states_stop() {
        let t = track(5); // last sent at 0.2, position 4, moving 20 units/s
        let p = t.sample(0.2 + 0.03 + 0.1 + 0.1, 0.1).unwrap();
        assert!(close(p[0], 6.0), "{}", p[0]);
        // and then waits
        let p = t.sample(100.0, 0.1).unwrap();
        assert!(close(p[0], 4.0 + 20.0 * MAX_EXTRAPOLATE as f32), "{}", p[0]);
    }

    #[test]
    fn a_teleport_is_not_a_journey() {
        let p = blend(&at(0.0), &at(500.0), 0.5);
        assert_eq!(p[0], 500.0);
    }

    #[test]
    fn rotation_goes_the_short_way() {
        let a: Pose = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        // the same rotation written with every sign flipped
        let b: Pose = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 1.0, 1.0, 1.0];
        let p = blend(&a, &b, 0.5);
        assert!(close(p[6], 1.0), "{}", p[6]);
    }
}
