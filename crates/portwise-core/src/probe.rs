//! Bind probes, free-port search and wait-for-port. Probes see *every* listener (including ones
//! owned by root or other users, and container port-forwards) because they ask the kernel
//! directly whether the port can be bound.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

/// Result of trying to bind a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeResult {
    /// Bindable on every local address we tried.
    Free,
    /// Something already holds it (`EADDRINUSE`).
    InUse,
    /// The OS refused (privileged port, or a Windows excluded port range: WSAEACCES).
    Denied,
}

const WSAEACCES: i32 = 10013;

fn classify(e: &io::Error) -> Option<ProbeResult> {
    match e.kind() {
        io::ErrorKind::AddrInUse => Some(ProbeResult::InUse),
        io::ErrorKind::PermissionDenied => Some(ProbeResult::Denied),
        // IPv6 disabled etc. — not an answer about this port.
        io::ErrorKind::AddrNotAvailable | io::ErrorKind::Unsupported => None,
        _ if e.raw_os_error() == Some(WSAEACCES) => Some(ProbeResult::Denied),
        _ => None,
    }
}

fn addrs(port: u16) -> [SocketAddr; 4] {
    [
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port),
    ]
}

/// Can a TCP listener bind `port` on 0.0.0.0, 127.0.0.1, [::] and [::1]?
pub fn probe_tcp(port: u16) -> ProbeResult {
    let mut result = ProbeResult::Free;
    for a in addrs(port) {
        if let Err(e) = TcpListener::bind(a) {
            match classify(&e) {
                Some(ProbeResult::InUse) => return ProbeResult::InUse,
                Some(ProbeResult::Denied) => result = ProbeResult::Denied,
                _ => {}
            }
        }
    }
    result
}

/// Same as [`probe_tcp`] for UDP.
pub fn probe_udp(port: u16) -> ProbeResult {
    let mut result = ProbeResult::Free;
    for a in addrs(port) {
        if let Err(e) = UdpSocket::bind(a) {
            match classify(&e) {
                Some(ProbeResult::InUse) => return ProbeResult::InUse,
                Some(ProbeResult::Denied) => result = ProbeResult::Denied,
                _ => {}
            }
        }
    }
    result
}

/// Is something accepting TCP connections on localhost:`port` (v4 or v6)?
pub fn tcp_accepting(port: u16) -> bool {
    [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ]
    .into_iter()
    .any(|ip| {
        TcpStream::connect_timeout(&SocketAddr::new(ip, port), Duration::from_millis(250)).is_ok()
    })
}

/// Ask the OS for an ephemeral free TCP port.
pub fn ephemeral_port() -> io::Result<u16> {
    Ok(TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?
        .local_addr()?
        .port())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_in_use_and_free() {
        let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        assert_eq!(probe_tcp(port), ProbeResult::InUse);
        assert!(tcp_accepting(port));
        drop(l);
        assert_eq!(probe_tcp(port), ProbeResult::Free);
        let p = ephemeral_port().unwrap();
        assert!(p > 0);
    }

    #[test]
    fn udp_probe() {
        let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = s.local_addr().unwrap().port();
        assert_eq!(probe_udp(port), ProbeResult::InUse);
    }
}
