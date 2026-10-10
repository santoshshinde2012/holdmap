//! Correlates listeners and live TCP endpoints without active probes.

use super::{access, AgentLink, AgentPort, LinkKind, PortRole, MAX_LINKS};
use crate::model::{PortEntry, Protocol, SocketState};
use crate::scan::Scan;
use std::collections::{BTreeMap, HashSet};
use std::net::IpAddr;

pub(super) fn ports(scan: &Scan, set: &HashSet<u32>, helpers: &HashSet<u32>) -> Vec<AgentPort> {
    let holder = |e: &PortEntry| {
        e.pid
            .into_iter()
            .chain(e.pids.iter().copied())
            .find(|p| set.contains(p))
    };
    let mut out: Vec<AgentPort> = scan
        .snapshot
        .entries
        .iter()
        .filter(|e| e.state.is_listening())
        .filter_map(|e| {
            let pid = holder(e)?;
            let role = if helpers.contains(&pid) {
                PortRole::Agent
            } else if e.is_dev {
                PortRole::DevServer
            } else {
                PortRole::Service
            };
            Some(AgentPort {
                entry_id: e.id.clone(),
                port: e.port,
                protocol: e.protocol,
                exposure: e.exposure,
                pid: Some(pid),
                process: scan.table.get(pid).map(|p| p.name.clone()).or_else(|| {
                    e.process
                        .as_ref()
                        .filter(|p| p.pid == pid)
                        .map(|p| p.name.clone())
                }),
                label: e.label.clone(),
                role,
                project: e.project.as_ref().map(|p| p.name.clone()),
                framework: e.framework.as_ref().map(|f| f.name.clone()),
            })
        })
        .collect();
    out.sort_by_key(|p| (p.role as u8, p.port, p.protocol as u8));
    out
}

pub(super) fn links(
    scan: &Scan,
    set: &HashSet<u32>,
    ports: &[AgentPort],
    local_addrs: &HashSet<IpAddr>,
) -> (Vec<AgentLink>, usize, usize) {
    let own_entries: HashSet<&str> = ports.iter().map(|p| p.entry_id.as_str()).collect();
    let listeners: Vec<&PortEntry> = scan
        .snapshot
        .entries
        .iter()
        .filter(|e| e.protocol == Protocol::Tcp && e.state.is_listening())
        .collect();
    let is_local = |a: IpAddr| {
        let a = norm(a);
        a.is_loopback() || a.is_unspecified() || local_addrs.contains(&a)
    };
    let mut groups: BTreeMap<String, AgentLink> = BTreeMap::new();
    let mut remote_hosts = HashSet::new();
    for s in &scan.raw {
        if s.protocol != Protocol::Tcp || s.state != SocketState::Established {
            continue;
        }
        let (Some(raddr), Some(rport)) = (s.remote_addr, s.remote_port) else {
            continue;
        };
        if !s.pids.iter().any(|p| set.contains(p)) {
            continue;
        }
        // The accepted side of a connection to one of the agent's own listeners.
        if listeners.iter().any(|e| {
            own_entries.contains(e.id.as_str())
                && e.port == s.local_port
                && bind_match(e, s.local_addr).is_some()
                && e.pids
                    .iter()
                    .chain(e.pid.iter())
                    .any(|pid| s.pids.contains(pid))
        }) {
            continue;
        }
        let link = if is_local(raddr) {
            // Exact bind address wins over a wildcard, and unrelated bind addresses never
            // become the target simply because their port number happens to match.
            let target = listeners
                .iter()
                .filter(|e| e.port == rport)
                .filter_map(|e| bind_match(e, raddr).map(|rank| (rank, *e)))
                .min_by(|(rank_a, a), (rank_b, b)| rank_a.cmp(rank_b).then(a.id.cmp(&b.id)))
                .map(|(_, e)| e);
            if target.is_some_and(|e| {
                e.pid
                    .into_iter()
                    .chain(e.pids.iter().copied())
                    .any(|p| set.contains(&p))
            }) {
                continue; // between the agent's own processes
            }
            match target {
                Some(e) => AgentLink {
                    id: format!("local:{}", e.id),
                    kind: LinkKind::Local,
                    label: e.label.clone(),
                    address: fmt_endpoint(raddr, rport),
                    port: rport,
                    connections: 0,
                    entry_id: Some(e.id.clone()),
                    process: e.process.as_ref().map(|p| p.name.clone()),
                    pid: e.pid,
                    service: access::service_hint(rport).map(str::to_string),
                },
                None => AgentLink {
                    id: format!("local:{}", fmt_endpoint(raddr, rport)),
                    kind: LinkKind::Local,
                    label: fmt_endpoint(raddr, rport),
                    address: fmt_endpoint(raddr, rport),
                    port: rport,
                    connections: 0,
                    entry_id: None,
                    process: None,
                    pid: None,
                    service: access::service_hint(rport).map(str::to_string),
                },
            }
        } else {
            remote_hosts.insert(norm(raddr));
            let address = fmt_endpoint(raddr, rport);
            AgentLink {
                id: format!("remote:{address}"),
                kind: LinkKind::Remote,
                label: address.clone(),
                address,
                port: rport,
                connections: 0,
                entry_id: None,
                process: None,
                pid: None,
                service: access::service_hint(rport).map(str::to_string),
            }
        };
        groups.entry(link.id.clone()).or_insert(link).connections += 1;
    }
    let mut out: Vec<AgentLink> = groups.into_values().collect();
    out.sort_by(|a, b| {
        (a.kind as u8)
            .cmp(&(b.kind as u8))
            .then(b.connections.cmp(&a.connections))
            .then(a.id.cmp(&b.id))
    });
    let more = out.len().saturating_sub(MAX_LINKS);
    out.truncate(MAX_LINKS);
    (out, more, remote_hosts.len())
}

fn bind_match(entry: &PortEntry, address: IpAddr) -> Option<u8> {
    let address = norm(address);
    entry
        .addresses
        .iter()
        .filter_map(|bound| bound.parse::<IpAddr>().ok())
        .filter_map(|bound| {
            let bound = norm(bound);
            if bound == address {
                Some(0)
            } else if bound.is_unspecified() && bound.is_ipv4() == address.is_ipv4() {
                Some(1)
            } else if bound.is_unspecified() && bound.is_ipv6() && address.is_ipv4() {
                // The scan does not expose IPV6_V6ONLY. A wildcard IPv6 listener may also
                // accept IPv4; use it only after exact binds and IPv4 wildcards.
                Some(2)
            } else {
                None
            }
        })
        .min()
}

pub(super) fn norm(a: IpAddr) -> IpAddr {
    match a {
        IpAddr::V6(v) => v.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v)),
        v4 => v4,
    }
}

fn fmt_endpoint(a: IpAddr, port: u16) -> String {
    match norm(a) {
        IpAddr::V6(v) => format!("[{v}]:{port}"),
        IpAddr::V4(v) => format!("{v}:{port}"),
    }
}
