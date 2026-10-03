//! Tunnel / port-forward detection from command lines: `kubectl port-forward`, `ssh -L`,
//! `cloudflared`, `ngrok`. A forwarded port is not "a server you started" — stopping it closes
//! the tunnel, and the real service lives elsewhere.

use crate::model::ProcessInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelKind {
    /// `kubectl port-forward` (local port → pod/service in a cluster).
    Kubectl,
    /// `ssh -L` local forward (local port → host:port via an SSH server).
    Ssh,
    /// `cloudflared tunnel --url …` (public URL → local port).
    Cloudflared,
    /// `ngrok http 3000` (public URL → local port).
    Ngrok,
}

/// A detected tunnel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub kind: TunnelKind,
    /// Human description, e.g. `svc/api:80 in namespace shop` or `db.internal:5432 via bastion`.
    pub target: String,
    /// Kubernetes namespace (kubectl only).
    pub namespace: Option<String>,
    /// Kubernetes context (kubectl only).
    pub context: Option<String>,
    /// Local port the tunnel exposes or forwards, when known.
    pub local_port: Option<u16>,
}

fn basename(s: &str) -> String {
    let b = s.rsplit(['/', '\\']).next().unwrap_or(s);
    b.strip_suffix(".exe").unwrap_or(b).to_ascii_lowercase()
}

/// Value of `-n X`, `--namespace X` or `--namespace=X` style flags.
fn flag_value(args: &[String], short: Option<&str>, long: &str) -> Option<String> {
    let eq = format!("{long}=");
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if Some(a.as_str()) == short || a == long {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&eq) {
            return Some(v.to_string());
        }
    }
    None
}

/// Detect a tunnel from a process. `port` narrows multi-forward commands to the listener's port.
pub fn detect_tunnel(p: &ProcessInfo, port: Option<u16>) -> Option<TunnelInfo> {
    let args = &p.cmdline;
    let prog = args
        .first()
        .map(|a| basename(a))
        .unwrap_or_else(|| basename(&p.name));
    match prog.as_str() {
        "kubectl" | "oc" if args.iter().any(|a| a == "port-forward") => kubectl(args, port),
        "ssh" => ssh(args, port),
        "cloudflared" => cloudflared(args),
        "ngrok" => ngrok(args),
        _ => None,
    }
}

fn kubectl(args: &[String], port: Option<u16>) -> Option<TunnelInfo> {
    let ns = flag_value(args, Some("-n"), "--namespace");
    let context = flag_value(args, None, "--context");
    let pf = args.iter().position(|a| a == "port-forward")?;
    // Positional args after `port-forward`, skipping flags and their values.
    let mut pos = Vec::new();
    let mut it = args[pf + 1..].iter();
    while let Some(a) = it.next() {
        if a.starts_with('-') {
            if !a.contains('=')
                && matches!(
                    a.as_str(),
                    "-n" | "--namespace"
                        | "--context"
                        | "--address"
                        | "--pod-running-timeout"
                        | "--kubeconfig"
                )
            {
                it.next();
            }
            continue;
        }
        pos.push(a.clone());
    }
    let resource = pos.first()?.clone();
    let specs: Vec<(u16, String)> = pos[1..]
        .iter()
        .filter_map(|s| {
            let (l, r) = s.split_once(':').unwrap_or((s.as_str(), s.as_str()));
            let lp: u16 = l
                .parse()
                .ok()
                .or(if l.is_empty() { Some(0) } else { None })?;
            Some((lp, r.to_string()))
        })
        .collect();
    let (local, remote) = specs
        .iter()
        .find(|(l, _)| port.is_some_and(|p| *l == p))
        .or(specs.first())
        .cloned()
        .unwrap_or((0, String::new()));
    let ns_text = ns.clone().unwrap_or_else(|| "default".into());
    Some(TunnelInfo {
        kind: TunnelKind::Kubectl,
        target: format!(
            "{resource}{} in namespace {ns_text}",
            if remote.is_empty() {
                String::new()
            } else {
                format!(":{remote}")
            }
        ),
        namespace: Some(ns_text),
        context,
        local_port: (local != 0).then_some(local).or(port),
    })
}

fn ssh(args: &[String], port: Option<u16>) -> Option<TunnelInfo> {
    let mut forwards = Vec::new();
    let mut i = 1;
    let mut dest = None;
    while i < args.len() {
        let a = &args[i];
        if a == "-L" {
            if let Some(v) = args.get(i + 1) {
                forwards.push(v.clone());
            }
            i += 2;
            continue;
        }
        if let Some(v) = a.strip_prefix("-L").filter(|v| !v.is_empty()) {
            forwards.push(v.to_string());
        } else if a.starts_with('-') {
            // Options that take a value.
            if matches!(
                a.as_str(),
                "-p" | "-i"
                    | "-o"
                    | "-J"
                    | "-F"
                    | "-l"
                    | "-R"
                    | "-D"
                    | "-W"
                    | "-b"
                    | "-c"
                    | "-E"
                    | "-S"
                    | "-w"
                    | "-m"
                    | "-e"
                    | "-Q"
                    | "-O"
                    | "-B"
                    | "-I"
            ) {
                i += 1;
            }
        } else if dest.is_none() {
            dest = Some(a.clone());
        }
        i += 1;
    }
    // [bind:]port:host:hostport
    let parsed: Vec<(u16, String)> = forwards
        .iter()
        .filter_map(|f| {
            let parts: Vec<&str> = f.split(':').collect();
            let (lp, rest) = match parts.len() {
                3 => (parts[0], &parts[1..]),
                4 => (parts[1], &parts[2..]),
                _ => return None,
            };
            Some((lp.parse().ok()?, rest.join(":")))
        })
        .collect();
    let (local, remote) = parsed
        .iter()
        .find(|(l, _)| port.is_some_and(|p| *l == p))
        .or(parsed.first())?
        .clone();
    Some(TunnelInfo {
        kind: TunnelKind::Ssh,
        target: format!(
            "{remote}{}",
            dest.map(|d| format!(" via {d}")).unwrap_or_default()
        ),
        namespace: None,
        context: None,
        local_port: Some(local),
    })
}

fn local_port_of_url(u: &str) -> Option<u16> {
    let rest = u.split_once("://").map(|x| x.1).unwrap_or(u);
    let host = rest.split('/').next()?;
    host.rsplit_once(':')?.1.parse().ok()
}

fn cloudflared(args: &[String]) -> Option<TunnelInfo> {
    let url = flag_value(args, None, "--url");
    let named = args
        .iter()
        .position(|a| a == "run")
        .and_then(|i| args.get(i + 1))
        .filter(|a| !a.starts_with('-'))
        .cloned();
    if url.is_none() && named.is_none() && !args.iter().any(|a| a == "tunnel") {
        return None;
    }
    Some(TunnelInfo {
        kind: TunnelKind::Cloudflared,
        target: match (&url, &named) {
            (Some(u), _) => format!("public URL → {u}"),
            (None, Some(n)) => format!("named tunnel {n}"),
            _ => "Cloudflare tunnel".into(),
        },
        namespace: None,
        context: None,
        local_port: url.as_deref().and_then(local_port_of_url),
    })
}

fn ngrok(args: &[String]) -> Option<TunnelInfo> {
    let proto = args.get(1)?.clone();
    if !matches!(proto.as_str(), "http" | "tcp" | "tls") {
        return None;
    }
    let addr = args[2..].iter().find(|a| !a.starts_with('-'))?.clone();
    let port = addr
        .parse::<u16>()
        .ok()
        .or_else(|| local_port_of_url(&addr));
    Some(TunnelInfo {
        kind: TunnelKind::Ngrok,
        target: format!("public {proto} URL → {addr}"),
        namespace: None,
        context: None,
        local_port: port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::proc;

    #[test]
    fn kubectl_port_forward() {
        let p = proc(
            1,
            0,
            "kubectl",
            &[
                "kubectl",
                "-n",
                "shop",
                "port-forward",
                "svc/api",
                "8080:80",
                "9090:9090",
            ],
        );
        let t = detect_tunnel(&p, Some(9090)).unwrap();
        assert_eq!(t.kind, TunnelKind::Kubectl);
        assert_eq!(t.namespace.as_deref(), Some("shop"));
        assert_eq!(t.local_port, Some(9090));
        assert_eq!(t.target, "svc/api:9090 in namespace shop");
        let p = proc(
            1,
            0,
            "kubectl",
            &[
                "/usr/local/bin/kubectl",
                "port-forward",
                "--context=dev",
                "pod/db-0",
                "5432",
            ],
        );
        let t = detect_tunnel(&p, None).unwrap();
        assert_eq!(t.namespace.as_deref(), Some("default"));
        assert_eq!(t.context.as_deref(), Some("dev"));
        assert_eq!(t.local_port, Some(5432));
        assert!(detect_tunnel(&proc(1, 0, "kubectl", &["kubectl", "get", "pods"]), None).is_none());
    }

    #[test]
    fn ssh_local_forward() {
        let p = proc(
            1,
            0,
            "ssh",
            &[
                "ssh",
                "-N",
                "-L",
                "15432:db.internal:5432",
                "-p",
                "22",
                "me@bastion",
            ],
        );
        let t = detect_tunnel(&p, Some(15432)).unwrap();
        assert_eq!(t.kind, TunnelKind::Ssh);
        assert_eq!(t.target, "db.internal:5432 via me@bastion");
        let p = proc(1, 0, "ssh", &["ssh", "-L127.0.0.1:8443:web:443", "box"]);
        assert_eq!(detect_tunnel(&p, None).unwrap().local_port, Some(8443));
        assert!(detect_tunnel(&proc(1, 0, "ssh", &["ssh", "host"]), None).is_none());
    }

    #[test]
    fn public_tunnels() {
        let p = proc(
            1,
            0,
            "cloudflared",
            &["cloudflared", "tunnel", "--url", "http://localhost:3000"],
        );
        let t = detect_tunnel(&p, None).unwrap();
        assert_eq!(
            (t.kind, t.local_port),
            (TunnelKind::Cloudflared, Some(3000))
        );
        let p = proc(1, 0, "ngrok", &["ngrok", "http", "8080"]);
        let t = detect_tunnel(&p, None).unwrap();
        assert_eq!((t.kind, t.local_port), (TunnelKind::Ngrok, Some(8080)));
        assert!(detect_tunnel(&proc(1, 0, "ngrok", &["ngrok", "version"]), None).is_none());
    }
}
