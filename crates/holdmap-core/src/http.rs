//! HTTP probe: ask a local port what it serves (status, page title, server header) with a single
//! short `GET /` over plain HTTP/1.1. It never follows redirects, never speaks TLS and reads at most
//! [`MAX_BODY`] bytes, so probing an unknown listener stays cheap and side-effect free.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// How much of the response is read, headers included.
pub const MAX_BODY: usize = 64 * 1024;

/// What an HTTP server on a local port answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpInfo {
    /// The port that answered.
    pub port: u16,
    /// HTTP status code, e.g. `200`.
    pub status: u16,
    /// Reason phrase, e.g. `OK` (may be empty).
    pub reason: String,
    /// The HTML `<title>`, trimmed and whitespace-collapsed, when the body has one.
    pub title: Option<String>,
    /// The `Server` header, e.g. `nginx/1.27`.
    pub server: Option<String>,
    /// The `Location` header of a redirect.
    pub location: Option<String>,
    /// Round-trip time to the first response bytes, in milliseconds.
    pub elapsed_ms: u64,
}

impl HttpInfo {
    /// `2xx` or `3xx`: the server is up and serving.
    pub fn healthy(&self) -> bool {
        (200..400).contains(&self.status)
    }

    /// One-line summary such as `200 OK · "Shop" · nginx`.
    pub fn summary(&self) -> String {
        let mut s = format!("{} {}", self.status, self.reason)
            .trim_end()
            .to_string();
        if let Some(t) = &self.title {
            s.push_str(&format!(" · \"{t}\""));
        }
        if let Some(l) = &self.location {
            s.push_str(&format!(" → {l}"));
        }
        if let Some(v) = &self.server {
            s.push_str(&format!(" · {v}"));
        }
        s
    }
}

/// Probe `http://localhost:{port}{path}`. Returns `None` when nothing answers within `timeout` or
/// the answer isn't HTTP (a database, a TLS-only server, a raw TCP protocol).
pub fn probe(port: u16, path: &str, timeout: Duration) -> Option<HttpInfo> {
    if !is_safe_path(path) {
        return None;
    }
    let started = Instant::now();
    let mut stream = [
        SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        SocketAddr::from((Ipv6Addr::LOCALHOST, port)),
    ]
    .iter()
    .find_map(|a| TcpStream::connect_timeout(a, timeout).ok())?;
    let left = timeout
        .saturating_sub(started.elapsed())
        .max(Duration::from_millis(50));
    stream.set_read_timeout(Some(left)).ok()?;
    stream.set_write_timeout(Some(left)).ok()?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost:{port}\r\nUser-Agent: holdmap/{}\r\nAccept: text/html,*/*;q=0.8\r\nConnection: close\r\n\r\n",
        crate::VERSION
    );
    stream.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0u8; 8192];
    let deadline = started + timeout;
    let mut first = None;
    while buf.len() < MAX_BODY && Instant::now() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                first.get_or_insert_with(|| started.elapsed());
                buf.extend_from_slice(&chunk[..n.min(MAX_BODY - buf.len())]);
                // Stop early once the title has been seen.
                if contains_ci(&buf, b"</title>") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let mut info = parse_response(port, &buf)?;
    info.elapsed_ms = first.unwrap_or_else(|| started.elapsed()).as_millis() as u64;
    Some(info)
}

/// An HTTP origin-form path, without bytes that can introduce headers or another request.
pub fn is_safe_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= 2048
        && path
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'#')
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle))
}

/// Parse a raw HTTP/1.x response (status line, headers and the start of the body).
pub fn parse_response(port: u16, raw: &[u8]) -> Option<HttpInfo> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .or_else(|| text.split_once("\n\n"))
        .unwrap_or((&text, ""));
    let mut lines = head.lines();
    let status_line = lines.next()?;
    let mut parts = status_line.splitn(3, ' ');
    if !parts.next()?.starts_with("HTTP/") {
        return None;
    }
    let status: u16 = parts.next()?.trim().parse().ok()?;
    let reason = clip(parts.next().unwrap_or("").trim(), 60);
    let mut server = None;
    let mut location = None;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            let v = v.trim();
            if v.is_empty() {
                continue;
            }
            if k.eq_ignore_ascii_case("server") {
                server = Some(clip(v, 60));
            } else if k.eq_ignore_ascii_case("location") {
                location = Some(clip(v, 120));
            }
        }
    }
    Some(HttpInfo {
        port,
        status,
        reason,
        title: title(body),
        server,
        location,
        elapsed_ms: 0,
    })
}

fn clip(s: &str, max: usize) -> String {
    let s = &*crate::util::printable(s);
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

/// The `<title>` of an HTML document: entity-decoded for the common cases, whitespace collapsed.
pub fn title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let raw = html.get(start..end)?;
    let decoded = raw
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ");
    let t = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    (!t.is_empty()).then(|| clip(&t, 80))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn a_local_page_cannot_inject_terminal_escapes() {
        let raw =
            b"HTTP/1.1 200 OK\x1b[2J\r\nServer: x\x1b]0;owned\x07\r\n\r\n<title>hi\x1b[31m</title>";
        let i = parse_response(3000, raw).unwrap();
        for s in [
            &i.reason,
            i.server.as_ref().unwrap(),
            i.title.as_ref().unwrap(),
        ] {
            assert!(!s.contains('\x1b') && !s.contains('\x07'), "{s:?}");
        }
    }

    #[test]
    fn parses_status_headers_and_title() {
        let raw = b"HTTP/1.1 200 OK\r\nServer: nginx/1.27\r\nContent-Type: text/html\r\n\r\n<html><head><TITLE>\n  Shop &amp; Co  \n</TITLE></head></html>";
        let i = parse_response(3000, raw).unwrap();
        assert_eq!((i.status, i.reason.as_str()), (200, "OK"));
        assert_eq!(i.title.as_deref(), Some("Shop & Co"));
        assert_eq!(i.server.as_deref(), Some("nginx/1.27"));
        assert!(i.healthy());
        assert_eq!(i.summary(), "200 OK · \"Shop & Co\" · nginx/1.27");
    }

    #[test]
    fn redirects_and_errors() {
        let i = parse_response(1, b"HTTP/1.0 302 Found\nLocation: /login\n\n").unwrap();
        assert_eq!(i.location.as_deref(), Some("/login"));
        assert!(i.healthy());
        let i = parse_response(1, b"HTTP/1.1 503 Service Unavailable\r\n\r\n").unwrap();
        assert!(!i.healthy());
        assert_eq!(i.title, None);
    }

    #[test]
    fn rejects_non_http() {
        assert!(parse_response(1, b"").is_none());
        assert!(parse_response(1, b"SSH-2.0-OpenSSH_9.6\r\n").is_none());
        assert!(parse_response(1, b"-ERR unknown command\r\n").is_none());
        assert!(parse_response(1, b"HTTP/1.1 abc\r\n\r\n").is_none());
    }

    #[test]
    fn security_probe_rejects_injected_request_targets_before_connecting() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        for path in [
            "/ HTTP/1.1\r\nHost: other\r\n\r\nPOST /write",
            "/health\nInjected: value",
            "/health path",
            "//other-host/path",
        ] {
            assert!(probe(port, path, Duration::from_millis(10)).is_none());
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock,
                "invalid path {path:?} must not even open a connection"
            );
        }
    }

    #[test]
    fn title_edge_cases() {
        assert_eq!(title("<title></title>"), None);
        assert_eq!(title("no title here"), None);
        assert_eq!(title("<title lang=\"en\">A</title>").as_deref(), Some("A"));
        assert_eq!(
            title(&format!("<title>{}</title>", "x".repeat(200)))
                .unwrap()
                .chars()
                .count(),
            80
        );
    }

    #[test]
    fn probes_a_real_server() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut req = [0u8; 1024];
            let n = s.read(&mut req).unwrap();
            let req = String::from_utf8_lossy(&req[..n]).to_string();
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<title>Hello</title>")
                .unwrap();
            req
        });
        let info = probe(port, "/health", Duration::from_secs(2)).unwrap();
        assert_eq!(info.status, 200);
        assert_eq!(info.title.as_deref(), Some("Hello"));
        let req = t.join().unwrap();
        assert!(req.starts_with("GET /health HTTP/1.1\r\n"), "{req}");
        assert!(req.contains(&format!("Host: localhost:{port}")));
    }

    #[test]
    fn nothing_listening_or_silent_server() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        // Accepts (kernel backlog) but never answers: must time out quickly.
        let started = Instant::now();
        assert!(probe(port, "/", Duration::from_millis(300)).is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(l);
        assert!(probe(port, "/", Duration::from_millis(300)).is_none());
    }
}
