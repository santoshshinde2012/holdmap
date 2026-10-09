//! Linux backend: `/proc/net/*` socket tables, `/proc/<pid>/fd` inode → PID mapping,
//! `/proc/<pid>/stat` start time, `/proc/<pid>/cgroup` → systemd unit, and pidfd signalling.

use super::{Sig, SignalError};
use crate::model::{Family, Protocol, RawSocket, SocketState};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const TABLES: [(&str, Protocol, Family); 4] = [
    ("/proc/self/net/tcp", Protocol::Tcp, Family::V4),
    ("/proc/self/net/tcp6", Protocol::Tcp, Family::V6),
    ("/proc/self/net/udp", Protocol::Udp, Family::V4),
    ("/proc/self/net/udp6", Protocol::Udp, Family::V6),
];

/// Every socket from `/proc/net/{tcp,udp}{,6}`, joined to PIDs via `/proc/*/fd`.
pub fn list_sockets() -> io::Result<Vec<RawSocket>> {
    let mut sockets = Vec::new();
    let mut any_ok = false;
    for (path, proto, fam) in TABLES {
        match fs::read_to_string(path) {
            Ok(content) => {
                any_ok = true;
                sockets.extend(parse_proc_net(&content, proto, fam));
            }
            // IPv6 may be disabled; that's fine.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    if !any_ok {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "/proc/net is not available",
        ));
    }
    let map = inode_pid_map();
    for s in &mut sockets {
        if let Some(inode) = s.inode {
            if let Some(pids) = map.get(&inode) {
                s.pids = pids.clone();
            }
        }
    }
    Ok(sockets)
}

/// Parse one `/proc/net/{tcp,tcp6,udp,udp6}` table.
pub fn parse_proc_net(content: &str, protocol: Protocol, family: Family) -> Vec<RawSocket> {
    content
        .lines()
        .skip(1)
        .filter_map(|line| parse_line(line, protocol, family))
        .collect()
}

fn parse_line(line: &str, protocol: Protocol, family: Family) -> Option<RawSocket> {
    let f: Vec<&str> = line.split_whitespace().collect();
    if f.len() < 10 {
        return None;
    }
    let (laddr, lport) = parse_endpoint(f[1], family)?;
    let (raddr, rport) = parse_endpoint(f[2], family)?;
    let st = u8::from_str_radix(f[3], 16).ok()?;
    let state = match (protocol, st) {
        (Protocol::Udp, 0x07) => SocketState::Bound,
        (_, 0x01) => SocketState::Established,
        (_, 0x02) => SocketState::SynSent,
        (_, 0x03) => SocketState::SynRecv,
        (_, 0x04) => SocketState::FinWait1,
        (_, 0x05) => SocketState::FinWait2,
        (_, 0x06) => SocketState::TimeWait,
        (_, 0x07) => SocketState::Close,
        (_, 0x08) => SocketState::CloseWait,
        (_, 0x09) => SocketState::LastAck,
        (_, 0x0A) => SocketState::Listen,
        (_, 0x0B) => SocketState::Closing,
        _ => SocketState::Unknown,
    };
    let uid = f[7].parse().ok();
    let inode: u64 = f[9].parse().ok()?;
    let remote_set = rport != 0 || !raddr.is_unspecified();
    Some(RawSocket {
        protocol,
        family,
        local_addr: laddr,
        local_port: lport,
        remote_addr: remote_set.then_some(raddr),
        remote_port: remote_set.then_some(rport),
        state,
        uid,
        // TIME_WAIT sockets report inode 0: they have no owner any more.
        inode: (inode != 0).then_some(inode),
        pids: Vec::new(),
    })
}

/// Parse `0100007F:0BB8` (v4) or `00000000000000000000000001000000:1F90` (v6).
fn parse_endpoint(s: &str, family: Family) -> Option<(IpAddr, u16)> {
    let (addr, port) = s.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let ip = match family {
        Family::V4 => {
            if addr.len() != 8 {
                return None;
            }
            // The kernel prints the network-order u32 as a host-order integer.
            let n = u32::from_str_radix(addr, 16).ok()?;
            IpAddr::V4(Ipv4Addr::from(n.to_ne_bytes()))
        }
        Family::V6 => {
            if addr.len() != 32 {
                return None;
            }
            let mut bytes = [0u8; 16];
            for i in 0..4 {
                let n = u32::from_str_radix(&addr[i * 8..i * 8 + 8], 16).ok()?;
                bytes[i * 4..i * 4 + 4].copy_from_slice(&n.to_ne_bytes());
            }
            IpAddr::V6(Ipv6Addr::from(bytes))
        }
    };
    Some((ip, port))
}

/// Map socket inode → PIDs by walking `/proc/<pid>/fd`. Processes we can't read are skipped.
pub fn inode_pid_map() -> HashMap<u64, Vec<u32>> {
    let mut map: HashMap<u64, Vec<u32>> = HashMap::new();
    let Ok(dir) = fs::read_dir("/proc") else {
        return map;
    };
    for entry in dir.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(fds) = fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if let Ok(target) = fs::read_link(fd.path()) {
                if let Some(inode) = parse_socket_link(&target.to_string_lossy()) {
                    let pids = map.entry(inode).or_default();
                    if !pids.contains(&pid) {
                        pids.push(pid);
                    }
                }
            }
        }
    }
    map
}

fn parse_socket_link(s: &str) -> Option<u64> {
    s.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
}

/// Fields of `/proc/<pid>/stat` after the `(comm)` field.
fn stat_fields(pid: u32) -> Option<Vec<String>> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    Some(rest.split_whitespace().map(str::to_owned).collect())
}

/// `starttime` (field 22) in clock ticks since boot: unique per PID incarnation.
pub fn start_token(pid: u32) -> Option<u64> {
    // After ')' the fields start at field 3 (state), so starttime (22) is index 19.
    stat_fields(pid)?.get(19)?.parse().ok()
}

/// True when the process is a zombie (exited, not yet reaped).
pub fn is_zombie(pid: u32) -> bool {
    stat_fields(pid)
        .and_then(|f| f.first().cloned())
        .is_some_and(|s| s == "Z" || s == "X")
}

/// A systemd unit derived from `/proc/<pid>/cgroup`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgroupUnit {
    /// Unit name, e.g. `nginx.service`.
    pub unit: String,
    /// True when the unit runs in the user manager (`systemctl --user`).
    pub user: bool,
}

/// Find the systemd *service* unit a process belongs to, if any.
pub fn cgroup_unit(pid: u32) -> Option<CgroupUnit> {
    let content = fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    parse_cgroup_unit(&content)
}

/// Parse the contents of `/proc/<pid>/cgroup` into a systemd unit.
pub fn parse_cgroup_unit(content: &str) -> Option<CgroupUnit> {
    // cgroup v2: "0::/user.slice/user-1000.slice/user@1000.service/app.slice/foo.service"
    // cgroup v1: "1:name=systemd:/system.slice/nginx.service"
    for line in content.lines() {
        let path = line.splitn(3, ':').nth(2)?;
        let user = path.contains("/user@");
        let unit = path
            .split('/')
            .rev()
            .find(|seg| seg.ends_with(".service") && !seg.starts_with("user@"));
        if let Some(unit) = unit {
            return Some(CgroupUnit {
                unit: unit.to_string(),
                user,
            });
        }
    }
    None
}

/// Read a process's environment (only works for our own processes or as root).
pub fn environ(pid: u32) -> Option<HashMap<String, String>> {
    let raw = fs::read(format!("/proc/{pid}/environ")).ok()?;
    Some(
        raw.split(|b| *b == 0)
            .filter_map(|kv| {
                let s = String::from_utf8_lossy(kv);
                let (k, v) = s.split_once('=')?;
                Some((k.to_string(), v.to_string()))
            })
            .collect(),
    )
}

fn errno() -> i32 {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn sig_num(sig: Sig) -> libc::c_int {
    match sig {
        Sig::Term => libc::SIGTERM,
        Sig::Kill => libc::SIGKILL,
    }
}

/// Race-free signalling: open a pidfd, confirm identity via start time, then signal the pidfd.
/// Once the pidfd is open it refers to exactly one process incarnation, so a PID reused between
/// the identity check and the signal can never be hit. Falls back to check + `kill(2)` on kernels
/// without pidfd (< 5.3).
pub fn signal(pid: u32, expected_token: u64, sig: Sig) -> Result<(), SignalError> {
    if pid == 0 || pid > i32::MAX as u32 {
        return Err(SignalError::Other(format!("invalid pid {pid}")));
    }
    // SAFETY: plain syscall with integer args.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::c_int, 0) };
    if fd < 0 {
        let e = errno();
        if e == libc::ESRCH {
            return Err(SignalError::NotFound);
        }
        if e != libc::ENOSYS && e != libc::EPERM {
            return Err(SignalError::Other(
                io::Error::from_raw_os_error(e).to_string(),
            ));
        }
        // Fallback: identity check then kill(2).
        check_identity(pid, expected_token)?;
        // SAFETY: kill with a validated pid.
        let rc = unsafe { libc::kill(pid as libc::pid_t, sig_num(sig)) };
        return map_rc(rc);
    }
    let fd = fd as libc::c_int;
    let result = (|| {
        check_identity(pid, expected_token)?;
        // SAFETY: fd is a valid pidfd we own; info pointer may be null per pidfd_send_signal(2).
        let rc = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd,
                sig_num(sig),
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        map_rc(rc as i32)
    })();
    // SAFETY: closing the fd we opened.
    unsafe { libc::close(fd) };
    result
}

fn check_identity(pid: u32, expected: u64) -> Result<(), SignalError> {
    match start_token(pid) {
        None => Err(SignalError::NotFound),
        Some(t) if t != expected => Err(SignalError::IdentityChanged),
        Some(_) => Ok(()),
    }
}

fn map_rc(rc: i32) -> Result<(), SignalError> {
    if rc == 0 {
        return Ok(());
    }
    match errno() {
        libc::ESRCH => Err(SignalError::NotFound),
        libc::EPERM => Err(SignalError::PermissionDenied),
        e => Err(SignalError::Other(
            io::Error::from_raw_os_error(e).to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = include_str!("../../tests/fixtures/proc_net_tcp.txt");
    const TCP6: &str = include_str!("../../tests/fixtures/proc_net_tcp6.txt");
    const UDP: &str = include_str!("../../tests/fixtures/proc_net_udp.txt");

    #[test]
    fn parses_tcp_v4() {
        let s = parse_proc_net(TCP, Protocol::Tcp, Family::V4);
        assert_eq!(s.len(), 4);
        assert_eq!(s[0].local_addr, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(s[0].local_port, 3000);
        assert_eq!(s[0].state, SocketState::Listen);
        assert_eq!(s[0].uid, Some(1000));
        assert_eq!(s[0].inode, Some(123456));
        assert_eq!(s[0].remote_addr, None);
        assert_eq!(s[1].local_addr, "0.0.0.0".parse::<IpAddr>().unwrap());
        assert_eq!(s[1].local_port, 5432);
        assert_eq!(s[2].state, SocketState::Established);
        assert_eq!(s[2].remote_port, Some(5432));
        assert_eq!(s[3].state, SocketState::TimeWait);
        assert_eq!(s[3].inode, None, "TIME_WAIT has no inode");
    }

    #[test]
    fn parses_tcp_v6() {
        let s = parse_proc_net(TCP6, Protocol::Tcp, Family::V6);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].local_addr, "::".parse::<IpAddr>().unwrap());
        assert_eq!(s[0].local_port, 8080);
        assert_eq!(s[1].local_addr, "::1".parse::<IpAddr>().unwrap());
        assert_eq!(s[1].local_port, 5173);
    }

    #[test]
    fn parses_udp_bound() {
        let s = parse_proc_net(UDP, Protocol::Udp, Family::V4);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].state, SocketState::Bound);
        assert_eq!(s[0].local_port, 5353);
    }

    #[test]
    fn ignores_garbage() {
        assert!(parse_proc_net("header\nnot a line\n", Protocol::Tcp, Family::V4).is_empty());
    }

    #[test]
    fn socket_link() {
        assert_eq!(parse_socket_link("socket:[42]"), Some(42));
        assert_eq!(parse_socket_link("pipe:[42]"), None);
    }

    #[test]
    fn cgroup_parsing() {
        let v2 = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/myapp.service\n";
        assert_eq!(
            parse_cgroup_unit(v2),
            Some(CgroupUnit {
                unit: "myapp.service".into(),
                user: true
            })
        );
        let sys = "0::/system.slice/nginx.service\n";
        assert_eq!(
            parse_cgroup_unit(sys),
            Some(CgroupUnit {
                unit: "nginx.service".into(),
                user: false
            })
        );
        let term =
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-terminal.scope\n";
        assert_eq!(parse_cgroup_unit(term), None);
    }

    #[test]
    fn own_start_token_is_stable() {
        let pid = std::process::id();
        let a = start_token(pid).unwrap();
        assert_eq!(Some(a), start_token(pid));
        assert!(super::super::is_alive(pid, a));
        assert!(!super::super::is_alive(pid, a + 1));
    }

    #[test]
    fn identity_mismatch_refuses_to_signal() {
        let pid = std::process::id();
        let t = start_token(pid).unwrap();
        // A wrong token must never deliver a signal (this would kill the test runner).
        assert_eq!(
            signal(pid, t + 1, Sig::Kill),
            Err(SignalError::IdentityChanged)
        );
    }
}
