//! Multiplayer games: Blitz3D's net commands (`HostNetGame`, `JoinNetGame`,
//! `CreateNetPlayer`, `SendNetMsg`, `RecvNetMsg`, ...) and the simpler (ext)
//! commands built on the same session (`NetPlay`, `NetSend`, `NetSet`, ...).
//!
//! Blitz3D built these on DirectPlay. Here [`Session`] holds the rules (who
//! is playing, who may say what, who hears it) and knows nothing about
//! sockets, so it can be tested alone. `netlink.rs` carries its traffic over
//! iroh, on its own thread, so the game never waits for the network and the
//! host keeps relaying during loading screens and menus.
//!
//! * every message goes through the host (star topology);
//! * reliable traffic is one ordered stream per computer; `reliable=0`
//!   messages and entity positions are datagrams, which may be lost;
//! * system messages: 100 a player joined (`NetMsgFrom` is the new player,
//!   `NetMsgData` its name), 101 a player left, 200 the session was lost.
//!   Program messages use types 1-99.
//!
//! Wire format: frames of `u32 length` + `u8 kind` + payload; strings are
//! `u32 length` + UTF-8. A datagram is `u8 kind` + payload.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::value::Value;
use crate::vm::NativeCtx;

/// Bumped when the wire format changes, so old and new games refuse politely.
pub const PROTOCOL: i32 = 2;
pub const DISCOVERY_PORT: u16 = 47701;
pub const MSG_JOINED: i32 = 100;
pub const MSG_LEFT: i32 = 101;
pub const MSG_SESSION_LOST: i32 = 200;

/// Identifies a connected computer to a [`Link`]. Clients only have [`HOST`].
pub type PeerId = u32;
pub const HOST: PeerId = 0;

const F_CREATE: u8 = 1; // client -> host: request id, name
const F_CREATED: u8 = 2; // host -> client: request id, player id, name
const F_JOINED: u8 = 3; // host -> client: player id, is host's, name
const F_LEFT: u8 = 4; // host -> client: player id
const F_MSG: u8 = 5; // either way: type, from, to, data
const F_DELETE: u8 = 6; // client -> host: player id
const F_HELLO: u8 = 7; // client -> host: protocol, game name
const F_WELCOME: u8 = 8; // host -> client: slot, invite
const F_REJECT: u8 = 9; // host -> client: reason
const F_EVENT: u8 = 10; // either way: from, to, name, data
const F_SET: u8 = 11; // client -> host: key, value
const F_VALUE: u8 = 12; // host -> client: key, value
const F_SPAWN: u8 = 13; // either way: id, owner, pose, template
const F_DESPAWN: u8 = 14; // either way: id
/// A datagram too large for the path, sent over the reliable stream instead.
pub const F_BIG_DATAGRAM: u8 = 15;
const F_CHAR_SET: u8 = 16; // host -> client: id, then character settings
const F_SHOT: u8 = 17; // client -> host: from, view time, origin, line, name, data

/// Furthest back the host looks for a shot, in milliseconds. A slower
/// connection than this has to lead its targets, and it limits what a
/// modified game can claim to have seen.
pub const MAX_REWIND_MS: u32 = 500;
const PING_EVERY_MS: u32 = 250;

const D_MSG: u8 = 1; // same body as F_MSG
const D_SYNC: u8 = 2; // sender time (ms), then (id, pose) entries
const D_INPUT: u8 = 3; // client -> host: id, look, then (sequence, input) entries
const D_CHAR: u8 = 4; // host -> client: id, last input run, character state
const D_PING: u8 = 5; // client -> host: client's clock
const D_PONG: u8 = 6; // host -> client: client's clock, host's clock

/// Entity states per datagram, so a packet fits every path's MTU.
const SYNC_PER_DATAGRAM: usize = 24;
/// A network entity id is the owner computer's slot, then this many bits of counter.
const SLOT_SHIFT: u32 = 19;
const MAX_SLOT: i32 = 4095;
/// Unread messages kept before the oldest are dropped.
const QUEUE_LIMIT: usize = 16_384;
const NAME_LIMIT: usize = 64;
const VALUE_LIMIT: usize = 64 * 1024;
const MAX_FRAME: usize = 16 << 20;

/// A received message, as `NetMsgType`/`From`/`To`/`Data` report it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Msg {
    pub kind: i32,
    pub from: i32,
    pub to: i32,
    pub data: String,
}

/// A `NetFire`: what was aimed along, and when (on the host's clock) the
/// world was as the shooter saw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub view_ms: u32,
    pub origin: [f32; 3],
    pub line: [f32; 3],
}

/// A received `NetSend` event, or "join", "leave", "spawn", "despawn", "lost".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Event {
    pub name: String,
    pub from: i32,
    pub to: i32,
    pub data: String,
    pub entity: i32,
    pub shot: Option<Shot>,
}

/// Position xyz, rotation quaternion xyzw and scale xyz of an entity.
pub type Pose = [f32; 10];

#[derive(Clone, Debug, PartialEq)]
pub struct Spawn {
    pub id: i32,
    pub owner: i32,
    pub template: String,
    pub pose: Pose,
}

/// One `UpdateChar` from a player: sequence number, then its numbers.
pub type CharInput = (u32, [f32; 5]);

/// What the entity sync layer has to apply.
#[derive(Clone, Debug, PartialEq)]
pub enum SyncIn {
    Spawn(Spawn),
    Despawn(i32),
    States { time_ms: u32, entries: Vec<(i32, Pose)> },
    /// A player's buttons, on the host.
    Inputs { id: i32, look: [f32; 4], inputs: Vec<CharInput> },
    /// What the host says became of a player's character.
    CharState { id: i32, ack: u32, state: Vec<f32> },
    CharSettings { id: i32, values: Vec<f32> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Starting,
    Hosting,
    Joined,
    Failed(String),
    Lost,
}

/// Carries a session's traffic. No method may wait for the network.
pub trait Link: Send {
    /// Sends a whole frame, reliably and in order.
    fn send(&mut self, peer: PeerId, frame: Vec<u8>);
    /// Sends a datagram, which may be lost.
    fn send_datagram(&mut self, peer: PeerId, data: Vec<u8>);
    /// Delivers what is already queued and then disconnects the computer.
    fn kick(&mut self, peer: PeerId);
}

// ---------------------------------------------------------------- frames

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

pub fn frame(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(body.len() + 5);
    f.extend_from_slice(&(body.len() as u32 + 1).to_le_bytes());
    f.push(kind);
    f.extend_from_slice(body);
    f
}

fn ints(v: &[i32]) -> Vec<u8> {
    v.iter().flat_map(|i| i.to_le_bytes()).collect()
}

fn put_pose(out: &mut Vec<u8>, p: &Pose) {
    for v in p {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn msg_body(m: &Msg) -> Vec<u8> {
    let mut b = ints(&[m.kind, m.from, m.to]);
    put_str(&mut b, &m.data);
    b
}

fn event_frame(e: &Event) -> Vec<u8> {
    let mut b = ints(&[e.from, e.to]);
    put_str(&mut b, &e.name);
    put_str(&mut b, &e.data);
    frame(F_EVENT, &b)
}

fn spawn_frame(s: &Spawn) -> Vec<u8> {
    let mut b = ints(&[s.id, s.owner]);
    put_pose(&mut b, &s.pose);
    put_str(&mut b, &s.template);
    frame(F_SPAWN, &b)
}

fn value_frame(kind: u8, key: &str, value: &str) -> Vec<u8> {
    let mut b = Vec::new();
    put_str(&mut b, key);
    put_str(&mut b, value);
    frame(kind, &b)
}

fn joined_frame(id: i32, is_host: bool, name: &str) -> Vec<u8> {
    let mut b = ints(&[id, is_host as i32]);
    put_str(&mut b, name);
    frame(F_JOINED, &b)
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn int(&mut self) -> i32 {
        let v = self.0.get(..4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()));
        self.0 = self.0.get(4..).unwrap_or(&[]);
        v
    }
    fn float(&mut self) -> f32 {
        f32::from_bits(self.int() as u32)
    }
    fn string(&mut self) -> String {
        let n = self.int().max(0) as usize;
        let s = String::from_utf8_lossy(self.0.get(..n).unwrap_or(&[])).into_owned();
        self.0 = self.0.get(n..).unwrap_or(&[]);
        s
    }
    fn pose(&mut self) -> Pose {
        std::array::from_fn(|_| self.float())
    }
    fn msg(&mut self) -> Msg {
        Msg { kind: self.int(), from: self.int(), to: self.int(), data: self.string() }
    }
}

/// Collects a byte stream into whole frames.
#[derive(Default)]
pub struct Framer {
    buf: Vec<u8>,
}

impl Framer {
    /// Adds received bytes and returns complete frames, or `None` if the
    /// stream is not valid (the connection should be dropped).
    pub fn push(&mut self, data: &[u8]) -> Option<Vec<(u8, Vec<u8>)>> {
        self.buf.extend_from_slice(data);
        let mut frames = Vec::new();
        let mut at = 0;
        while self.buf.len() >= at + 4 {
            let len = u32::from_le_bytes(self.buf[at..at + 4].try_into().unwrap()) as usize;
            if len == 0 || len > MAX_FRAME {
                return None;
            }
            if self.buf.len() < at + 4 + len {
                break;
            }
            frames.push((self.buf[at + 4], self.buf[at + 5..at + 4 + len].to_vec()));
            at += 4 + len;
        }
        self.buf.drain(..at);
        Some(frames)
    }
}

fn limit<T>(q: &mut VecDeque<T>) {
    if q.len() >= QUEUE_LIMIT {
        q.pop_front();
    }
}

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

// ---------------------------------------------------------------- session

struct Client {
    greeted: bool,
    slot: i32,
    players: Vec<i32>,
}

enum Incoming {
    Frame(PeerId, u8, Vec<u8>),
    Datagram(PeerId, Vec<u8>),
}

/// `NetSimulate`: pretends the network is worse than it is.
#[derive(Default)]
struct Sim {
    lag: Duration,
    loss_percent: u32,
    queue: VecDeque<(Instant, Incoming)>,
    rng: u32,
}

impl Sim {
    fn lose(&mut self) -> bool {
        // xorshift32
        let mut x = self.rng.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x % 100 < self.loss_percent
    }
}

/// One computer's view of a game.
pub struct Session {
    pub name: String,
    pub status: Status,
    /// Why the session failed or was lost, for `NetError`.
    pub error: String,
    /// `NetInvite`: what others can paste to join this game.
    pub invite: String,
    /// `NetPing`: round trip to the host in milliseconds (0 on the host).
    pub ping_ms: i32,
    /// `NetMe`: the first player created on this computer.
    pub me: i32,
    /// `NetHostPlayer`: the first player created on the host.
    pub host_player: i32,
    link: Box<dyn Link>,
    hosting: bool,
    slot: i32,
    next_slot: i32,
    next_entity: i32,
    clients: HashMap<PeerId, Client>,
    players: BTreeMap<i32, String>,
    local: Vec<i32>,
    next_id: i32,
    next_request: i32,
    created: HashMap<i32, i32>,
    auto_player: Option<String>,
    inbox: VecDeque<Msg>,
    events: VecDeque<Event>,
    values: HashMap<String, String>,
    spawns: BTreeMap<i32, Spawn>,
    sync_on: bool,
    sync_in: VecDeque<SyncIn>,
    /// Entities the host moves for their owners, who may not say where they are.
    host_moved: HashSet<i32>,
    started: Instant,
    next_ping: u32,
    /// Client: this computer's clock minus the host's, as seen by a message
    /// from the host, so it includes the trip here.
    host_offset: Option<f64>,
    sim: Sim,
}

impl Session {
    pub fn new(name: &str, link: Box<dyn Link>) -> Session {
        Session {
            name: name.to_string(),
            status: Status::Starting,
            error: String::new(),
            invite: String::new(),
            ping_ms: 0,
            me: 0,
            host_player: 0,
            link,
            hosting: false,
            slot: 0,
            next_slot: 2,
            next_entity: 1,
            clients: HashMap::new(),
            players: BTreeMap::new(),
            local: Vec::new(),
            next_id: 1,
            next_request: 1,
            created: HashMap::new(),
            auto_player: None,
            inbox: VecDeque::new(),
            events: VecDeque::new(),
            values: HashMap::new(),
            spawns: BTreeMap::new(),
            sync_on: false,
            sync_in: VecDeque::new(),
            host_moved: HashSet::new(),
            started: Instant::now(),
            next_ping: 0,
            host_offset: None,
            sim: Sim { rng: 0x9E37_79B9, ..Default::default() },
        }
    }

    /// `NetPlay` and friends: the player to create once the game is running.
    pub fn set_auto_player(&mut self, name: &str) {
        self.auto_player = Some(name.to_string());
    }

    pub fn is_host(&self) -> bool {
        self.hosting
    }

    pub fn start_host(&mut self, invite: &str) {
        self.hosting = true;
        self.slot = 1;
        self.invite = invite.to_string();
        self.status = Status::Hosting;
        if let Some(name) = self.auto_player.take() {
            self.create_player(&name);
        }
    }

    /// A client's connection to the host is ready: introduces itself.
    pub fn connected(&mut self) {
        let mut b = ints(&[PROTOCOL]);
        put_str(&mut b, &self.name);
        self.link.send(HOST, frame(F_HELLO, &b));
    }

    pub fn fail(&mut self, reason: &str) {
        self.error = reason.to_string();
        self.status = Status::Failed(reason.to_string());
    }

    /// The host side of a new connection; it says hello next.
    pub fn peer_connected(&mut self, peer: PeerId) {
        self.clients.insert(peer, Client { greeted: false, slot: 0, players: Vec::new() });
    }

    pub fn peer_gone(&mut self, peer: PeerId, reason: &str) {
        if self.hosting {
            if let Some(gone) = self.clients.remove(&peer) {
                let ids: Vec<i32> = self.spawns.values().filter(|s| s.id >> SLOT_SHIFT == gone.slot).map(|s| s.id).collect();
                for id in ids {
                    self.remove_spawn(id, None);
                }
                for id in gone.players {
                    self.remove_player(id, None);
                }
            }
        } else if self.status == Status::Joined {
            self.status = Status::Lost;
            self.error = reason.to_string();
            self.inbox.push_back(Msg { kind: MSG_SESSION_LOST, ..Default::default() });
            self.push_event(Event { name: "lost".into(), data: reason.to_string(), ..Default::default() });
        } else if self.status == Status::Starting {
            self.fail(reason);
        }
    }

    pub fn frame_in(&mut self, peer: PeerId, kind: u8, body: Vec<u8>) {
        self.incoming(Incoming::Frame(peer, kind, body));
    }

    pub fn datagram_in(&mut self, peer: PeerId, data: Vec<u8>) {
        if self.sim.loss_percent > 0 && self.sim.lose() {
            return;
        }
        self.incoming(Incoming::Datagram(peer, data));
    }

    fn incoming(&mut self, item: Incoming) {
        if self.sim.lag.is_zero() && self.sim.queue.is_empty() {
            self.handle(item);
        } else {
            self.sim.queue.push_back((Instant::now() + self.sim.lag, item));
        }
    }

    /// Runs often on the network thread: delivers delayed traffic.
    pub fn tick(&mut self) {
        let now = Instant::now();
        while self.sim.queue.front().is_some_and(|(due, _)| *due <= now) {
            if let Some((_, item)) = self.sim.queue.pop_front() {
                self.handle(item);
            }
        }
        if !self.hosting && self.status == Status::Joined && self.clock_ms() >= self.next_ping {
            self.next_ping = self.clock_ms() + PING_EVERY_MS;
            let mut d = vec![D_PING];
            d.extend_from_slice(&self.clock_ms().to_le_bytes());
            self.link.send_datagram(HOST, d);
        }
    }

    /// Milliseconds since this session began. On the host, this is the
    /// clock shots are measured against.
    pub fn clock_ms(&self) -> u32 {
        self.started.elapsed().as_millis() as u32
    }

    /// What time it was on the host when things looked as they do on this
    /// computer now, given that copies are drawn `delay_ms` in the past.
    fn view_time_ms(&self, delay_ms: u32) -> u32 {
        let now = self.clock_ms();
        if self.hosting {
            return now.saturating_sub(delay_ms);
        }
        // 0 means "not known yet"; the host takes that to mean now
        self.host_offset.map_or(0, |o| (now as f64 - o - delay_ms as f64).max(1.0) as u32)
    }

    /// `NetSimulate`.
    pub fn simulate(&mut self, lag_ms: u32, loss_percent: u32) {
        self.sim.lag = Duration::from_millis(lag_ms as u64);
        self.sim.loss_percent = loss_percent.min(100);
    }

    pub fn simulated_lag_ms(&self) -> i32 {
        self.sim.lag.as_millis() as i32
    }

    fn handle(&mut self, item: Incoming) {
        match item {
            Incoming::Frame(peer, F_BIG_DATAGRAM, body) => self.on_datagram(peer, &body),
            Incoming::Frame(peer, kind, body) if self.hosting => self.host_frame(peer, kind, &body),
            Incoming::Frame(_, kind, body) => self.client_frame(kind, &body),
            Incoming::Datagram(peer, data) => self.on_datagram(peer, &data),
        }
    }

    fn on_datagram(&mut self, peer: PeerId, data: &[u8]) {
        let Some((&kind, body)) = data.split_first() else { return };
        let mut r = Reader(body);
        match kind {
            D_MSG => {
                let m = r.msg();
                if !self.hosting || (self.owns(peer, m.from) && (1..=99).contains(&m.kind) && self.relay(m.to, m.from, Some(peer), data, false)) {
                    self.deliver(m);
                }
            }
            D_SYNC => {
                let time_ms = r.int() as u32;
                let mut entries = Vec::with_capacity(body.len() / 44);
                while r.0.len() >= 44 {
                    let id = r.int();
                    entries.push((id, r.pose()));
                }
                if self.hosting {
                    // nobody may move another computer's entities
                    let Some(slot) = self.clients.get(&peer).filter(|c| c.greeted).map(|c| c.slot) else { return };
                    if entries.iter().any(|(id, _)| id >> SLOT_SHIFT != slot || self.host_moved.contains(id)) {
                        return;
                    }
                    self.relay(0, 0, Some(peer), data, false);
                }
                if self.sync_on {
                    limit(&mut self.sync_in);
                    self.sync_in.push_back(SyncIn::States { time_ms, entries });
                }
            }
            D_PING if self.hosting => {
                if self.clients.get(&peer).is_some_and(|c| c.greeted) {
                    let mut d = vec![D_PONG];
                    d.extend_from_slice(&body[..body.len().min(4)]);
                    d.extend_from_slice(&self.clock_ms().to_le_bytes());
                    self.link.send_datagram(peer, d);
                }
            }
            D_PONG if !self.hosting => {
                let (sent, host_time) = (r.int() as u32, r.int() as u32);
                let now = self.clock_ms();
                let rtt = now.wrapping_sub(sent).min(60_000) as i32;
                self.ping_ms = if self.ping_ms == 0 { rtt } else { (self.ping_ms * 4 + rtt) / 5 };
                let sample = now as f64 - host_time as f64;
                self.host_offset = Some(match self.host_offset {
                    Some(o) if (sample - o).abs() < 1000.0 => o + (sample - o) * 0.1,
                    _ => sample,
                });
            }
            D_INPUT if self.hosting && self.sync_on => {
                let id = r.int();
                if self.clients.get(&peer).is_none_or(|c| !c.greeted || c.slot != id >> SLOT_SHIFT) {
                    return;
                }
                let look: [f32; 4] = std::array::from_fn(|_| r.float());
                let mut inputs = Vec::with_capacity(r.0.len() / 24);
                while r.0.len() >= 24 {
                    let seq = r.int() as u32;
                    inputs.push((seq, std::array::from_fn(|_| r.float())));
                }
                limit(&mut self.sync_in);
                self.sync_in.push_back(SyncIn::Inputs { id, look, inputs });
            }
            D_CHAR if !self.hosting && self.sync_on => {
                let (id, ack) = (r.int(), r.int() as u32);
                let mut state = Vec::with_capacity(r.0.len() / 4);
                while r.0.len() >= 4 {
                    state.push(r.float());
                }
                limit(&mut self.sync_in);
                self.sync_in.push_back(SyncIn::CharState { id, ack, state });
            }
            _ => {}
        }
    }

    fn owns(&self, peer: PeerId, player: i32) -> bool {
        self.clients.get(&peer).is_some_and(|c| c.players.contains(&player))
    }

    fn reject(&mut self, peer: PeerId, reason: &str) {
        let mut b = Vec::new();
        put_str(&mut b, reason);
        self.link.send(peer, frame(F_REJECT, &b));
        self.link.kick(peer);
    }

    fn broadcast(&mut self, f: &[u8], except: Option<PeerId>) {
        for (p, c) in &self.clients {
            if c.greeted && Some(*p) != except {
                self.link.send(*p, f.to_vec());
            }
        }
    }

    fn host_frame(&mut self, peer: PeerId, kind: u8, body: &[u8]) {
        let Some(client) = self.clients.get(&peer) else { return };
        let mut r = Reader(body);
        if !client.greeted {
            if kind == F_HELLO {
                self.greet(peer, &mut r);
            } else {
                self.reject(peer, "Expected a greeting");
            }
            return;
        }
        match kind {
            F_CREATE => {
                let req = r.int();
                let name = r.string();
                let id = self.next_id;
                self.next_id += 1;
                let name = if name.is_empty() { format!("Player {id}") } else { clip(&name, NAME_LIMIT) };
                self.players.insert(id, name.clone());
                if let Some(c) = self.clients.get_mut(&peer) {
                    c.players.push(id);
                }
                let mut created = ints(&[req, id]);
                put_str(&mut created, &name);
                self.link.send(peer, frame(F_CREATED, &created));
                self.broadcast(&joined_frame(id, false, &name), Some(peer));
                self.player_joined(id, name);
            }
            F_DELETE => {
                let id = r.int();
                if self.owns(peer, id) {
                    if let Some(c) = self.clients.get_mut(&peer) {
                        c.players.retain(|p| *p != id);
                    }
                    self.remove_player(id, Some(peer));
                }
            }
            F_MSG => {
                let m = r.msg();
                if self.owns(peer, m.from) && (1..=99).contains(&m.kind) && self.relay(m.to, m.from, Some(peer), &frame(F_MSG, body), true) {
                    self.deliver(m);
                }
            }
            F_EVENT => {
                let e = Event { from: r.int(), to: r.int(), name: r.string(), data: r.string(), entity: 0, shot: None };
                if self.owns(peer, e.from) && self.relay(e.to, e.from, Some(peer), &event_frame(&e), true) {
                    self.push_event(e);
                }
            }
            F_SET => {
                let (key, value) = (r.string(), r.string());
                if key.len() <= VALUE_LIMIT && value.len() <= VALUE_LIMIT {
                    self.broadcast(&value_frame(F_VALUE, &key, &value), None);
                    self.values.insert(key, value);
                }
            }
            F_SHOT => {
                let (from, view) = (r.int(), r.int() as u32);
                let origin: [f32; 3] = std::array::from_fn(|_| r.float());
                let line: [f32; 3] = std::array::from_fn(|_| r.float());
                let (name, data) = (r.string(), r.string());
                if self.owns(peer, from) && origin.iter().chain(&line).all(|v| v.is_finite()) {
                    let now = self.clock_ms();
                    let view_ms = if view == 0 { now } else { view.clamp(now.saturating_sub(MAX_REWIND_MS), now) };
                    self.push_event(Event { name, from, to: from, data, entity: 0, shot: Some(Shot { view_ms, origin, line }) });
                }
            }
            F_SPAWN => {
                let s = Spawn { id: r.int(), owner: r.int(), pose: r.pose(), template: r.string() };
                let slot = self.clients[&peer].slot;
                if s.id >> SLOT_SHIFT == slot && self.owns(peer, s.owner) {
                    self.broadcast(&spawn_frame(&s), Some(peer));
                    self.add_spawn(s);
                }
            }
            F_DESPAWN => {
                let id = r.int();
                if id >> SLOT_SHIFT == self.clients[&peer].slot {
                    self.remove_spawn(id, Some(peer));
                }
            }
            _ => {}
        }
    }

    fn greet(&mut self, peer: PeerId, r: &mut Reader) {
        let protocol = r.int();
        let game = r.string();
        if protocol != PROTOCOL {
            return self.reject(peer, "A different version of NeptuneBASIC is hosting this game");
        }
        if !game.is_empty() && game != self.name {
            return self.reject(peer, "This is a different game");
        }
        if self.next_slot > MAX_SLOT {
            return self.reject(peer, "This game is not accepting more computers");
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        if let Some(c) = self.clients.get_mut(&peer) {
            c.greeted = true;
            c.slot = slot;
        }
        let mut b = ints(&[slot]);
        put_str(&mut b, &self.invite);
        self.link.send(peer, frame(F_WELCOME, &b));
        // catch the newcomer up
        for (id, name) in &self.players {
            self.link.send(peer, joined_frame(*id, *id == self.host_player, name));
        }
        for (k, v) in &self.values {
            self.link.send(peer, value_frame(F_VALUE, k, v));
        }
        for s in self.spawns.values() {
            self.link.send(peer, spawn_frame(s));
        }
    }

    fn player_joined(&mut self, id: i32, name: String) {
        self.push_event(Event { name: "join".into(), from: id, data: name.clone(), ..Default::default() });
        limit(&mut self.inbox);
        self.inbox.push_back(Msg { kind: MSG_JOINED, from: id, to: 0, data: name });
    }

    fn remove_player(&mut self, id: i32, from_client: Option<PeerId>) {
        if self.players.remove(&id).is_some() {
            self.broadcast(&frame(F_LEFT, &ints(&[id])), from_client);
            self.player_left(id);
        }
    }

    fn player_left(&mut self, id: i32) {
        self.push_event(Event { name: "leave".into(), from: id, ..Default::default() });
        limit(&mut self.inbox);
        self.inbox.push_back(Msg { kind: MSG_LEFT, from: id, ..Default::default() });
    }

    /// The host passes traffic on to the computers it is for; true if this
    /// computer should receive it too. Entity states use `to` = 0.
    fn relay(&mut self, to: i32, from: i32, origin: Option<PeerId>, data: &[u8], reliable: bool) -> bool {
        let send = |link: &mut Box<dyn Link>, p: PeerId| {
            if reliable {
                link.send(p, data.to_vec());
            } else {
                link.send_datagram(p, data.to_vec());
            }
        };
        if to == 0 {
            for (p, c) in &self.clients {
                if c.greeted && Some(*p) != origin {
                    send(&mut self.link, *p);
                }
            }
            return origin.is_some() || self.local.iter().any(|p| *p != from);
        }
        if self.local.contains(&to) {
            return true;
        }
        if let Some((p, _)) = self.clients.iter().find(|(_, c)| c.players.contains(&to)) {
            send(&mut self.link, *p);
        }
        false
    }

    fn deliver(&mut self, m: Msg) {
        if m.to == 0 || self.local.contains(&m.to) {
            limit(&mut self.inbox);
            self.inbox.push_back(m);
        }
    }

    pub fn push_event(&mut self, e: Event) {
        limit(&mut self.events);
        self.events.push_back(e);
    }

    fn client_frame(&mut self, kind: u8, body: &[u8]) {
        let mut r = Reader(body);
        match kind {
            F_WELCOME => {
                self.slot = r.int();
                self.invite = r.string();
                self.status = Status::Joined;
                if let Some(name) = self.auto_player.take() {
                    self.request_player(&name);
                }
            }
            F_REJECT => self.fail(&r.string()),
            F_CREATED => {
                let (req, id) = (r.int(), r.int());
                self.created.insert(req, id);
                self.players.insert(id, r.string());
                self.local.push(id);
                if self.me == 0 {
                    self.me = id;
                }
            }
            F_JOINED => {
                let (id, is_host) = (r.int(), r.int());
                let name = r.string();
                if is_host != 0 {
                    self.host_player = id;
                }
                self.players.insert(id, name.clone());
                self.player_joined(id, name);
            }
            F_LEFT => {
                let id = r.int();
                self.players.remove(&id);
                self.player_left(id);
            }
            F_MSG => {
                let m = r.msg();
                self.deliver(m);
            }
            F_EVENT => {
                let e = Event { from: r.int(), to: r.int(), name: r.string(), data: r.string(), entity: 0, shot: None };
                self.push_event(e);
            }
            F_VALUE => {
                let (key, value) = (r.string(), r.string());
                self.values.insert(key, value);
            }
            F_SPAWN => {
                let s = Spawn { id: r.int(), owner: r.int(), pose: r.pose(), template: r.string() };
                self.add_spawn(s);
            }
            F_DESPAWN => {
                let id = r.int();
                self.remove_spawn(id, Some(HOST));
            }
            F_CHAR_SET if self.sync_on => {
                let id = r.int();
                let mut values = Vec::with_capacity(r.0.len() / 4);
                while r.0.len() >= 4 {
                    values.push(r.float());
                }
                limit(&mut self.sync_in);
                self.sync_in.push_back(SyncIn::CharSettings { id, values });
            }
            _ => {}
        }
    }

    /// `CreateNetPlayer` on the host. Clients use [`Session::request_player`].
    pub fn create_player(&mut self, name: &str) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        let name = if name.is_empty() { format!("Player {id}") } else { clip(name, NAME_LIMIT) };
        self.players.insert(id, name.clone());
        self.local.push(id);
        if self.me == 0 {
            self.me = id;
        }
        if self.host_player == 0 {
            self.host_player = id;
        }
        self.broadcast(&joined_frame(id, id == self.host_player, &name), None);
        id
    }

    /// Asks the host for a player; returns the request to pass to
    /// [`Session::take_created`].
    pub fn request_player(&mut self, name: &str) -> i32 {
        let req = self.next_request;
        self.next_request += 1;
        let mut b = ints(&[req]);
        put_str(&mut b, name);
        self.link.send(HOST, frame(F_CREATE, &b));
        req
    }

    pub fn take_created(&mut self, req: i32) -> Option<i32> {
        self.created.remove(&req)
    }

    pub fn delete_player(&mut self, id: i32) {
        if !self.local.contains(&id) {
            return;
        }
        self.local.retain(|p| *p != id);
        if self.me == id {
            self.me = self.local.first().copied().unwrap_or(0);
        }
        if self.hosting {
            self.remove_player(id, None);
        } else {
            self.link.send(HOST, frame(F_DELETE, &ints(&[id])));
            self.players.remove(&id);
        }
    }

    pub fn player_name(&self, id: i32) -> String {
        self.players.get(&id).cloned().unwrap_or_default()
    }

    pub fn is_local(&self, id: i32) -> bool {
        self.local.contains(&id)
    }

    pub fn player_count(&self) -> i32 {
        self.players.len() as i32
    }

    /// `NetPlayerAt`: players in joining order, from 1.
    pub fn player_at(&self, index: i32) -> i32 {
        usize::try_from(index - 1).ok().and_then(|i| self.players.keys().nth(i)).copied().unwrap_or(0)
    }

    fn running(&self) -> bool {
        matches!(self.status, Status::Hosting | Status::Joined)
    }

    /// `SendNetMsg`: to one player, or to everyone else (`to` = 0).
    pub fn send(&mut self, kind: i32, data: &str, from: i32, to: i32, reliable: bool) -> bool {
        if !self.running() || !self.local.contains(&from) || !(1..=99).contains(&kind) {
            return false;
        }
        let m = Msg { kind, from, to, data: data.to_string() };
        let body = msg_body(&m);
        let bytes = if reliable {
            frame(F_MSG, &body)
        } else {
            let mut d = vec![D_MSG];
            d.extend_from_slice(&body);
            d
        };
        if self.hosting {
            if self.relay(to, from, None, &bytes, reliable) {
                self.deliver(m);
            }
        } else if self.send_from_client(to, from, bytes, reliable) {
            self.deliver(m);
        }
        true
    }

    /// True if the traffic is also for this computer.
    fn send_from_client(&mut self, to: i32, from: i32, bytes: Vec<u8>, reliable: bool) -> bool {
        let here = if to == 0 { self.local.iter().any(|p| *p != from) } else { self.local.contains(&to) };
        if to == 0 || !here {
            if reliable {
                self.link.send(HOST, bytes);
            } else {
                self.link.send_datagram(HOST, bytes);
            }
        }
        here
    }

    /// `RecvNetMsg`: the next message for this computer's players.
    pub fn recv(&mut self) -> Option<Msg> {
        self.inbox.pop_front()
    }

    /// `NetSend`: from this computer's main player.
    pub fn send_event(&mut self, name: &str, data: &str, to: i32) -> bool {
        if !self.running() || self.me == 0 {
            return false;
        }
        let e = Event { name: name.to_string(), from: self.me, to, data: data.to_string(), entity: 0, shot: None };
        let f = event_frame(&e);
        let here = if self.hosting {
            self.relay(to, e.from, None, &f, true)
        } else {
            self.send_from_client(to, e.from, f, true)
        };
        if here {
            self.push_event(e);
        }
        true
    }

    /// `NetFire`: tells the host a shot was fired, and what the shooter
    /// could see. Copies here are drawn `delay_ms` in the past.
    pub fn fire(&mut self, name: &str, data: &str, origin: [f32; 3], line: [f32; 3], delay_ms: u32) -> bool {
        if !self.running() || self.me == 0 {
            return false;
        }
        let view_ms = self.view_time_ms(delay_ms);
        if self.hosting {
            let shot = Some(Shot { view_ms, origin, line });
            self.push_event(Event { name: name.to_string(), from: self.me, to: self.me, data: data.to_string(), entity: 0, shot });
            return true;
        }
        let mut b = ints(&[self.me, view_ms as i32]);
        for v in origin.iter().chain(&line) {
            b.extend_from_slice(&v.to_le_bytes());
        }
        put_str(&mut b, name);
        put_str(&mut b, data);
        self.link.send(HOST, frame(F_SHOT, &b));
        true
    }

    /// `NetRecv`.
    pub fn next_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// `NetSet`: every computer ends up with the order the host saw.
    pub fn set_value(&mut self, key: &str, value: &str) {
        if !self.running() {
            return;
        }
        self.values.insert(key.to_string(), value.to_string());
        if self.hosting {
            self.broadcast(&value_frame(F_VALUE, key, value), None);
        } else {
            self.link.send(HOST, value_frame(F_SET, key, value));
        }
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    // ------------------------------------------------------------ entities

    /// Starts entity sync; what already exists arrives through `take_sync`.
    pub fn enable_sync(&mut self) {
        if !self.sync_on {
            self.sync_on = true;
            self.sync_in.extend(self.spawns.values().cloned().map(SyncIn::Spawn));
        }
    }

    pub fn take_sync(&mut self) -> Vec<SyncIn> {
        self.sync_in.drain(..).collect()
    }

    fn add_spawn(&mut self, s: Spawn) {
        if self.sync_on {
            limit(&mut self.sync_in);
            self.sync_in.push_back(SyncIn::Spawn(s.clone()));
        }
        self.spawns.insert(s.id, s);
    }

    /// `from` is who announced it: on the host, computers other than that
    /// one are told, and from a client's point of view nobody is.
    fn remove_spawn(&mut self, id: i32, from: Option<PeerId>) {
        if self.spawns.remove(&id).is_some() {
            if self.hosting {
                self.broadcast(&frame(F_DESPAWN, &ints(&[id])), from);
            }
            if self.sync_on {
                limit(&mut self.sync_in);
                self.sync_in.push_back(SyncIn::Despawn(id));
            }
        }
    }

    /// `NetSpawn`: announces an entity created here and returns its network id.
    pub fn spawn(&mut self, template: &str, pose: Pose) -> Option<i32> {
        if !self.running() || self.me == 0 || self.next_entity >= 1 << SLOT_SHIFT {
            return None;
        }
        let s = Spawn { id: self.slot << SLOT_SHIFT | self.next_entity, owner: self.me, template: template.to_string(), pose };
        self.next_entity += 1;
        let f = spawn_frame(&s);
        if self.hosting {
            self.broadcast(&f, None);
        } else {
            self.link.send(HOST, f);
        }
        self.spawns.insert(s.id, s.clone());
        Some(s.id)
    }

    /// From now on the host moves this entity for its owner.
    pub fn mark_host_moved(&mut self, id: i32) {
        self.host_moved.insert(id);
    }

    /// Sends a player's buttons to the host.
    pub fn send_inputs(&mut self, id: i32, look: [f32; 4], inputs: &[CharInput]) {
        if !self.running() || self.hosting {
            return;
        }
        let mut d = vec![D_INPUT];
        d.extend_from_slice(&id.to_le_bytes());
        for v in look {
            d.extend_from_slice(&v.to_le_bytes());
        }
        for (seq, input) in inputs {
            d.extend_from_slice(&seq.to_le_bytes());
            for v in input {
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        self.link.send_datagram(HOST, d);
    }

    /// Tells a character's owner what became of its inputs.
    pub fn send_char_state(&mut self, id: i32, owner: i32, ack: u32, state: &[f32]) {
        if !self.hosting {
            return;
        }
        let mut d = vec![D_CHAR];
        d.extend_from_slice(&id.to_le_bytes());
        d.extend_from_slice(&ack.to_le_bytes());
        for v in state {
            d.extend_from_slice(&v.to_le_bytes());
        }
        self.relay(owner, 0, None, &d, false);
    }

    /// The host changed a player's character settings.
    pub fn send_char_settings(&mut self, id: i32, owner: i32, values: &[f32]) {
        if !self.hosting {
            return;
        }
        let mut b = id.to_le_bytes().to_vec();
        for v in values {
            b.extend_from_slice(&v.to_le_bytes());
        }
        self.relay(owner, 0, None, &frame(F_CHAR_SET, &b), true);
    }

    /// `NetDespawn` or `FreeEntity` of an entity spawned here.
    pub fn despawn(&mut self, id: i32) {
        if self.spawns.remove(&id).is_none() {
            return;
        }
        let f = frame(F_DESPAWN, &ints(&[id]));
        if self.hosting {
            self.broadcast(&f, None);
        } else if self.running() {
            self.link.send(HOST, f);
        }
    }

    /// Sends the current state of entities spawned here. May be lost, so it
    /// is sent again and again by the sync layer.
    pub fn send_states(&mut self, time_ms: u32, entries: &[(i32, Pose)]) {
        if !self.running() {
            return;
        }
        for chunk in entries.chunks(SYNC_PER_DATAGRAM) {
            let mut d = vec![D_SYNC];
            d.extend_from_slice(&time_ms.to_le_bytes());
            for (id, pose) in chunk {
                d.extend_from_slice(&id.to_le_bytes());
                put_pose(&mut d, pose);
            }
            if self.hosting {
                self.relay(0, 0, None, &d, false);
            } else {
                self.link.send_datagram(HOST, d);
            }
        }
    }
}

// ---------------------------------------------------------------- handle

/// How a game gets started.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    Host,
    /// An invite, an address, or "" to search the local network.
    Join(String),
    /// Join a game on the local network, or host one if there is none.
    Auto,
}

/// The running game, with its network thread.
pub struct NetGame {
    session: Arc<Mutex<Session>>,
    #[cfg(feature = "online")]
    _thread: crate::netlink::Thread,
}

impl NetGame {
    #[cfg(feature = "online")]
    pub fn start(plan: Plan, game: &str, player: Option<&str>, opts: &Options) -> Option<NetGame> {
        let links = crate::netlink::Peers::default();
        let mut s = Session::new(game, Box::new(crate::netlink::IrohLink(links.clone())));
        if let Some(p) = player {
            s.set_auto_player(p);
        }
        s.simulate(opts.lag_ms, opts.loss_percent);
        let session = Arc::new(Mutex::new(s));
        let thread = crate::netlink::start(plan, session.clone(), links, opts.relay.clone())?;
        Some(NetGame { session, _thread: thread })
    }

    #[cfg(not(feature = "online"))]
    pub fn start(_plan: Plan, _game: &str, _player: Option<&str>, _opts: &Options) -> Option<NetGame> {
        None
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Session) -> R) -> R {
        f(&mut self.session.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Settings made before a game starts.
#[derive(Clone, Default)]
pub struct Options {
    pub relay: Option<String>,
    pub lag_ms: u32,
    pub loss_percent: u32,
}

/// Net state shared between the standard library and the 3D runtime.
#[derive(Default)]
pub struct NetState {
    pub game: Option<NetGame>,
    /// The last message `RecvNetMsg` and event `NetRecv` returned.
    pub msg: Msg,
    pub event: Event,
    /// A `NetPlay`-style start in progress: when it began, and if it wants a player.
    opening: Option<(Instant, bool)>,
    /// A `CreateNetPlayer` waiting for the host: request and when it was made.
    creating: Option<(i32, Instant)>,
    pub options: Options,
    pub error: String,
}

impl std::fmt::Debug for NetState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NetState")
    }
}

pub type SharedNet = Rc<RefCell<NetState>>;

/// How long a start may wait for the host's answers.
const OPEN_TIMEOUT: Duration = Duration::from_secs(30);
const CREATE_TIMEOUT: Duration = Duration::from_secs(5);

impl NetState {
    pub fn with<R>(&self, default: R, f: impl FnOnce(&mut Session) -> R) -> R {
        match &self.game {
            Some(g) => g.with(f),
            None => default,
        }
    }

    /// The commands that start a game wait a frame at a time, so the window
    /// stays alive: 1 joined, 2 hosting, 0 failed.
    pub fn open(&mut self, ctx: &mut NativeCtx, plan: Plan, game: &str, player: Option<&str>) -> Result<Value, String> {
        if self.opening.is_none() {
            self.game = None;
            self.error.clear();
            self.creating = None;
            self.game = NetGame::start(plan, game, player, &self.options);
            if self.game.is_none() {
                self.error = "Networking is not available".into();
                return Ok(Value::Int(0));
            }
            self.opening = Some((Instant::now(), player.is_some()));
        }
        let Some((began, wants_player)) = self.opening else { return Ok(Value::Int(0)) };
        let (status, me, error) = self.with((Status::Starting, 0, String::new()), |s| (s.status.clone(), s.me, s.error.clone()));
        let result = match status {
            Status::Hosting => Some(2),
            Status::Joined if !wants_player || me != 0 => Some(1),
            Status::Failed(_) | Status::Lost => Some(0),
            _ if began.elapsed() > OPEN_TIMEOUT => Some(-1),
            _ => None,
        };
        let Some(result) = result else {
            ctx.request_retry();
            std::thread::sleep(Duration::from_millis(1));
            return Ok(Value::Int(0));
        };
        self.opening = None;
        if result <= 0 {
            self.error = if result < 0 { "The host did not answer".to_string() } else { error };
            self.game = None;
            return Ok(Value::Int(0));
        }
        Ok(Value::Int(result))
    }

    /// `CreateNetPlayer`, which needs an answer from the host on a client.
    pub fn create_player(&mut self, ctx: &mut NativeCtx, name: &str) -> Result<Value, String> {
        let Some(game) = &self.game else {
            return Err("no net game: use HostNetGame or JoinNetGame first".into());
        };
        if game.with(|s| s.is_host()) {
            return Ok(Value::Int(game.with(|s| s.create_player(name))));
        }
        let (req, at) = *self.creating.get_or_insert_with(|| (game.with(|s| s.request_player(name)), Instant::now()));
        if let Some(id) = game.with(|s| s.take_created(req)) {
            self.creating = None;
            return Ok(Value::Int(id));
        }
        if at.elapsed() > CREATE_TIMEOUT || game.with(|s| !s.running()) {
            self.creating = None;
            return Ok(Value::Int(0));
        }
        ctx.request_retry();
        std::thread::sleep(Duration::from_millis(1));
        Ok(Value::Int(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A network in a test: what each computer sent, to be delivered by hand.
    type Sent = (PeerId, Vec<u8>, bool);

    #[derive(Clone, Default)]
    struct Wires(Arc<Mutex<Vec<Sent>>>);

    struct TestLink(Wires);

    impl Link for TestLink {
        fn send(&mut self, peer: PeerId, frame: Vec<u8>) {
            self.0 .0.lock().unwrap().push((peer, frame, true));
        }
        fn send_datagram(&mut self, peer: PeerId, data: Vec<u8>) {
            self.0 .0.lock().unwrap().push((peer, data, false));
        }
        fn kick(&mut self, _peer: PeerId) {}
    }

    /// A host and its clients, wired together.
    struct Net {
        host: Session,
        host_out: Wires,
        clients: Vec<(Session, Wires, Framer, Framer)>,
    }

    impl Net {
        fn new(game: &str) -> Net {
            let host_out = Wires::default();
            let mut host = Session::new(game, Box::new(TestLink(host_out.clone())));
            host.start_host("invite");
            Net { host, host_out, clients: Vec::new() }
        }

        fn join(&mut self, game: &str) -> usize {
            let out = Wires::default();
            let client = Session::new(game, Box::new(TestLink(out.clone())));
            self.clients.push((client, out, Framer::default(), Framer::default()));
            let i = self.clients.len() - 1;
            self.host.peer_connected(i as PeerId + 1);
            self.clients[i].0.connected();
            self.settle();
            i
        }

        /// Delivers everything sent until the network is quiet.
        fn settle(&mut self) {
            loop {
                let mut moved = false;
                let from_host: Vec<_> = self.host_out.0.lock().unwrap().drain(..).collect();
                for (peer, data, reliable) in from_host {
                    moved = true;
                    let Some((c, _, _, framer)) = self.clients.get_mut(peer as usize - 1) else { continue };
                    if reliable {
                        for (k, b) in framer.push(&data).unwrap() {
                            c.frame_in(HOST, k, b);
                        }
                    } else {
                        c.datagram_in(HOST, data);
                    }
                }
                for i in 0..self.clients.len() {
                    let sent: Vec<_> = self.clients[i].1 .0.lock().unwrap().drain(..).collect();
                    for (_, data, reliable) in sent {
                        moved = true;
                        if reliable {
                            for (k, b) in self.clients[i].2.push(&data).unwrap() {
                                self.host.frame_in(i as PeerId + 1, k, b);
                            }
                        } else {
                            self.host.datagram_in(i as PeerId + 1, data);
                        }
                    }
                }
                if !moved {
                    return;
                }
            }
        }

        fn player(&mut self, i: usize, name: &str) -> i32 {
            let req = self.clients[i].0.request_player(name);
            self.settle();
            self.clients[i].0.take_created(req).unwrap()
        }

        fn c(&mut self, i: usize) -> &mut Session {
            &mut self.clients[i].0
        }
    }

    fn drain(s: &mut Session) -> Vec<Msg> {
        std::iter::from_fn(|| s.recv()).collect()
    }

    fn drain_events(s: &mut Session) -> Vec<Event> {
        std::iter::from_fn(|| s.next_event()).collect()
    }

    fn pose(x: f32) -> Pose {
        [x, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]
    }

    #[test]
    fn host_and_two_clients() {
        let mut n = Net::new("test");
        let alice = n.host.create_player("Alice");
        let a = n.join("test");
        assert_eq!(n.c(a).status, Status::Joined);
        assert_eq!(n.c(a).recv().map(|m| (m.kind, m.from, m.data)), Some((MSG_JOINED, alice, "Alice".to_string())));
        assert_eq!(n.c(a).host_player, alice);

        let bob = n.player(a, "Bob");
        assert!(bob > 0 && n.c(a).is_local(bob) && !n.c(a).is_local(alice));
        let m = n.host.recv().unwrap();
        assert_eq!((m.kind, m.from, m.data.as_str()), (MSG_JOINED, bob, "Bob"));
        assert_eq!(n.host.player_name(bob), "Bob");

        assert!(n.c(a).send(7, "hello alice", bob, alice, true));
        n.settle();
        let m = n.host.recv().unwrap();
        assert_eq!((m.kind, m.from, m.to, m.data.as_str()), (7, bob, alice, "hello alice"));
        n.host.send(9, "to everyone", alice, 0, true);
        n.settle();
        let m = n.c(a).recv().unwrap();
        assert_eq!((m.kind, m.from, m.data.as_str()), (9, alice, "to everyone"));

        // a second computer learns about everyone, and hears about leaving
        let b = n.join("test");
        let names: Vec<String> = drain(n.c(b)).into_iter().map(|m| m.data).collect();
        assert_eq!(names, ["Alice", "Bob"]);
        n.host.peer_gone(1, "gone");
        n.settle();
        assert_eq!(n.host.recv().map(|m| (m.kind, m.from)), Some((MSG_LEFT, bob)));
        assert_eq!(n.c(b).recv().map(|m| (m.kind, m.from)), Some((MSG_LEFT, bob)));

        // the host closes: the client's session is lost
        n.c(b).peer_gone(HOST, "closed");
        assert_eq!(n.c(b).recv().map(|m| m.kind), Some(MSG_SESSION_LOST));
        assert_eq!(n.c(b).status, Status::Lost);
    }

    #[test]
    fn a_blank_name_gets_a_default_everywhere() {
        let mut n = Net::new("test");
        n.host.create_player("");
        assert_eq!(n.host.player_name(1), "Player 1");
        let a = n.join("test");
        let bob = n.player(a, "");
        assert_eq!(n.c(a).player_name(bob), "Player 2");
        assert_eq!(n.host.player_name(bob), "Player 2");
    }

    #[test]
    fn a_different_game_is_refused() {
        let mut n = Net::new("Space Race");
        let a = n.join("Kart Race");
        assert_eq!(n.c(a).status, Status::Failed("This is a different game".into()));
        // an empty name means "whatever is at this address"
        let b = n.join("");
        assert_eq!(n.c(b).status, Status::Joined);
    }

    #[test]
    fn a_client_cannot_impersonate_or_remove_others() {
        let mut n = Net::new("test");
        let alice = n.host.create_player("Alice");
        let a = n.join("test");
        let bob = n.player(a, "Bob");
        drain(&mut n.host);

        // forged sender
        let forged = Msg { kind: 1, from: alice, to: 0, data: "I quit".into() };
        n.host.frame_in(1, F_MSG, msg_body(&forged));
        // system message types are for the host
        let system = Msg { kind: MSG_LEFT, from: bob, to: 0, data: String::new() };
        n.host.frame_in(1, F_MSG, msg_body(&system));
        // deleting somebody else's player
        n.host.frame_in(1, F_DELETE, ints(&[alice]));
        n.settle();
        assert_eq!(drain(&mut n.host), vec![]);
        assert_eq!(n.host.player_name(alice), "Alice");
    }

    #[test]
    fn traffic_before_a_greeting_is_refused() {
        let mut n = Net::new("test");
        n.host.peer_connected(9);
        n.host.frame_in(9, F_CREATE, ints(&[1, 0]));
        assert_eq!(n.host.player_count(), 0);
    }

    #[test]
    fn players_on_one_computer_hear_each_other() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        let a = n.join("test");
        let p1 = n.player(a, "One");
        let p2 = n.player(a, "Two");
        drain(n.c(a));
        n.c(a).send(3, "all", p1, 0, true);
        n.c(a).send(4, "direct", p1, p2, true);
        n.settle();
        let kinds: Vec<i32> = drain(n.c(a)).iter().map(|m| m.kind).collect();
        assert_eq!(kinds, [3, 4]);
    }

    #[test]
    fn unreliable_messages_are_datagrams() {
        let mut n = Net::new("test");
        let alice = n.host.create_player("Alice");
        let a = n.join("test");
        let bob = n.player(a, "Bob");
        drain(&mut n.host);
        n.c(a).send(5, "pos", bob, 0, false);
        assert!(!n.clients[a].1 .0.lock().unwrap()[0].2);
        n.settle();
        assert_eq!(drain(&mut n.host)[0].data, "pos");
        // relayed the same way
        drain(n.c(a));
        n.host.send(6, "pos2", alice, 0, false);
        assert!(!n.host_out.0.lock().unwrap()[0].2);
    }

    #[test]
    fn events_values_and_late_joiners() {
        let mut n = Net::new("test");
        let alice = n.host.create_player("Alice");
        n.host.set_value("round", "1");
        let a = n.join("test");
        let bob = n.player(a, "Bob");
        assert_eq!(n.c(a).value("round"), Some("1"));
        drain_events(n.c(a));
        drain_events(&mut n.host);

        n.c(a).me = bob;
        assert!(n.c(a).send_event("hit", "12", alice));
        n.settle();
        let e = n.host.next_event().unwrap();
        assert_eq!((e.name.as_str(), e.from, e.data.as_str()), ("hit", bob, "12"));

        // the host's order wins, on every computer
        n.c(a).set_value("score", "5");
        n.host.set_value("score", "6");
        n.settle();
        assert_eq!(n.host.value("score"), Some("5"));
        assert_eq!(n.c(a).value("score"), Some("5"));
    }

    #[test]
    fn a_leaving_computer_takes_its_entities() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        n.host.enable_sync();
        let a = n.join("test");
        n.player(a, "Bob");
        let b = n.join("test");
        n.player(b, "Carol");
        n.c(b).enable_sync();
        n.c(b).take_sync();

        let id = n.c(a).spawn("kart", pose(1.0)).unwrap();
        n.settle();
        assert!(matches!(&n.host.take_sync()[0], SyncIn::Spawn(s) if s.id == id && s.template == "kart"));
        assert!(matches!(&n.c(b).take_sync()[0], SyncIn::Spawn(s) if s.pose[0] == 1.0));

        n.c(a).send_states(50, &[(id, pose(2.0))]);
        n.settle();
        assert!(matches!(&n.c(b).take_sync()[0], SyncIn::States { time_ms: 50, entries } if entries[0] == (id, pose(2.0))));

        n.host.peer_gone(1, "gone");
        n.settle();
        assert_eq!(n.c(b).take_sync(), vec![SyncIn::Despawn(id)]);
    }

    #[test]
    fn nobody_moves_another_computers_entities() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        n.host.enable_sync();
        let mine = n.host.spawn("kart", pose(0.0)).unwrap();
        let a = n.join("test");
        n.player(a, "Bob");
        n.host.take_sync();
        n.c(a).send_states(1, &[(mine, pose(9.0))]);
        n.c(a).send_states(1, &[]);
        n.settle();
        assert_eq!(n.host.take_sync(), vec![]);
    }

    #[test]
    fn inputs_reach_only_the_host_and_answers_only_the_owner() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        n.host.enable_sync();
        let a = n.join("test");
        let bob = n.player(a, "Bob");
        let b = n.join("test");
        n.player(b, "Carol");
        n.c(a).enable_sync();
        n.c(b).enable_sync();
        let id = n.c(a).spawn("hero", pose(0.0)).unwrap();
        n.settle();
        n.host.take_sync();
        n.c(a).take_sync();
        n.c(b).take_sync();

        n.c(a).send_inputs(id, [0.0, 0.0, 0.0, 1.0], &[(1, [0.016, 1.0, 0.0, 0.0, 0.0])]);
        n.settle();
        assert_eq!(n.host.take_sync(), vec![SyncIn::Inputs { id, look: [0.0, 0.0, 0.0, 1.0], inputs: vec![(1, [0.016, 1.0, 0.0, 0.0, 0.0])] }]);
        assert_eq!(n.c(b).take_sync(), vec![]);

        n.host.send_char_state(id, bob, 1, &[1.0, 2.0]);
        n.host.send_char_settings(id, bob, &[7.0]);
        n.settle();
        assert_eq!(n.c(a).take_sync(), vec![SyncIn::CharState { id, ack: 1, state: vec![1.0, 2.0] }, SyncIn::CharSettings { id, values: vec![7.0] }]);
        assert_eq!(n.c(b).take_sync(), vec![]);
    }

    #[test]
    fn inputs_for_someone_elses_character_are_refused() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        n.host.enable_sync();
        let theirs = n.host.spawn("hero", pose(0.0)).unwrap();
        let a = n.join("test");
        n.player(a, "Bob");
        n.host.take_sync();
        n.c(a).send_inputs(theirs, [0.0; 4], &[(1, [0.016, 1.0, 0.0, 0.0, 0.0])]);
        n.settle();
        assert_eq!(n.host.take_sync(), vec![]);
    }

    #[test]
    fn a_player_cannot_say_where_a_host_moved_character_is() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        n.host.enable_sync();
        let a = n.join("test");
        n.player(a, "Bob");
        let id = n.c(a).spawn("hero", pose(0.0)).unwrap();
        n.settle();
        n.host.take_sync();
        n.host.mark_host_moved(id);
        n.c(a).send_states(1, &[(id, pose(50.0))]);
        n.settle();
        assert_eq!(n.host.take_sync(), vec![]);
    }

    fn fire_from(n: &mut Net, a: usize) -> Event {
        drain_events(&mut n.host);
        n.c(a).fire("shot", "gun", [1.0, 2.0, 3.0], [0.0, 0.0, 9.0], 100);
        n.settle();
        n.host.next_event().unwrap()
    }

    #[test]
    fn a_shot_reaches_only_the_host() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        let a = n.join("test");
        let bob = n.player(a, "Bob");
        n.c(a).me = bob;
        let b = n.join("test");
        n.player(b, "Carol");
        drain_events(n.c(b));
        let e = fire_from(&mut n, a);
        assert_eq!((e.name.as_str(), e.from, e.data.as_str()), ("shot", bob, "gun"));
        let shot = e.shot.unwrap();
        assert_eq!((shot.origin, shot.line), ([1.0, 2.0, 3.0], [0.0, 0.0, 9.0]));
        assert_eq!(drain_events(n.c(b)), vec![]);
    }

    #[test]
    fn a_shot_before_the_clocks_are_known_is_judged_as_now() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        std::thread::sleep(Duration::from_millis(20));
        let a = n.join("test");
        let bob = n.player(a, "Bob");
        n.c(a).me = bob;
        let e = fire_from(&mut n, a);
        assert!(n.host.clock_ms() - e.shot.unwrap().view_ms < 20);
    }

    #[test]
    fn nobody_can_claim_to_have_seen_further_back_than_the_limit() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        let a = n.join("test");
        let bob = n.player(a, "Bob");
        std::thread::sleep(Duration::from_millis(600));
        drain_events(&mut n.host);
        let mut b = ints(&[bob, 1]);
        for v in [0.0f32; 6] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        put_str(&mut b, "shot");
        put_str(&mut b, "");
        n.host.frame_in(1, F_SHOT, b);
        let shot = n.host.next_event().unwrap().shot.unwrap();
        assert!(n.host.clock_ms() - shot.view_ms >= MAX_REWIND_MS);
        assert!(n.host.clock_ms() - shot.view_ms < MAX_REWIND_MS + 50);
    }

    #[test]
    fn a_shot_cannot_be_fired_as_someone_else_or_with_nonsense() {
        let mut n = Net::new("test");
        let alice = n.host.create_player("Alice");
        let a = n.join("test");
        let bob = n.player(a, "Bob");
        drain_events(&mut n.host);
        let shot_from = |from: i32, x: f32| {
            let mut b = ints(&[from, 1]);
            for v in [x, 0.0, 0.0, 0.0, 0.0, 1.0] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            put_str(&mut b, "shot");
            put_str(&mut b, "");
            b
        };
        n.host.frame_in(1, F_SHOT, shot_from(alice, 0.0));
        n.host.frame_in(1, F_SHOT, shot_from(bob, f32::NAN));
        assert_eq!(n.host.next_event(), None);
    }

    #[test]
    fn the_host_hears_its_own_shots_the_same_way() {
        let mut n = Net::new("test");
        let alice = n.host.create_player("Alice");
        drain_events(&mut n.host);
        assert!(n.host.fire("shot", "", [0.0; 3], [0.0, 0.0, 1.0], 100));
        let e = n.host.next_event().unwrap();
        assert_eq!(e.from, alice);
        assert!(e.shot.is_some());
    }

    #[test]
    fn pings_measure_the_trip_and_the_hosts_clock() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        std::thread::sleep(Duration::from_millis(200));
        let a = n.join("test");
        n.player(a, "Bob");
        n.c(a).simulate(20, 0);
        n.c(a).tick();
        n.settle();
        std::thread::sleep(Duration::from_millis(30));
        n.c(a).tick();
        n.settle();
        assert!(n.c(a).ping_ms >= 20, "{}", n.c(a).ping_ms);
        // the view is this long ago on the host: the trip here, and the delay
        let behind = n.host.clock_ms() as i64 - n.c(a).view_time_ms(100) as i64;
        assert!((100..200).contains(&behind), "{behind}");
    }

    #[test]
    fn late_joiners_see_existing_entities() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        let id = n.host.spawn("kart", pose(3.0)).unwrap();
        let a = n.join("test");
        n.c(a).enable_sync();
        assert!(matches!(&n.c(a).take_sync()[0], SyncIn::Spawn(s) if s.id == id));
    }

    #[test]
    fn many_entities_are_split_into_datagrams() {
        let mut n = Net::new("test");
        n.host.create_player("Alice");
        let a = n.join("test");
        n.c(a).enable_sync();
        n.c(a).take_sync();
        let entries: Vec<(i32, Pose)> = (0..50).map(|i| (1 << SLOT_SHIFT | i, pose(i as f32))).collect();
        n.host.send_states(1, &entries);
        assert!(n.host_out.0.lock().unwrap().iter().all(|(_, d, r)| !r && d.len() < 1162));
        n.settle();
        let got: usize = n.c(a).take_sync().iter().map(|s| if let SyncIn::States { entries, .. } = s { entries.len() } else { 0 }).sum();
        assert_eq!(got, 50);
    }

    #[test]
    fn simulated_lag_and_loss() {
        let mut n = Net::new("test");
        let alice = n.host.create_player("Alice");
        let a = n.join("test");
        drain(n.c(a));
        n.c(a).simulate(30, 0);
        n.host.send(1, "late", alice, 0, true);
        n.settle();
        assert_eq!(n.c(a).recv(), None);
        std::thread::sleep(Duration::from_millis(50));
        n.c(a).tick();
        assert_eq!(n.c(a).recv().map(|m| m.data), Some("late".to_string()));

        // full loss drops datagrams but never reliable traffic
        n.c(a).simulate(0, 100);
        n.host.send(1, "gone", alice, 0, false);
        n.host.send(1, "kept", alice, 0, true);
        n.settle();
        assert_eq!(drain(n.c(a)).iter().map(|m| m.data.as_str()).collect::<Vec<_>>(), ["kept"]);
    }

    #[test]
    fn unread_messages_are_capped() {
        let mut n = Net::new("test");
        let alice = n.host.create_player("Alice");
        let a = n.join("test");
        for _ in 0..QUEUE_LIMIT + 10 {
            n.host.send(1, "x", alice, 0, true);
        }
        n.settle();
        assert!(n.c(a).inbox.len() <= QUEUE_LIMIT);
    }

    #[test]
    fn framer_handles_split_frames_and_garbage() {
        let f = frame(F_MSG, b"hello");
        let mut r = Framer::default();
        assert_eq!(r.push(&f[..3]), Some(vec![]));
        assert_eq!(r.push(&f[3..]), Some(vec![(F_MSG, b"hello".to_vec())]));
        assert_eq!(Framer::default().push(&u32::MAX.to_le_bytes()), None);
        assert_eq!(Framer::default().push(&0u32.to_le_bytes()), None);
    }
}
