//! Minimal, synchronous Docker Engine API client (also works with Podman, OrbStack, Colima and
//! Rancher Desktop, which expose the same API). Speaks HTTP/1.1 over a Unix socket, a Windows
//! named pipe, or plain TCP, with short timeouts so a stopped daemon never stalls a scan.

use crate::model::ContainerInfo;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_millis(1500);

/// Where a container runtime API is reachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Unix(PathBuf),
    Tcp(String),
    Pipe(String),
}

/// A container runtime endpoint plus a human label ("Docker Desktop", "OrbStack", …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    pub endpoint: Endpoint,
    pub label: String,
}

/// A container publishing one host port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedPort {
    pub host_port: u16,
    pub protocol: String,
    pub host_ip: Option<String>,
    pub container: ContainerInfo,
    pub runtime: Runtime,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ApiContainer {
    id: String,
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    image: String,
    #[serde(default)]
    ports: Vec<ApiPort>,
    #[serde(default)]
    labels: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct ApiPort {
    #[serde(rename = "IP", default)]
    ip: Option<String>,
    #[serde(rename = "PrivatePort", default)]
    private_port: u16,
    #[serde(rename = "PublicPort", default)]
    public_port: Option<u16>,
    #[serde(rename = "Type", default)]
    kind: String,
}

fn label_for(path: &str) -> &'static str {
    let p = path.to_ascii_lowercase();
    if p.contains("orbstack") {
        "OrbStack"
    } else if p.contains("colima") {
        "Colima"
    } else if p.contains("podman") {
        "Podman"
    } else if p.contains(".rd") {
        "Rancher Desktop"
    } else if p.contains(".docker/run")
        || p.contains("docker_engine")
        || p.contains("dockerdesktop")
    {
        "Docker Desktop"
    } else {
        "Docker"
    }
}

/// Candidate runtime endpoints, honouring `DOCKER_HOST` first. Only endpoints that exist are
/// returned (for sockets); nothing is contacted here.
pub fn discover() -> Vec<Runtime> {
    let mut out = Vec::new();
    if let Ok(host) = std::env::var("DOCKER_HOST") {
        if let Some(ep) = parse_docker_host(&host) {
            out.push(Runtime {
                label: label_for(&host).into(),
                endpoint: ep,
            });
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let xdg = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let mut paths: Vec<PathBuf> = vec![
        "/var/run/docker.sock".into(),
        "/run/podman/podman.sock".into(),
    ];
    if let Some(h) = &home {
        for rel in [
            ".docker/run/docker.sock",
            ".orbstack/run/docker.sock",
            ".colima/default/docker.sock",
            ".colima/docker.sock",
            ".rd/docker.sock",
            ".local/share/containers/podman/machine/podman.sock",
        ] {
            paths.push(h.join(rel));
        }
    }
    if let Some(x) = &xdg {
        paths.push(x.join("docker.sock"));
        paths.push(x.join("podman/podman.sock"));
    }
    for p in paths {
        // Resolve symlinks (/var/run/docker.sock → ~/.docker/run/docker.sock) to dedupe.
        let real = std::fs::canonicalize(&p).unwrap_or(p.clone());
        if p.exists()
            && !out
                .iter()
                .any(|r: &Runtime| r.endpoint == Endpoint::Unix(real.clone()))
        {
            let label = label_for(&real.to_string_lossy()).to_string();
            out.push(Runtime {
                endpoint: Endpoint::Unix(real),
                label,
            });
        }
    }
    if cfg!(windows) {
        for pipe in [
            r"\\.\pipe\docker_engine",
            r"\\.\pipe\podman-machine-default",
        ] {
            if !out
                .iter()
                .any(|r| r.endpoint == Endpoint::Pipe(pipe.into()))
            {
                out.push(Runtime {
                    endpoint: Endpoint::Pipe(pipe.into()),
                    label: label_for(pipe).into(),
                });
            }
        }
    }
    out
}

pub fn parse_docker_host(host: &str) -> Option<Endpoint> {
    if let Some(p) = host.strip_prefix("unix://") {
        Some(Endpoint::Unix(PathBuf::from(p)))
    } else if let Some(t) = host.strip_prefix("tcp://") {
        Some(Endpoint::Tcp(t.trim_end_matches('/').to_string()))
    } else {
        host.strip_prefix("npipe://")
            .map(|p| Endpoint::Pipe(p.replace('/', "\\")))
    }
}

trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}

fn connect(ep: &Endpoint) -> io::Result<Box<dyn Stream>> {
    match ep {
        #[cfg(unix)]
        Endpoint::Unix(p) => {
            let s = std::os::unix::net::UnixStream::connect(p)?;
            s.set_read_timeout(Some(TIMEOUT))?;
            s.set_write_timeout(Some(TIMEOUT))?;
            Ok(Box::new(s))
        }
        #[cfg(not(unix))]
        Endpoint::Unix(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unix sockets unsupported",
        )),
        Endpoint::Tcp(addr) => {
            use std::net::ToSocketAddrs;
            let sa = addr
                .to_socket_addrs()?
                .next()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad DOCKER_HOST"))?;
            let s = std::net::TcpStream::connect_timeout(&sa, TIMEOUT)?;
            s.set_read_timeout(Some(TIMEOUT))?;
            s.set_write_timeout(Some(TIMEOUT))?;
            Ok(Box::new(s))
        }
        Endpoint::Pipe(name) => {
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(name)?;
            Ok(Box::new(f))
        }
    }
}

/// Perform one HTTP request; returns (status, body).
pub fn request(ep: &Endpoint, method: &str, path: &str) -> io::Result<(u16, Vec<u8>)> {
    let mut s = connect(ep)?;
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: docker\r\nUser-Agent: portwise\r\nAccept: application/json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    s.write_all(req.as_bytes())?;
    s.flush()?;
    let mut buf = Vec::new();
    // Read until EOF (Connection: close) or until a full response has been received.
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if response_complete(&buf) {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e)
                if !buf.is_empty()
                    && matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
            {
                break
            }
            Err(e) => return Err(e),
        }
    }
    parse_response(&buf)
}

fn split_head(buf: &[u8]) -> Option<(&str, &[u8])> {
    let pos = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    Some((std::str::from_utf8(&buf[..pos]).ok()?, &buf[pos + 4..]))
}

fn response_complete(buf: &[u8]) -> bool {
    let Some((head, body)) = split_head(buf) else {
        return false;
    };
    let lower = head.to_ascii_lowercase();
    if lower.contains("transfer-encoding: chunked") {
        return body.ends_with(b"0\r\n\r\n");
    }
    if let Some(len) = lower
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
    {
        return body.len() >= len;
    }
    false
}

/// Parse an HTTP/1.1 response, de-chunking if necessary.
pub fn parse_response(buf: &[u8]) -> io::Result<(u16, Vec<u8>)> {
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "malformed HTTP response");
    let (head, body) = split_head(buf).ok_or_else(bad)?;
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(bad)?;
    let chunked = head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked");
    if !chunked {
        return Ok((status, body.to_vec()));
    }
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let line_end = rest.windows(2).position(|w| w == b"\r\n").ok_or_else(bad)?;
        let size_str = std::str::from_utf8(&rest[..line_end]).map_err(|_| bad())?;
        let size = usize::from_str_radix(size_str.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| bad())?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            break;
        }
        if rest.len() < size {
            return Err(bad());
        }
        out.extend_from_slice(&rest[..size]);
        rest = rest.get(size + 2..).unwrap_or(&[]);
    }
    Ok((status, out))
}

/// Parse `GET /containers/json` output into published ports.
pub fn parse_containers(body: &[u8], runtime: &Runtime) -> io::Result<Vec<PublishedPort>> {
    let list: Vec<ApiContainer> =
        serde_json::from_slice(body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut out = Vec::new();
    for c in list {
        let labels = c.labels.unwrap_or_default();
        let name = c
            .names
            .first()
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_else(|| c.id.chars().take(12).collect());
        for p in c.ports {
            let Some(public) = p.public_port.filter(|p| *p != 0) else {
                continue;
            };
            let container = ContainerInfo {
                id: c.id.clone(),
                name: name.clone(),
                image: c.image.clone(),
                runtime: runtime.label.clone(),
                compose_project: labels.get("com.docker.compose.project").cloned(),
                compose_service: labels.get("com.docker.compose.service").cloned(),
                private_port: p.private_port,
            };
            let dup = out.iter().any(|o: &PublishedPort| {
                o.host_port == public && o.protocol == p.kind && o.container.id == c.id
            });
            if !dup {
                out.push(PublishedPort {
                    host_port: public,
                    protocol: p.kind.clone(),
                    host_ip: p.ip.clone(),
                    container,
                    runtime: runtime.clone(),
                });
            }
        }
    }
    Ok(out)
}

/// Query every reachable runtime for running containers with published ports.
/// Returns (ports, whether any runtime answered).
pub fn published_ports() -> (Vec<PublishedPort>, bool) {
    let mut all = Vec::new();
    let mut any = false;
    for rt in discover() {
        if let Ok((200, body)) = request(&rt.endpoint, "GET", "/containers/json") {
            any = true;
            if let Ok(ports) = parse_containers(&body, &rt) {
                for p in ports {
                    if !all.iter().any(|a: &PublishedPort| {
                        a.container.id == p.container.id
                            && a.host_port == p.host_port
                            && a.protocol == p.protocol
                    }) {
                        all.push(p);
                    }
                }
            }
        }
    }
    (all, any)
}

/// Gracefully stop a container (`POST /containers/{id}/stop?t=N`): SIGTERM, then SIGKILL after N s.
pub fn stop_container(runtime: &Endpoint, id: &str, timeout_s: u64) -> io::Result<()> {
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid container id",
        ));
    }
    // The daemon blocks for up to t seconds; allow for that on top of our socket timeout.
    let (status, body) = request_with_timeout(
        runtime,
        "POST",
        &format!("/containers/{id}/stop?t={timeout_s}"),
        Duration::from_secs(timeout_s + 5),
    )?;
    match status {
        204 | 304 => Ok(()),
        404 => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "container not found",
        )),
        _ => Err(io::Error::other(format!(
            "runtime returned HTTP {status}: {}",
            String::from_utf8_lossy(&body).trim()
        ))),
    }
}

fn request_with_timeout(
    ep: &Endpoint,
    method: &str,
    path: &str,
    t: Duration,
) -> io::Result<(u16, Vec<u8>)> {
    #[cfg(unix)]
    if let Endpoint::Unix(p) = ep {
        let mut s = std::os::unix::net::UnixStream::connect(p)?;
        s.set_read_timeout(Some(t))?;
        s.set_write_timeout(Some(TIMEOUT))?;
        let req = format!("{method} {path} HTTP/1.1\r\nHost: docker\r\nUser-Agent: portwise\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes())?;
        let mut buf = Vec::new();
        s.read_to_end(&mut buf)?;
        return parse_response(&buf);
    }
    let _ = t;
    request(ep, method, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt() -> Runtime {
        Runtime {
            endpoint: Endpoint::Unix("/var/run/docker.sock".into()),
            label: "Docker".into(),
        }
    }

    #[test]
    fn parses_chunked_response() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n[{\"a\"\r\n4\r\n:1}]\r\n0\r\n\r\n";
        let (status, body) = parse_response(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, b"[{\"a\":1}]");
        assert!(response_complete(raw));
    }

    #[test]
    fn parses_content_length_response() {
        let raw = b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n";
        assert_eq!(parse_response(raw).unwrap(), (204, vec![]));
        assert!(response_complete(raw));
    }

    #[test]
    fn parses_container_list() {
        let body = include_bytes!("../tests/fixtures/docker_containers.json");
        let ports = parse_containers(body, &rt()).unwrap();
        assert_eq!(
            ports.len(),
            2,
            "IPv4+IPv6 duplicates collapse, unpublished ports skipped"
        );
        let pg = &ports[0];
        assert_eq!(pg.host_port, 5432);
        assert_eq!(pg.container.name, "shop-db-1");
        assert_eq!(pg.container.image, "postgres:16");
        assert_eq!(pg.container.compose_project.as_deref(), Some("shop"));
        assert_eq!(pg.container.compose_service.as_deref(), Some("db"));
        assert_eq!(ports[1].host_port, 6380);
        assert_eq!(ports[1].container.private_port, 6379);
    }

    #[test]
    fn docker_host_parsing() {
        assert_eq!(
            parse_docker_host("unix:///x/d.sock"),
            Some(Endpoint::Unix("/x/d.sock".into()))
        );
        assert_eq!(
            parse_docker_host("tcp://127.0.0.1:2375"),
            Some(Endpoint::Tcp("127.0.0.1:2375".into()))
        );
        assert_eq!(
            parse_docker_host("npipe:////./pipe/docker_engine"),
            Some(Endpoint::Pipe(r"\\.\pipe\docker_engine".into()))
        );
        assert_eq!(parse_docker_host("ssh://host"), None);
    }

    #[test]
    fn runtime_labels() {
        assert_eq!(label_for("/Users/me/.orbstack/run/docker.sock"), "OrbStack");
        assert_eq!(
            label_for("/Users/me/.docker/run/docker.sock"),
            "Docker Desktop"
        );
        assert_eq!(label_for("/run/user/1000/podman/podman.sock"), "Podman");
        assert_eq!(label_for("/var/run/docker.sock"), "Docker");
    }

    #[test]
    fn rejects_bad_container_ids() {
        assert!(stop_container(&Endpoint::Tcp("127.0.0.1:1".into()), "../../etc", 1).is_err());
    }
}
