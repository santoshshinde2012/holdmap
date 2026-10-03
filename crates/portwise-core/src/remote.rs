//! Remote machines over SSH (read-only): run one script on the remote host, parse `ss` + `ps`
//! output into the same [`RawSocket`]/[`ProcessTable`] model, and feed it to a fixed
//! [`Scanner`] — so listing, explaining and the topology graph all work remotely without
//! installing anything there. (If `portwise` *is* installed remotely, the CLI forwards commands
//! to it instead, keeping all safety checks on the machine that owns the processes.)

use crate::model::{Family, ProcessInfo, Protocol, RawSocket, SocketState};
use crate::process::ProcessTable;
use crate::scan::{Scan, ScanOptions, Scanner};
use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::time::Duration;

/// Shell script run on the remote host: sockets, a separator, then processes.
pub const REMOTE_SCRIPT: &str = "LC_ALL=C; (ss -tanupH 2>/dev/null || ss -tanup 2>/dev/null | tail -n +2); echo @@PORTWISE-PS@@; ps -eo pid=,ppid=,uid=,user=,etimes=,rss=,args= 2>/dev/null";

/// Runs a shell script somewhere and returns its stdout.
pub trait RemoteRunner {
    fn describe(&self) -> String;
    fn run(&self, script: &str) -> io::Result<String>;
}

/// Validate an SSH destination typed by a user: `host`, `user@host`, `host:port`,
/// `user@host:port`, an `~/.ssh/config` alias, or a bracketed IPv6 literal. Anything that could
/// be read as an ssh *option* (a leading `-`) or contains whitespace / shell metacharacters is
/// rejected. Returns a user-facing message on failure.
pub fn validate_host(input: &str) -> Result<(), String> {
    let h = input.trim();
    if h.is_empty() {
        return Err("Enter a host, like devbox or user@10.0.0.5".into());
    }
    if h.len() > 255 {
        return Err("That host name is too long".into());
    }
    if h.starts_with('-') {
        return Err("A host can't start with “-”".into());
    }
    let (user, rest) = match h.rsplit_once('@') {
        Some((u, r)) => (Some(u), r),
        None => (None, h),
    };
    if let Some(u) = user {
        if u.is_empty()
            || !u
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        {
            return Err("The user name before “@” has unsupported characters".into());
        }
    }
    let (host, port) = if let Some(r) = rest.strip_prefix('[') {
        let (v6, tail) = r.split_once(']').ok_or("Close the IPv6 address with “]”")?;
        if v6.parse::<std::net::Ipv6Addr>().is_err() {
            return Err("That isn't a valid IPv6 address".into());
        }
        (v6, tail.strip_prefix(':'))
    } else {
        match rest.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (rest, None),
        }
    };
    if let Some(p) = port {
        match p.parse::<u16>() {
            Ok(n) if n > 0 => {}
            _ => return Err("The SSH port after “:” must be a number from 1 to 65535".into()),
        }
    }
    if host.is_empty() {
        return Err("Enter a host name after “@”".into());
    }
    if !rest.starts_with('[')
        && (host.starts_with('-')
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c)))
    {
        return Err("Host names may only contain letters, digits, “.”, “-” and “_”".into());
    }
    Ok(())
}

/// Split a validated destination into the `ssh` arguments (`-p PORT` and `user@host`).
pub fn ssh_destination(input: &str) -> (Option<u16>, String) {
    let h = input.trim();
    let (user, rest) = match h.rsplit_once('@') {
        Some((u, r)) => (Some(u), r),
        None => (None, h),
    };
    let (host, port) = if let Some(r) = rest.strip_prefix('[') {
        let (v6, tail) = r.split_once(']').unwrap_or((r, ""));
        (
            v6.to_string(),
            tail.strip_prefix(':').and_then(|p| p.parse().ok()),
        )
    } else {
        match rest.rsplit_once(':') {
            Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().ok()),
            _ => (rest.to_string(), None),
        }
    };
    let dest = match user {
        Some(u) => format!("{u}@{host}"),
        None => host,
    };
    (port, dest)
}

/// `ssh -o BatchMode=yes HOST sh -c SCRIPT`.
pub struct SshRunner {
    pub host: String,
    pub timeout: Duration,
}

impl SshRunner {
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            timeout: Duration::from_secs(20),
        }
    }
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl RemoteRunner for SshRunner {
    fn describe(&self) -> String {
        format!("ssh {}", self.host)
    }
    fn run(&self, script: &str) -> io::Result<String> {
        validate_host(&self.host).map_err(io::Error::other)?;
        let remote = format!("sh -c {}", sh_quote(script));
        let (port, dest) = ssh_destination(&self.host);
        let port = port.map(|p| p.to_string());
        let mut args = vec!["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"];
        if let Some(p) = &port {
            args.extend(["-p", p.as_str()]);
        }
        // `--` ends option parsing, so the destination can never be read as an option.
        args.extend(["--", dest.as_str(), remote.as_str()]);
        let out = crate::util::run_with_timeout("ssh", &args, self.timeout)
            .ok_or_else(|| io::Error::other(format!("ssh {} failed or timed out", self.host)))?;
        if out.stdout.trim().is_empty() {
            return Err(io::Error::other(format!(
                "ssh {} returned nothing{}",
                self.host,
                if out.stderr.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", out.stderr.trim())
                }
            )));
        }
        Ok(out.stdout)
    }
}

/// Runs the script with the local `sh` (tests, and `portwise ssh localhost` without sshd).
pub struct LocalShell;
impl RemoteRunner for LocalShell {
    fn describe(&self) -> String {
        "local sh".into()
    }
    fn run(&self, script: &str) -> io::Result<String> {
        crate::util::run_with_timeout("sh", &["-c", script], Duration::from_secs(20))
            .map(|o| o.stdout)
            .ok_or_else(|| io::Error::other("sh failed"))
    }
}

/// Scan a remote host through `runner`.
pub fn scan_remote(runner: &dyn RemoteRunner, label: &str) -> io::Result<Scan> {
    let out = runner.run(REMOTE_SCRIPT)?;
    let (ss, ps) = out
        .split_once("@@PORTWISE-PS@@")
        .ok_or_else(|| io::Error::other("unexpected remote output (no ss/ps separator)"))?;
    let raw = parse_ss(ss);
    let table = parse_ps(ps);
    if raw.is_empty() {
        return Err(io::Error::other(
            "the remote host returned no sockets (is `ss` from iproute2 installed?)",
        ));
    }
    Scanner::fixed(raw, table, Vec::new())
        .with_platform(format!("remote:{label}"))
        .scan(&ScanOptions {
            all_states: false,
            docker: false,
        })
}

fn parse_addr(s: &str) -> Option<(IpAddr, Option<u16>)> {
    let (host, port) = s.rsplit_once(':')?;
    let port = if port == "*" {
        None
    } else {
        Some(port.parse().ok()?)
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let host = host.split('%').next().unwrap_or(host); // 127.0.0.53%lo
    let ip = if host == "*" {
        "0.0.0.0".parse().ok()?
    } else {
        host.parse().ok()?
    };
    Some((ip, port))
}

fn parse_state(s: &str, udp: bool) -> SocketState {
    match s {
        "LISTEN" => SocketState::Listen,
        "UNCONN" => SocketState::Bound,
        "ESTAB" if udp => SocketState::Established,
        "ESTAB" => SocketState::Established,
        "SYN-SENT" => SocketState::SynSent,
        "SYN-RECV" => SocketState::SynRecv,
        "FIN-WAIT-1" => SocketState::FinWait1,
        "FIN-WAIT-2" => SocketState::FinWait2,
        "TIME-WAIT" => SocketState::TimeWait,
        "CLOSE-WAIT" => SocketState::CloseWait,
        "LAST-ACK" => SocketState::LastAck,
        "CLOSING" => SocketState::Closing,
        "CLOSE" | "UNCONNECTED" => SocketState::Close,
        _ => SocketState::Unknown,
    }
}

/// `users:(("node",pid=123,fd=20),("node",pid=124,fd=20))` → pids.
fn parse_users(s: &str) -> Vec<u32> {
    let mut v: Vec<u32> = s
        .split("pid=")
        .skip(1)
        .filter_map(|x| x.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Parse `ss -tanupH` output.
pub fn parse_ss(text: &str) -> Vec<RawSocket> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 6 {
                return None;
            }
            let udp = match f[0] {
                "tcp" => false,
                "udp" => true,
                _ => return None,
            };
            let (local_addr, local_port) = parse_addr(f[4])?;
            let (remote_addr, remote_port) = match parse_addr(f[5]) {
                Some((a, p)) if !a.is_unspecified() || p.is_some() => (Some(a), p),
                _ => (None, None),
            };
            let state = parse_state(f[1], udp);
            Some(RawSocket {
                protocol: if udp { Protocol::Udp } else { Protocol::Tcp },
                family: if local_addr.is_ipv6() {
                    Family::V6
                } else {
                    Family::V4
                },
                local_addr,
                local_port: local_port?,
                remote_addr: if state.is_listening() {
                    None
                } else {
                    remote_addr
                },
                remote_port: if state.is_listening() {
                    None
                } else {
                    remote_port
                },
                state,
                uid: None,
                inode: None,
                pids: f
                    .get(6..)
                    .map(|r| parse_users(&r.join(" ")))
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// Parse `ps -eo pid=,ppid=,uid=,user=,etimes=,rss=,args=` output.
pub fn parse_ps(text: &str) -> ProcessTable {
    let now = crate::util::now_secs();
    let mut procs = HashMap::new();
    for l in text.lines() {
        let mut it = l.split_whitespace();
        let (Some(pid), Some(ppid), Some(uid), Some(user), Some(et), Some(rss)) = (
            it.next(),
            it.next(),
            it.next(),
            it.next(),
            it.next(),
            it.next(),
        ) else {
            continue;
        };
        let (Ok(pid), Ok(ppid)) = (pid.parse::<u32>(), ppid.parse::<u32>()) else {
            continue;
        };
        let cmdline: Vec<String> = it.map(|s| s.to_string()).collect();
        let name = cmdline
            .first()
            .map(|c| c.rsplit('/').next().unwrap_or(c).to_string())
            .unwrap_or_default();
        let start = now.saturating_sub(et.parse().unwrap_or(0));
        procs.insert(
            pid,
            ProcessInfo {
                pid,
                ppid: Some(ppid),
                name,
                exe: None,
                cmdline,
                cwd: None,
                uid: uid.parse().ok(),
                user: Some(user.to_string()),
                start_time: start,
                start_token: start,
                memory_bytes: rss.parse::<u64>().unwrap_or(0) * 1024,
                cpu_percent: 0.0,
            },
        );
    }
    // No local process is "self" on a remote host.
    ProcessTable::from_processes(procs, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_validation_accepts_real_destinations() {
        for ok in [
            "devbox",
            "user@devbox",
            "dev.example.com",
            "me@10.0.0.5:2222",
            "[::1]",
            "u@[fe80::1]:22",
            "build_01",
            "a-b.c",
        ] {
            assert_eq!(validate_host(ok), Ok(()), "{ok}");
        }
    }

    #[test]
    fn host_validation_rejects_options_and_shell_metacharacters() {
        for bad in [
            "",
            "  ",
            "-oProxyCommand=sh",
            "x@-oProxyCommand=sh",
            "host;rm -rf",
            "a b",
            "h:0",
            "h:99999",
            "h:abc",
            "@host",
            "us er@h",
            "[::1",
            "[nope]",
            "u@",
            "$(id)",
        ] {
            assert!(validate_host(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn destinations_split_into_ssh_arguments() {
        assert_eq!(ssh_destination("devbox"), (None, "devbox".into()));
        assert_eq!(
            ssh_destination("me@10.0.0.5:2222"),
            (Some(2222), "me@10.0.0.5".into())
        );
        assert_eq!(
            ssh_destination("u@[fe80::1]:22"),
            (Some(22), "u@fe80::1".into())
        );
        assert_eq!(ssh_destination("[::1]"), (None, "::1".into()));
    }

    const SS: &str = "\
tcp   LISTEN 0      511          0.0.0.0:3000      0.0.0.0:*     users:((\"node\",pid=200,fd=20))
tcp   LISTEN 0      244        127.0.0.1:5432      0.0.0.0:*     users:((\"postgres\",pid=300,fd=6))
tcp   ESTAB  0      0          127.0.0.1:40100   127.0.0.1:5432  users:((\"node\",pid=200,fd=25))
tcp   ESTAB  0      0          127.0.0.1:5432    127.0.0.1:40100 users:((\"postgres\",pid=301,fd=9))
tcp   LISTEN 0      128             [::]:22           [::]:*
udp   UNCONN 0      0      127.0.0.53%lo:53        0.0.0.0:*
tcp   ESTAB  0      0     [::ffff:10.0.0.4]:22 [::ffff:10.0.0.9]:51515
";
    const PS: &str = "\
    1     0     0 root      9000  1000 /sbin/init
  200     1  1000 deploy     120 50000 node /srv/app/server.js
  300     1   999 postgres  9000 30000 /usr/lib/postgresql/16/bin/postgres -D /var/lib/postgresql
  301   300   999 postgres   100  8000 postgres: deploy app 127.0.0.1(40100) idle
";

    #[test]
    fn parses_ss_and_ps() {
        let raw = parse_ss(SS);
        assert_eq!(raw.len(), 7);
        assert_eq!(
            (raw[0].local_port, raw[0].pids.as_slice()),
            (3000, &[200][..])
        );
        assert_eq!(raw[2].state, SocketState::Established);
        assert_eq!(raw[2].remote_port, Some(5432));
        assert_eq!(raw[4].family, Family::V6);
        assert!(raw[4].pids.is_empty());
        assert_eq!(
            (raw[5].protocol, raw[5].state),
            (Protocol::Udp, SocketState::Bound)
        );
        let t = parse_ps(PS);
        assert_eq!(t.get(200).unwrap().name, "node");
        assert_eq!(t.get(301).unwrap().ppid, Some(300));
        assert_eq!(t.get(300).unwrap().memory_bytes, 30000 * 1024);
    }

    struct Canned;
    impl RemoteRunner for Canned {
        fn describe(&self) -> String {
            "canned".into()
        }
        fn run(&self, _: &str) -> io::Result<String> {
            Ok(format!("{SS}@@PORTWISE-PS@@\n{PS}"))
        }
    }

    #[test]
    fn remote_scan_feeds_listing_and_topology() {
        let scan = scan_remote(&Canned, "prod-1").unwrap();
        assert_eq!(scan.snapshot.platform, "remote:prod-1");
        let ports: Vec<u16> = scan.snapshot.entries.iter().map(|e| e.port).collect();
        assert!(ports.contains(&3000) && ports.contains(&5432) && ports.contains(&22));
        let pg = scan
            .snapshot
            .entries
            .iter()
            .find(|e| e.port == 5432)
            .unwrap();
        assert_eq!(pg.framework.as_ref().unwrap().name, "PostgreSQL");
        let g = crate::topology::TopologyBuilder::new(&scan).build();
        assert!(
            g.edges
                .iter()
                .any(|e| e.from == "svc:200" && e.to == "svc:300" && e.port == 5432),
            "{:?}",
            g.edges
        );
        // sshd's socket has no visible owner without root: a hidden node, no edges.
        let ssh = g.node_for_port(22).unwrap();
        assert_eq!(ssh.kind, crate::topology::NodeKind::Hidden);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn local_shell_runner_reads_this_machine() {
        if crate::util::run_with_timeout("ss", &["-V"], Duration::from_secs(3)).is_none() {
            return; // iproute2 not installed
        }
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let scan = scan_remote(&LocalShell, "localhost").unwrap();
        let e = scan
            .snapshot
            .entries
            .iter()
            .find(|e| e.port == port)
            .expect("own listener visible through ss");
        assert_eq!(e.pid, Some(std::process::id()));
    }
}
