//! Multiplayer games with Blitz3D's net commands (`HostNetGame`,
//! `JoinNetGame`, `CreateNetPlayer`, `SendNetMsg`, `RecvNetMsg`, ...).
//!
//! Blitz3D built these on DirectPlay. Here they run over TCP with the same
//! commands and message types, on every platform:
//!
//! * the host listens on a TCP port (47700 by default) and answers LAN
//!   searches on UDP port 47701, so `JoinNetGame name$, ""` and
//!   `StartNetGame` find games on the local network without typing an IP;
//! * every message goes through the host, which relays it to the players it
//!   is for (star topology); all messages are delivered reliably and in order;
//! * system messages: 100 a player joined (`NetMsgFrom` is the new player,
//!   `NetMsgData` its name), 101 a player left, 200 the session was lost
//!   (the host closed or the connection dropped). Program messages use
//!   types 1-99.
//!
//! Wire format: frames of `u32 length` + `u8 kind` + payload; strings are
//! `u32 length` + UTF-8.

use std::collections::{HashMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 47700;
pub const DISCOVERY_PORT: u16 = 47701;
pub const MSG_JOINED: i32 = 100;
pub const MSG_LEFT: i32 = 101;
pub const MSG_SESSION_LOST: i32 = 200;

const F_CREATE: u8 = 1; // client -> host: request id, name
const F_CREATED: u8 = 2; // host -> client: request id, player id
const F_JOINED: u8 = 3; // host -> client: player id, name
const F_LEFT: u8 = 4; // host -> client: player id
const F_MSG: u8 = 5; // either way: type, from, to, data
const F_DELETE: u8 = 6; // client -> host: player id

/// A received message, as `NetMsgType`/`From`/`To`/`Data` report it.
#[derive(Clone, Debug, PartialEq)]
pub struct Msg {
    pub kind: i32,
    pub from: i32,
    pub to: i32,
    pub data: String,
}

// ---------------------------------------------------------------- frames

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn frame(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(body.len() + 5);
    f.extend_from_slice(&(body.len() as u32 + 1).to_le_bytes());
    f.push(kind);
    f.extend_from_slice(body);
    f
}

fn ints(v: &[i32]) -> Vec<u8> {
    v.iter().flat_map(|i| i.to_le_bytes()).collect()
}

fn msg_frame(m: &Msg) -> Vec<u8> {
    let mut b = ints(&[m.kind, m.from, m.to]);
    put_str(&mut b, &m.data);
    frame(F_MSG, &b)
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn int(&mut self) -> i32 {
        let v = self.0.get(..4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()));
        self.0 = self.0.get(4..).unwrap_or(&[]);
        v
    }
    fn string(&mut self) -> String {
        let n = self.int().max(0) as usize;
        let s = String::from_utf8_lossy(self.0.get(..n).unwrap_or(&[])).into_owned();
        self.0 = self.0.get(n..).unwrap_or(&[]);
        s
    }
}

/// A non-blocking TCP connection that collects whole frames.
struct Conn {
    sock: TcpStream,
    buf: Vec<u8>,
    closed: bool,
}

impl Conn {
    fn new(sock: TcpStream) -> Conn {
        let _ = sock.set_nodelay(true);
        let _ = sock.set_nonblocking(true);
        Conn { sock, buf: Vec::new(), closed: false }
    }

    fn send(&mut self, f: &[u8]) {
        // frames are small; switch to blocking for the write so none is cut short
        let _ = self.sock.set_nonblocking(false);
        if self.sock.write_all(f).is_err() {
            self.closed = true;
        }
        let _ = self.sock.set_nonblocking(true);
    }

    /// Reads what has arrived and returns complete frames (kind, body).
    fn poll(&mut self) -> Vec<(u8, Vec<u8>)> {
        let mut tmp = [0u8; 8192];
        loop {
            match self.sock.read(&mut tmp) {
                Ok(0) => {
                    self.closed = true;
                    break;
                }
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.closed = true;
                    break;
                }
            }
        }
        let mut frames = Vec::new();
        while self.buf.len() >= 4 {
            let len = u32::from_le_bytes(self.buf[..4].try_into().unwrap()) as usize;
            if len == 0 || len > 16 << 20 {
                self.closed = true;
                break;
            }
            if self.buf.len() < 4 + len {
                break;
            }
            let kind = self.buf[4];
            let body = self.buf[5..4 + len].to_vec();
            self.buf.drain(..4 + len);
            frames.push((kind, body));
        }
        frames
    }

    /// Waits up to `timeout` for a frame of `kind` (other frames are returned too).
    fn wait_for(&mut self, kind: u8, timeout: Duration) -> Vec<(u8, Vec<u8>)> {
        let end = Instant::now() + timeout;
        let mut got = Vec::new();
        while Instant::now() < end && !self.closed {
            let frames = self.poll();
            let found = frames.iter().any(|(k, _)| *k == kind);
            got.extend(frames);
            if found {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        got
    }
}

// ---------------------------------------------------------------- the game

struct Client {
    conn: Conn,
    players: Vec<i32>,
}

pub struct NetGame {
    pub name: String,
    hosting: bool,
    listener: Option<TcpListener>,
    discovery: Option<UdpSocket>,
    /// The host's side: one entry per joined computer.
    clients: Vec<Client>,
    /// A client's connection to the host.
    host_conn: Option<Conn>,
    /// Every player in the game: id -> name.
    players: HashMap<i32, String>,
    local: Vec<i32>,
    next_id: i32,
    next_request: i32,
    inbox: VecDeque<Msg>,
    lost: bool,
}

impl NetGame {
    fn new(name: &str, hosting: bool) -> NetGame {
        NetGame {
            name: name.to_string(),
            hosting,
            listener: None,
            discovery: None,
            clients: Vec::new(),
            host_conn: None,
            players: HashMap::new(),
            local: Vec::new(),
            next_id: 1,
            next_request: 1,
            inbox: VecDeque::new(),
            lost: false,
        }
    }

    /// `HostNetGame`: listens on `port` (0 = any free port).
    pub fn host(name: &str, port: u16) -> Option<NetGame> {
        let listener = TcpListener::bind(("0.0.0.0", port)).ok()?;
        listener.set_nonblocking(true).ok()?;
        let mut g = NetGame::new(name, true);
        g.listener = Some(listener);
        // LAN discovery is optional: another game may hold the port
        if let Ok(u) = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)) {
            let _ = u.set_nonblocking(true);
            g.discovery = Some(u);
        }
        Some(g)
    }

    pub fn port(&self) -> u16 {
        self.listener.as_ref().and_then(|l| l.local_addr().ok()).map_or(0, |a| a.port())
    }

    pub fn is_host(&self) -> bool {
        self.hosting
    }

    /// `JoinNetGame` with an address ("ip" or "ip:port").
    pub fn join(name: &str, addr: &str) -> Option<NetGame> {
        let target: SocketAddr = if addr.contains(':') {
            addr.parse().ok()?
        } else {
            std::net::ToSocketAddrs::to_socket_addrs(&(addr, DEFAULT_PORT)).ok()?.next()?
        };
        let sock = TcpStream::connect_timeout(&target, Duration::from_secs(3)).ok()?;
        let mut g = NetGame::new(name, false);
        g.host_conn = Some(Conn::new(sock));
        // the host sends the current players straight away
        g.pump();
        Some(g)
    }

    /// Looks for a game on the local network ("" = any game); returns its address.
    pub fn find(name: &str, wait: Duration) -> Option<SocketAddr> {
        let u = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
        u.set_broadcast(true).ok()?;
        u.set_read_timeout(Some(Duration::from_millis(100))).ok()?;
        let query = format!("NEPTUNE_FIND {name}");
        let end = Instant::now() + wait;
        let mut buf = [0u8; 512];
        while Instant::now() < end {
            let _ = u.send_to(query.as_bytes(), ("255.255.255.255", DISCOVERY_PORT));
            let _ = u.send_to(query.as_bytes(), ("127.0.0.1", DISCOVERY_PORT));
            if let Ok((n, from)) = u.recv_from(&mut buf) {
                let reply = String::from_utf8_lossy(&buf[..n]);
                if let Some(rest) = reply.strip_prefix("NEPTUNE_GAME ") {
                    let (port, game) = rest.split_once(' ').unwrap_or((rest, ""));
                    if name.is_empty() || game == name {
                        return Some(SocketAddr::new(from.ip(), port.parse().ok()?));
                    }
                }
            }
        }
        None
    }

    fn deliver_local(&mut self, m: Msg) {
        if m.to == 0 || self.local.contains(&m.to) {
            self.inbox.push_back(m);
        }
    }

    fn broadcast_clients(&mut self, f: &[u8], except: Option<usize>) {
        for (i, c) in self.clients.iter_mut().enumerate() {
            if Some(i) != except {
                c.conn.send(f);
            }
        }
    }

    /// Sends and receives everything waiting (called by every net command).
    pub fn pump(&mut self) {
        if self.hosting {
            self.pump_host();
        } else {
            self.pump_client();
        }
    }

    fn pump_host(&mut self) {
        // answer LAN searches
        if let Some(u) = &self.discovery {
            let mut buf = [0u8; 512];
            while let Ok((n, from)) = u.recv_from(&mut buf) {
                let q = String::from_utf8_lossy(&buf[..n]).into_owned();
                if let Some(want) = q.strip_prefix("NEPTUNE_FIND") {
                    let want = want.trim();
                    if want.is_empty() || want == self.name {
                        let port = self.listener.as_ref().and_then(|l| l.local_addr().ok()).map_or(0, |a| a.port());
                        let _ = u.send_to(format!("NEPTUNE_GAME {port} {}", self.name).as_bytes(), from);
                    }
                }
            }
        }
        // new computers: tell them who is already playing
        if let Some(l) = &self.listener {
            while let Ok((sock, _)) = l.accept() {
                let mut conn = Conn::new(sock);
                for (id, name) in &self.players {
                    let mut b = ints(&[*id]);
                    put_str(&mut b, name);
                    conn.send(&frame(F_JOINED, &b));
                }
                self.clients.push(Client { conn, players: Vec::new() });
            }
        }
        let mut i = 0;
        while i < self.clients.len() {
            let frames = self.clients[i].conn.poll();
            for (kind, body) in frames {
                let mut r = Reader(&body);
                match kind {
                    F_CREATE => {
                        let req = r.int();
                        let name = r.string();
                        let id = self.next_id;
                        self.next_id += 1;
                        self.players.insert(id, name.clone());
                        self.clients[i].players.push(id);
                        self.clients[i].conn.send(&frame(F_CREATED, &ints(&[req, id])));
                        let mut b = ints(&[id]);
                        put_str(&mut b, &name);
                        self.broadcast_clients(&frame(F_JOINED, &b), Some(i));
                        self.inbox.push_back(Msg { kind: MSG_JOINED, from: id, to: 0, data: name });
                    }
                    F_DELETE => {
                        let id = r.int();
                        self.clients[i].players.retain(|p| *p != id);
                        self.remove_player(id, Some(i));
                    }
                    F_MSG => {
                        let m = Msg { kind: r.int(), from: r.int(), to: r.int(), data: r.string() };
                        self.route(m, Some(i));
                    }
                    _ => {}
                }
            }
            if self.clients[i].conn.closed {
                let gone = self.clients.remove(i);
                for id in gone.players {
                    self.remove_player(id, None);
                }
            } else {
                i += 1;
            }
        }
    }

    fn remove_player(&mut self, id: i32, from_client: Option<usize>) {
        if self.players.remove(&id).is_some() {
            self.broadcast_clients(&frame(F_LEFT, &ints(&[id])), from_client);
            self.inbox.push_back(Msg { kind: MSG_LEFT, from: id, to: 0, data: String::new() });
        }
    }

    /// The host delivers a message to its own players and relays it to the
    /// computers whose players it is for.
    fn route(&mut self, m: Msg, from_client: Option<usize>) {
        let f = msg_frame(&m);
        if m.to == 0 {
            self.broadcast_clients(&f, from_client);
        } else if let Some(c) = self.clients.iter_mut().find(|c| c.players.contains(&m.to)) {
            c.conn.send(&f);
        }
        self.deliver_local(m);
    }

    fn pump_client(&mut self) {
        let Some(conn) = self.host_conn.as_mut() else { return };
        let frames = conn.poll();
        let closed = conn.closed;
        for (kind, body) in frames {
            self.client_frame(kind, &body);
        }
        if closed && !self.lost {
            self.lost = true;
            self.inbox.push_back(Msg { kind: MSG_SESSION_LOST, from: 0, to: 0, data: String::new() });
        }
    }

    fn client_frame(&mut self, kind: u8, body: &[u8]) {
        let mut r = Reader(body);
        match kind {
            F_JOINED => {
                let id = r.int();
                let name = r.string();
                self.players.insert(id, name.clone());
                self.inbox.push_back(Msg { kind: MSG_JOINED, from: id, to: 0, data: name });
            }
            F_LEFT => {
                let id = r.int();
                self.players.remove(&id);
                self.inbox.push_back(Msg { kind: MSG_LEFT, from: id, to: 0, data: String::new() });
            }
            F_MSG => {
                let m = Msg { kind: r.int(), from: r.int(), to: r.int(), data: r.string() };
                self.deliver_local(m);
            }
            _ => {}
        }
    }

    /// `CreateNetPlayer`: a player controlled from this computer.
    pub fn create_player(&mut self, name: &str) -> i32 {
        if self.hosting {
            let id = self.next_id;
            self.next_id += 1;
            self.players.insert(id, name.to_string());
            self.local.push(id);
            let mut b = ints(&[id]);
            put_str(&mut b, name);
            self.broadcast_clients(&frame(F_JOINED, &b), None);
            return id;
        }
        let req = self.next_request;
        self.next_request += 1;
        let Some(conn) = self.host_conn.as_mut() else { return 0 };
        let mut b = ints(&[req]);
        put_str(&mut b, name);
        conn.send(&frame(F_CREATE, &b));
        let frames = conn.wait_for(F_CREATED, Duration::from_secs(3));
        let mut id = 0;
        for (kind, body) in frames {
            if kind == F_CREATED {
                let mut r = Reader(&body);
                if r.int() == req {
                    id = r.int();
                }
            } else {
                self.client_frame(kind, &body);
            }
        }
        if id != 0 {
            self.players.insert(id, name.to_string());
            self.local.push(id);
        }
        id
    }

    pub fn delete_player(&mut self, id: i32) {
        self.local.retain(|p| *p != id);
        if self.hosting {
            self.remove_player(id, None);
        } else if let Some(c) = self.host_conn.as_mut() {
            c.send(&frame(F_DELETE, &ints(&[id])));
            self.players.remove(&id);
        }
    }

    pub fn player_name(&self, id: i32) -> String {
        self.players.get(&id).cloned().unwrap_or_default()
    }

    pub fn is_local(&self, id: i32) -> bool {
        self.local.contains(&id)
    }

    /// `SendNetMsg`: to one player, or to everyone (`to` = 0) but the sender.
    pub fn send(&mut self, kind: i32, data: &str, from: i32, to: i32) -> bool {
        let m = Msg { kind, from, to, data: data.to_string() };
        if self.hosting {
            let f = msg_frame(&m);
            if to == 0 {
                self.broadcast_clients(&f, None);
                // other players on this computer get it too
                if self.local.iter().any(|p| *p != from) {
                    self.inbox.push_back(m);
                }
            } else if self.local.contains(&to) {
                self.inbox.push_back(m);
            } else if let Some(c) = self.clients.iter_mut().find(|c| c.players.contains(&to)) {
                c.conn.send(&f);
            }
            true
        } else {
            match self.host_conn.as_mut() {
                Some(c) if !c.closed => {
                    c.send(&msg_frame(&m));
                    !c.closed
                }
                _ => false,
            }
        }
    }

    /// `RecvNetMsg`: the next message for this computer's players.
    pub fn recv(&mut self) -> Option<Msg> {
        self.pump();
        self.inbox.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_msg(g: &mut NetGame) -> Msg {
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(m) = g.recv() {
                return m;
            }
            assert!(Instant::now() < end, "no message arrived");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn host_and_two_clients() {
        let mut host = NetGame::host("test", 0).unwrap();
        let addr = format!("127.0.0.1:{}", host.port());
        let alice = host.create_player("Alice");

        let mut c1 = NetGame::join("test", &addr).unwrap();
        host.pump();
        // the client learns about Alice
        let m = wait_msg(&mut c1);
        assert_eq!((m.kind, m.from, m.data.as_str()), (MSG_JOINED, alice, "Alice"));

        // creating a player needs the host to answer; run it on a thread
        let h = std::thread::spawn(move || {
            let bob = c1.create_player("Bob");
            (c1, bob)
        });
        let start = Instant::now();
        while !h.is_finished() && start.elapsed() < Duration::from_secs(3) {
            host.pump();
            std::thread::sleep(Duration::from_millis(2));
        }
        let (mut c1, bob) = h.join().unwrap();
        assert!(bob > 0 && c1.is_local(bob) && !c1.is_local(alice));
        let m = wait_msg(&mut host);
        assert_eq!((m.kind, m.from, m.data.as_str()), (MSG_JOINED, bob, "Bob"));
        assert_eq!(host.player_name(bob), "Bob");

        // messages both ways, to one player and to everyone
        assert!(c1.send(7, "hello alice", bob, alice));
        let m = wait_msg(&mut host);
        assert_eq!((m.kind, m.from, m.to, m.data.as_str()), (7, bob, alice, "hello alice"));
        host.send(9, "to everyone", alice, 0);
        let m = wait_msg(&mut c1);
        assert_eq!((m.kind, m.from, m.data.as_str()), (9, alice, "to everyone"));

        // a second computer joins, and then the first leaves
        let mut c2 = NetGame::join("test", &addr).unwrap();
        host.pump();
        let names: Vec<String> = (0..2).map(|_| wait_msg(&mut c2).data).collect();
        assert!(names.contains(&"Alice".to_string()) && names.contains(&"Bob".to_string()));
        drop(c1);
        let m = wait_msg(&mut host);
        assert_eq!((m.kind, m.from), (MSG_LEFT, bob));
        let m = wait_msg(&mut c2);
        assert_eq!((m.kind, m.from), (MSG_LEFT, bob));

        // the host closes: the client's session is lost
        drop(host);
        let m = wait_msg(&mut c2);
        assert_eq!(m.kind, MSG_SESSION_LOST);
    }
}
