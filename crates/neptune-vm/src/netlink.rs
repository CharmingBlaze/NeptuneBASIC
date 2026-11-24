//! Carries a multiplayer [`Session`] over iroh: encrypted QUIC connections
//! that cross home routers, with a relay as the fallback.
//!
//! Everything runs on a background thread, and the game only touches the
//! session under a short lock, so a slow network can never stall a frame.
//! The host answers LAN searches on a UDP port; an invite is the host's
//! public key, which works from anywhere.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex, MutexGuard};
use std::time::Duration;

use iroh::endpoint::{presets, Connection, IdleTimeout, QuicTransportConfig, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{interval, timeout, MissedTickBehavior};

use crate::netgame::{frame, Framer, Link, PeerId, Plan, Session, Status, DISCOVERY_PORT, F_BIG_DATAGRAM, HOST, PROTOCOL};

const ALPN: &[u8] = b"neptune/net/2";
/// A computer this far behind on reliable traffic is disconnected.
const SEND_QUEUE: usize = 8192;
const SEARCH_TIME: Duration = Duration::from_millis(1500);
const CONNECT_TIME: Duration = Duration::from_secs(5);
const INVITE_CONNECT_TIME: Duration = Duration::from_secs(20);
/// How long a vanished computer goes unnoticed.
const IDLE_TIME: Duration = Duration::from_secs(8);
const KEEP_ALIVE: Duration = Duration::from_secs(2);

type Shared = Arc<Mutex<Session>>;

fn lock(s: &Shared) -> MutexGuard<'_, Session> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

struct Peer {
    conn: Connection,
    frames: mpsc::Sender<Vec<u8>>,
}

/// The connected computers.
#[derive(Clone, Default)]
pub struct Peers(Arc<Mutex<HashMap<PeerId, Peer>>>);

impl Peers {
    fn map(&self) -> MutexGuard<'_, HashMap<PeerId, Peer>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct IrohLink(pub Peers);

impl Link for IrohLink {
    fn send(&mut self, peer: PeerId, f: Vec<u8>) {
        if let Some(p) = self.0.map().get(&peer) {
            if p.frames.try_send(f).is_err() {
                // it has stopped reading: lose it rather than hold up the game
                p.conn.close(1u32.into(), b"too far behind");
            }
        }
    }

    fn send_datagram(&mut self, peer: PeerId, data: Vec<u8>) {
        let map = self.0.map();
        let Some(p) = map.get(&peer) else { return };
        if p.conn.max_datagram_size().is_some_and(|max| data.len() <= max) {
            let _ = p.conn.send_datagram(data.into());
        } else {
            let _ = p.frames.try_send(frame(F_BIG_DATAGRAM, &data));
        }
    }

    fn kick(&mut self, peer: PeerId) {
        if let Some(p) = self.0.map().get(&peer) {
            // an empty frame tells the writer to finish and hang up
            let _ = p.frames.try_send(Vec::new());
        }
    }
}

/// The network thread. Dropping it says goodbye to the other computers.
pub struct Thread {
    stop: Option<oneshot::Sender<()>>,
    done: std_mpsc::Receiver<()>,
}

impl Drop for Thread {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
            let _ = self.done.recv_timeout(Duration::from_millis(500));
        }
    }
}

pub fn start(plan: Plan, session: Shared, peers: Peers, relay: Option<String>) -> Option<Thread> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().thread_name("neptune-net").build().ok()?;
    let (stop, stopped) = oneshot::channel();
    let (done_tx, done) = std_mpsc::channel();
    std::thread::Builder::new()
        .name("neptune-net-main".into())
        .spawn(move || {
            rt.block_on(run(plan, session, peers, relay, stopped));
            rt.shutdown_background();
            let _ = done_tx.send(());
        })
        .ok()?;
    Some(Thread { stop: Some(stop), done })
}

async fn run(plan: Plan, session: Shared, peers: Peers, relay: Option<String>, mut stopped: oneshot::Receiver<()>) {
    let ep = match bind(relay).await {
        Ok(ep) => ep,
        Err(e) => {
            lock(&session).fail(&e);
            return;
        }
    };
    // traffic delayed by NetSimulate
    let ticker = session.clone();
    tokio::spawn(async move {
        let mut t = interval(Duration::from_millis(2));
        t.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            t.tick().await;
            lock(&ticker).tick();
        }
    });
    tokio::select! {
        _ = drive(plan, ep.clone(), session.clone(), peers.clone()) => { let _ = (&mut stopped).await; }
        _ = &mut stopped => {}
    }
    for p in peers.map().values() {
        p.conn.close(0u32.into(), b"bye");
    }
    let _ = timeout(Duration::from_millis(300), ep.close()).await;
}

async fn bind(relay: Option<String>) -> Result<Endpoint, String> {
    let config = QuicTransportConfig::builder()
        .max_idle_timeout(Some(IdleTimeout::try_from(IDLE_TIME).map_err(|e| e.to_string())?))
        .keep_alive_interval(KEEP_ALIVE)
        .build();
    let mut b = Endpoint::builder(presets::N0).alpns(vec![ALPN.to_vec()]).transport_config(config);
    if let Some(url) = relay {
        let url: RelayUrl = url.parse().map_err(|_| "Relay address is not valid".to_string())?;
        b = b.relay_mode(RelayMode::Custom(url.into()));
    }
    b.bind().await.map_err(|e| format!("Could not open the network: {e}"))
}

/// Starts the game the plan asks for. Returns when there is nothing more to
/// set up, or never if hosting.
async fn drive(plan: Plan, ep: Endpoint, session: Shared, peers: Peers) {
    let name = lock(&session).name.clone();
    match plan {
        Plan::Host => host(ep, session, peers).await,
        Plan::Join(target) => {
            let wait = if parse_invite(&target).is_some() { INVITE_CONNECT_TIME } else { CONNECT_TIME };
            let Some(addr) = resolve(&ep, &name, &target).await else {
                let reason = if target.is_empty() { "No game was found on the local network" } else { "No game was found at that address" };
                lock(&session).fail(reason);
                return;
            };
            if let Err(e) = connect(&ep, addr, wait, session.clone(), peers).await {
                lock(&session).fail(&e);
            }
        }
        Plan::Auto => {
            // so two copies started together are unlikely to both host
            let jitter = Duration::from_millis(std::process::id() as u64 % 500);
            if let Some(addr) = search(&ep, &name, None, SEARCH_TIME + jitter).await {
                if connect(&ep, addr, CONNECT_TIME, session.clone(), peers.clone()).await.is_ok() {
                    return;
                }
            }
            host(ep, session, peers).await;
        }
    }
}

fn parse_invite(s: &str) -> Option<EndpointId> {
    EndpointId::from_z32(s.trim()).ok()
}

async fn resolve(ep: &Endpoint, name: &str, target: &str) -> Option<EndpointAddr> {
    if target.is_empty() {
        return search(ep, name, None, SEARCH_TIME).await;
    }
    if let Some(id) = parse_invite(target) {
        return Some(EndpointAddr::new(id));
    }
    // an address: ask that computer who it is
    let spec = if target.contains(':') { target.to_string() } else { format!("{target}:{DISCOVERY_PORT}") };
    let there = tokio::net::lookup_host(spec).await.ok()?.find(SocketAddr::is_ipv4)?;
    search(ep, name, Some(there), SEARCH_TIME).await
}

/// Finds a game by broadcasting on the local network, or by asking `there`.
async fn search(ep: &Endpoint, name: &str, there: Option<SocketAddr>, wait: Duration) -> Option<EndpointAddr> {
    let sock = UdpSocket::bind(("0.0.0.0", 0)).await.ok()?;
    sock.set_broadcast(true).ok()?;
    let query = format!("NEPTUNE_FIND {PROTOCOL} {name}");
    let end = tokio::time::Instant::now() + wait;
    let mut buf = [0u8; 512];
    let mut every = interval(Duration::from_millis(150));
    loop {
        tokio::select! {
            _ = every.tick() => {
                if tokio::time::Instant::now() >= end {
                    return None;
                }
                for t in targets(ep, there) {
                    let _ = sock.send_to(query.as_bytes(), t).await;
                }
            }
            got = sock.recv_from(&mut buf) => {
                let Ok((n, from)) = got else { continue };
                if let Some(addr) = parse_reply(&String::from_utf8_lossy(&buf[..n]), name, from) {
                    return Some(addr);
                }
            }
        }
    }
}

/// Where to ask: this computer, the broadcast address, and each network
/// this computer is on (the plain broadcast only reaches one of them).
fn targets(ep: &Endpoint, there: Option<SocketAddr>) -> Vec<SocketAddr> {
    if let Some(t) = there {
        return vec![t];
    }
    let mut v: Vec<SocketAddr> = vec![(std::net::Ipv4Addr::BROADCAST, DISCOVERY_PORT).into(), (std::net::Ipv4Addr::LOCALHOST, DISCOVERY_PORT).into()];
    for a in ep.addr().ip_addrs() {
        if let std::net::IpAddr::V4(ip) = a.ip() {
            if ip.is_private() {
                let [a, b, c, _] = ip.octets();
                v.push((std::net::Ipv4Addr::new(a, b, c, 255), DISCOVERY_PORT).into());
            }
        }
    }
    v
}

fn parse_reply(reply: &str, name: &str, from: SocketAddr) -> Option<EndpointAddr> {
    let mut parts = reply.strip_prefix("NEPTUNE_GAME ")?.splitn(3, ' ');
    let port: u16 = parts.next()?.parse().ok()?;
    let id = EndpointId::from_z32(parts.next()?).ok()?;
    let game = parts.next().unwrap_or("");
    (name.is_empty() || game == name).then(|| EndpointAddr::new(id).with_ip_addr(SocketAddr::new(from.ip(), port)))
}

async fn connect(ep: &Endpoint, addr: EndpointAddr, wait: Duration, session: Shared, peers: Peers) -> Result<(), String> {
    let conn = match timeout(wait, ep.connect(addr, ALPN)).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => return Err(format!("Could not connect: {e}")),
        Err(_) => return Err("Could not reach that game".into()),
    };
    let (send, recv) = conn.open_bi().await.map_err(|e| format!("Could not connect: {e}"))?;
    let c = conn.clone();
    let pinger = session.clone();
    tokio::spawn(async move {
        let mut t = interval(Duration::from_millis(500));
        loop {
            t.tick().await;
            if c.close_reason().is_some() {
                return;
            }
            let best = c.paths().iter().map(|p| p.rtt()).min();
            if let Some(rtt) = best {
                let mut s = lock(&pinger);
                s.ping_ms = rtt.as_millis() as i32 + s.simulated_lag_ms();
            }
        }
    });
    // the session stays a client from here on, even if the connection drops
    tokio::spawn(serve(conn, send, recv, HOST, session, peers, true));
    Ok(())
}

async fn host(ep: Endpoint, session: Shared, peers: Peers) {
    let invite = ep.id().to_z32();
    lock(&session).start_host(&invite);
    let name = lock(&session).name.clone();
    let port = ep.bound_sockets().iter().find(|a| a.is_ipv4()).map_or(0, |a| a.port());
    tokio::spawn(answer_searches(name, port, invite));

    static NEXT_PEER: AtomicU32 = AtomicU32::new(1);
    while let Some(incoming) = ep.accept().await {
        let (session, peers) = (session.clone(), peers.clone());
        tokio::spawn(async move {
            let Ok(conn) = incoming.await else { return };
            let Ok(Ok((send, recv))) = timeout(CONNECT_TIME, conn.accept_bi()).await else { return };
            let peer = NEXT_PEER.fetch_add(1, Ordering::Relaxed);
            lock(&session).peer_connected(peer);
            serve(conn, send, recv, peer, session, peers, false).await;
        });
    }
}

/// LAN search. Optional: another game on this computer may hold the port.
async fn answer_searches(name: String, port: u16, invite: String) {
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await else { return };
    let mut buf = [0u8; 512];
    let prefix = format!("NEPTUNE_FIND {PROTOCOL}");
    loop {
        let Ok((n, from)) = sock.recv_from(&mut buf).await else { continue };
        let q = String::from_utf8_lossy(&buf[..n]).into_owned();
        if let Some(want) = q.strip_prefix(&prefix) {
            let want = want.trim();
            if want.is_empty() || want == name {
                let _ = sock.send_to(format!("NEPTUNE_GAME {port} {invite} {name}").as_bytes(), from).await;
            }
        }
    }
}

/// Runs one connection until it ends.
async fn serve(conn: Connection, mut send: SendStream, mut recv: RecvStream, peer: PeerId, session: Shared, peers: Peers, is_client: bool) {
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(SEND_QUEUE);
    peers.map().insert(peer, Peer { conn: conn.clone(), frames: tx });
    if is_client {
        lock(&session).connected();
    }

    let closer = conn.clone();
    let writer = tokio::spawn(async move {
        while let Some(f) = rx.recv().await {
            if f.is_empty() {
                let _ = send.finish();
                let _ = timeout(Duration::from_secs(1), closer.closed()).await;
                closer.close(0u32.into(), b"refused");
                return;
            }
            if send.write_all(&f).await.is_err() {
                return;
            }
        }
    });

    let (dconn, dsession) = (conn.clone(), session.clone());
    let datagrams = tokio::spawn(async move {
        while let Ok(d) = dconn.read_datagram().await {
            lock(&dsession).datagram_in(peer, d.to_vec());
        }
    });

    let mut framer = Framer::default();
    let mut buf = vec![0u8; 16 * 1024];
    let reason = loop {
        match recv.read(&mut buf).await {
            Ok(Some(n)) => {
                let Some(frames) = framer.push(&buf[..n]) else {
                    conn.close(2u32.into(), b"bad data");
                    break "The other computer sent bad data".to_string();
                };
                let mut s = lock(&session);
                for (kind, body) in frames {
                    s.frame_in(peer, kind, body);
                }
            }
            Ok(None) => break "The connection was closed".to_string(),
            Err(_) => break "The connection was lost".to_string(),
        }
    };
    writer.abort();
    datagrams.abort();
    peers.map().remove(&peer);
    let mut s = lock(&session);
    if !matches!(s.status, Status::Failed(_)) {
        s.peer_gone(peer, &reason);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netgame::{NetGame, Options};
    use std::time::Instant;

    /// These tests share the LAN search port, so they take turns.
    static PORT: Mutex<()> = Mutex::new(());

    fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(15);
        while !ok() {
            assert!(Instant::now() < end, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A real connection through iroh, on this computer.
    #[test]
    fn two_games_talk_over_the_network() {
        let _turn = PORT.lock().unwrap_or_else(|e| e.into_inner());
        let opts = Options::default();
        let host = NetGame::start(Plan::Host, "linktest", Some("Alice"), &opts).unwrap();
        wait_until("hosting", || host.with(|s| s.status == Status::Hosting));
        let alice = host.with(|s| s.me);
        let invite = host.with(|s| s.invite.clone());
        assert_eq!(invite.len(), 52);

        // by address, as JoinNetGame does: needs no internet
        let client = NetGame::start(Plan::Join("127.0.0.1".into()), "linktest", Some("Bob"), &opts).unwrap();
        wait_until("joining", || client.with(|s| s.status == Status::Joined && s.me != 0));
        let bob = client.with(|s| s.me);
        assert_eq!(client.with(|s| s.invite.clone()), invite);
        assert_eq!(client.with(|s| s.host_player), alice);

        // reliable
        assert!(client.with(|s| s.send(7, "hello", bob, alice, true)));
        wait_until("a reliable message", || host.with(|s| {
            std::iter::from_fn(|| s.recv()).any(|m| m.kind == 7 && m.data == "hello")
        }));

        // a datagram can be lost, so keep sending until one gets through
        wait_until("a datagram", || {
            client.with(|s| s.send(8, "fast", bob, alice, false));
            std::thread::sleep(Duration::from_millis(5));
            host.with(|s| std::iter::from_fn(|| s.recv()).any(|m| m.kind == 8))
        });

        // a datagram too large for one packet still arrives
        let big = "x".repeat(4000);
        client.with(|s| s.send(9, &big, bob, alice, false));
        wait_until("a large datagram", || host.with(|s| {
            std::iter::from_fn(|| s.recv()).any(|m| m.kind == 9 && m.data.len() == 4000)
        }));

        // leaving is noticed at once, not after the idle timeout
        let t = Instant::now();
        drop(host);
        wait_until("the session to be lost", || client.with(|s| s.status == Status::Lost));
        assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
    }

    #[test]
    fn a_search_only_finds_the_named_game() {
        let _turn = PORT.lock().unwrap_or_else(|e| e.into_inner());
        let opts = Options::default();
        let host = NetGame::start(Plan::Host, "game-a", None, &opts).unwrap();
        wait_until("hosting", || host.with(|s| s.status == Status::Hosting));
        let lan_only = NetGame::start(Plan::Join("127.0.0.1".into()), "game-b", None, &opts).unwrap();
        wait_until("no game of that name", || lan_only.with(|s| matches!(s.status, Status::Failed(_))));
    }

    #[test]
    fn auto_joins_the_same_game() {
        let _turn = PORT.lock().unwrap_or_else(|e| e.into_inner());
        let opts = Options::default();
        let first = NetGame::start(Plan::Auto, "auto-test", Some("One"), &opts).unwrap();
        wait_until("the first to host", || first.with(|s| s.status == Status::Hosting));
        let second = NetGame::start(Plan::Auto, "auto-test", Some("Two"), &opts).unwrap();
        wait_until("the second to join", || second.with(|s| s.status == Status::Joined && s.me != 0));
        wait_until("the host to see both", || first.with(|s| s.player_count() == 2));
        drop(second);
        wait_until("the host to see one leave", || first.with(|s| s.player_count() == 1));
    }

    /// Needs the internet: `cargo test -p neptune-vm --features online -- --ignored`.
    #[test]
    #[ignore]
    fn an_invite_works_with_nothing_but_the_code() {
        let _turn = PORT.lock().unwrap_or_else(|e| e.into_inner());
        let opts = Options::default();
        let host = NetGame::start(Plan::Host, "invite-test", Some("Host"), &opts).unwrap();
        wait_until("hosting", || host.with(|s| s.status == Status::Hosting));
        // the host needs a moment to publish how to reach it
        std::thread::sleep(Duration::from_secs(3));
        let invite = host.with(|s| s.invite.clone());
        let guest = NetGame::start(Plan::Join(invite), "", Some("Guest"), &opts).unwrap();
        wait_until("joining by invite", || guest.with(|s| s.status == Status::Joined && s.me != 0));
    }
}
