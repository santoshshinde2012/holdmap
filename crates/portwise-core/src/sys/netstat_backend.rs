//! Socket enumeration for macOS (libproc) and Windows (iphlpapi) via the `netstat2` crate.

use crate::model::{Family, Protocol, RawSocket, SocketState};
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use std::io;
use std::net::IpAddr;

pub fn list_sockets() -> io::Result<Vec<RawSocket>> {
    let infos = get_sockets_info(
        AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6,
        ProtocolFlags::TCP | ProtocolFlags::UDP,
    )
    .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(infos
        .into_iter()
        .map(|info| {
            let pids = info.associated_pids;
            match info.protocol_socket_info {
                ProtocolSocketInfo::Tcp(t) => {
                    let remote_set = t.remote_port != 0 || !t.remote_addr.is_unspecified();
                    RawSocket {
                        protocol: Protocol::Tcp,
                        family: family(&t.local_addr),
                        local_addr: t.local_addr,
                        local_port: t.local_port,
                        remote_addr: remote_set.then_some(t.remote_addr),
                        remote_port: remote_set.then_some(t.remote_port),
                        state: tcp_state(t.state),
                        uid: None,
                        inode: None,
                        pids,
                    }
                }
                ProtocolSocketInfo::Udp(u) => RawSocket {
                    protocol: Protocol::Udp,
                    family: family(&u.local_addr),
                    local_addr: u.local_addr,
                    local_port: u.local_port,
                    remote_addr: None,
                    remote_port: None,
                    state: SocketState::Bound,
                    uid: None,
                    inode: None,
                    pids,
                },
            }
        })
        .collect())
}

fn family(ip: &IpAddr) -> Family {
    if ip.is_ipv4() {
        Family::V4
    } else {
        Family::V6
    }
}

fn tcp_state(s: TcpState) -> SocketState {
    match s {
        TcpState::Listen => SocketState::Listen,
        TcpState::SynSent => SocketState::SynSent,
        TcpState::SynReceived => SocketState::SynRecv,
        TcpState::Established => SocketState::Established,
        TcpState::FinWait1 => SocketState::FinWait1,
        TcpState::FinWait2 => SocketState::FinWait2,
        TcpState::CloseWait => SocketState::CloseWait,
        TcpState::Closing => SocketState::Closing,
        TcpState::LastAck => SocketState::LastAck,
        TcpState::TimeWait => SocketState::TimeWait,
        TcpState::Closed | TcpState::DeleteTcb => SocketState::Close,
        TcpState::Unknown => SocketState::Unknown,
    }
}
