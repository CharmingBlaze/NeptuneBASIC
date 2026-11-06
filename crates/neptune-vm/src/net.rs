//! TCP and UDP streams (`OpenTCPStream`, `CreateTCPServer`, `CreateUDPStream`,
//! ...) on `std::net`, following Blitz3D's `bbsockets.cpp`.
//!
//! TCP streams are ordinary streams: `ReadLine`, `WriteInt`, `Eof`,
//! `ReadAvail` and the rest work on them. UDP streams collect written bytes
//! until `SendUDPMsg` sends them as one datagram, and `RecvUDPMsg` loads the
//! next datagram for the read commands.
//!
//! IP addresses are 32-bit integers, most significant byte first
//! (`DottedIP` turns one into "a.b.c.d").

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

pub fn ip_to_int(ip: IpAddr) -> i32 {
    match ip {
        IpAddr::V4(v4) => u32::from(v4) as i32,
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(0, |v4| u32::from(v4) as i32),
    }
}

pub fn int_to_ip(ip: i32) -> Ipv4Addr {
    Ipv4Addr::from(ip as u32)
}

pub fn dotted(ip: i32) -> String {
    int_to_ip(ip).to_string()
}

/// All IPv4 addresses of a host name ("" = this computer).
pub fn host_ips(host: &str) -> Vec<i32> {
    let name = if host.is_empty() { "localhost" } else { host };
    let mut ips: Vec<i32> = (name, 0)
        .to_socket_addrs()
        .map(|it| it.filter(|a| a.is_ipv4()).map(|a| ip_to_int(a.ip())).collect())
        .unwrap_or_default();
    ips.dedup();
    ips
}

pub struct Tcp {
    pub sock: TcpStream,
    pub peer: SocketAddr,
    /// Read timeout set by `TCPTimeouts`.
    pub closed: bool,
}

impl Tcp {
    pub fn connect(host: &str, port: u16, timeout_ms: u32) -> Option<Tcp> {
        let addr = (host, port).to_socket_addrs().ok()?.find(SocketAddr::is_ipv4)?;
        let sock = TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms.max(1) as u64)).ok()?;
        Tcp::from_socket(sock, timeout_ms)
    }

    pub fn from_socket(sock: TcpStream, read_timeout_ms: u32) -> Option<Tcp> {
        let peer = sock.peer_addr().ok()?;
        let _ = sock.set_nodelay(true);
        let _ = sock.set_read_timeout(Some(Duration::from_millis(read_timeout_ms.max(1) as u64)));
        Some(Tcp { sock, peer, closed: false })
    }

    pub fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.sock.read(buf) {
            Ok(0) => {
                self.closed = true;
                Ok(0)
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(0),
            other => other,
        }
    }

    pub fn write(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.sock.write_all(buf)
    }

    /// Bytes that can be read without waiting.
    pub fn available(&mut self) -> i32 {
        let mut buf = [0u8; 65536];
        if self.sock.set_nonblocking(true).is_err() {
            return 0;
        }
        let n = match self.sock.peek(&mut buf) {
            Ok(0) => {
                self.closed = true;
                0
            }
            Ok(n) => n as i32,
            Err(_) => 0,
        };
        let _ = self.sock.set_nonblocking(false);
        n
    }

    /// Blitz `Eof` on a TCP stream: 1 when the other side closed it.
    pub fn eof(&mut self) -> i32 {
        if !self.closed {
            self.available();
        }
        self.closed as i32
    }
}

pub struct Server {
    pub listener: TcpListener,
}

impl Server {
    pub fn bind(port: u16) -> Option<Server> {
        let listener = TcpListener::bind(("0.0.0.0", port)).ok()?;
        listener.set_nonblocking(true).ok()?;
        Some(Server { listener })
    }

    /// Waits up to `timeout_ms` for a client.
    pub fn accept(&self, timeout_ms: u32, read_timeout_ms: u32) -> Option<Tcp> {
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms as u64);
        loop {
            match self.listener.accept() {
                Ok((sock, _)) => {
                    let _ = sock.set_nonblocking(false);
                    return Tcp::from_socket(sock, read_timeout_ms);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(_) => return None,
            }
        }
    }
}

pub struct Udp {
    pub sock: UdpSocket,
    /// Received datagram being read, and the read position.
    pub rbuf: Vec<u8>,
    pub rpos: usize,
    /// Bytes written since the last `SendUDPMsg`.
    pub wbuf: Vec<u8>,
    /// Sender of the last datagram received.
    pub msg_from: Option<SocketAddr>,
}

impl Udp {
    pub fn bind(port: u16, timeout_ms: u32) -> Option<Udp> {
        let sock = UdpSocket::bind(("0.0.0.0", port)).ok()?;
        let _ = sock.set_read_timeout(Some(Duration::from_millis(timeout_ms.max(1) as u64)));
        let _ = sock.set_broadcast(true);
        Some(Udp { sock, rbuf: Vec::new(), rpos: 0, wbuf: Vec::new(), msg_from: None })
    }

    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let n = buf.len().min(self.rbuf.len() - self.rpos);
        buf[..n].copy_from_slice(&self.rbuf[self.rpos..self.rpos + n]);
        self.rpos += n;
        n
    }

    pub fn available(&self) -> i32 {
        (self.rbuf.len() - self.rpos) as i32
    }

    /// `SendUDPMsg`: sends the written bytes; port 0 reuses this stream's port.
    pub fn send(&mut self, ip: i32, port: u16) -> bool {
        let port = if port == 0 { self.sock.local_addr().map(|a| a.port()).unwrap_or(0) } else { port };
        let ok = self.sock.send_to(&self.wbuf, (int_to_ip(ip), port)).is_ok();
        self.wbuf.clear();
        ok
    }

    /// `RecvUDPMsg`: loads the next datagram; returns the sender IP or 0.
    pub fn recv(&mut self) -> i32 {
        let mut buf = vec![0u8; 65536];
        match self.sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                buf.truncate(n);
                self.rbuf = buf;
                self.rpos = 0;
                self.msg_from = Some(from);
                ip_to_int(from.ip())
            }
            Err(_) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_conversions() {
        let ip = ip_to_int(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)));
        assert_eq!(dotted(ip), "192.168.1.20");
        assert_eq!(ip as u32, 0xC0A8_0114);
        assert!(host_ips("127.0.0.1").contains(&ip_to_int(IpAddr::V4(Ipv4Addr::LOCALHOST))));
    }

    #[test]
    fn tcp_round_trip() {
        let server = Server::bind(0).unwrap();
        let port = server.listener.local_addr().unwrap().port();
        let mut client = Tcp::connect("127.0.0.1", port, 2000).unwrap();
        let mut conn = server.accept(2000, 2000).unwrap();
        client.write(b"hello").unwrap();
        let mut buf = [0u8; 5];
        let mut got = 0;
        while got < 5 {
            got += conn.read(&mut buf[got..]).unwrap();
        }
        assert_eq!(&buf, b"hello");
        drop(client);
        // the closed connection reads as end of file
        let mut tries = 0;
        while conn.eof() == 0 && tries < 100 {
            tries += 1;
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(conn.eof(), 1);
    }

    #[test]
    fn udp_round_trip() {
        let mut a = Udp::bind(0, 2000).unwrap();
        let mut b = Udp::bind(0, 2000).unwrap();
        let bport = b.sock.local_addr().unwrap().port();
        a.wbuf.extend_from_slice(&[1, 2, 3]);
        assert!(a.send(ip_to_int(IpAddr::V4(Ipv4Addr::LOCALHOST)), bport));
        assert_eq!(dotted(b.recv()), "127.0.0.1");
        let mut buf = [0u8; 3];
        assert_eq!(b.read(&mut buf), 3);
        assert_eq!(buf, [1, 2, 3]);
        assert_eq!(b.available(), 0);
    }
}
