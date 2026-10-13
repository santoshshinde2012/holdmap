//! HTTP probe: ask a local port what it serves (status, page title, server header) with a single
//! short `GET /` over plain HTTP/1.1. It never follows redirects, never speaks TLS and reads at most
//! [`MAX_BODY`] bytes, so probing an unknown listener stays cheap and side-effect free.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
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
    let deadline = started.checked_add(timeout)?;
    let mut stream = [
        SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        SocketAddr::from((Ipv6Addr::LOCALHOST, port)),
    ]
    .iter()
    .find_map(|a| {
        let left = deadline.saturating_duration_since(Instant::now());
        (!left.is_zero())
            .then(|| TcpStream::connect_timeout(a, left).ok())
            .flatten()
    })?;
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return None;
    }
    stream.set_write_timeout(Some(left)).ok()?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost:{port}\r\nUser-Agent: holdmap/{}\r\nAccept: text/html,*/*;q=0.8\r\nConnection: close\r\n\r\n",
        crate::VERSION
    );
    stream.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0u8; 8192];
    let mut first = None;
    while buf.len() < MAX_BODY && Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || stream.set_read_timeout(Some(left)).is_err() {
            break;
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                first.get_or_insert_with(|| started.elapsed());
                buf.extend_from_slice(&chunk[..n.min(MAX_BODY - buf.len())]);
                // Only inspect the final response's framed body: headers and trailers can
                // contain title-like text, and informational responses precede the result.
                if let Some(response) = final_response(&buf) {
                    let body = response.body_prefix()?;
                    if body.complete || contains_ci(&body.bytes, b"</title>") {
                        break;
                    }
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
    let response = final_response(raw)?;
    let prefix = response.body_prefix()?;
    let head = String::from_utf8_lossy(response.head);
    let body = String::from_utf8_lossy(&prefix.bytes);
    let mut lines = head.lines();
    let status_line = lines.next()?;
    let mut parts = status_line.splitn(3, ' ');
    parts.next()?;
    parts.next()?;
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
        status: response.status,
        reason,
        title: title(&body),
        server,
        location,
        elapsed_ms: 0,
    })
}

// Keep the parser small and bounded; probes only need a final status and a body prefix.
// Eight informational responses are sufficient for a local diagnostic without allowing
// a listener to keep the parser chasing an unbounded chain of Early Hints.
const MAX_INFORMATIONAL_RESPONSES: usize = 8;

#[derive(Clone, Copy)]
enum BodyFraming {
    Empty,
    Length(usize),
    Chunked,
    Close,
}

struct Response<'a> {
    head: &'a [u8],
    body: &'a [u8],
    status: u16,
    framing: BodyFraming,
}

struct BodyPrefix<'a> {
    bytes: Cow<'a, [u8]>,
    complete: bool,
}

impl Response<'_> {
    fn body_prefix(&self) -> Option<BodyPrefix<'_>> {
        let (bytes, complete) = match self.framing {
            BodyFraming::Empty => (&[][..], true),
            BodyFraming::Length(length) => (
                &self.body[..self.body.len().min(length)],
                self.body.len() >= length,
            ),
            BodyFraming::Close => (self.body, false),
            BodyFraming::Chunked => return chunked_prefix(self.body),
        };
        Some(BodyPrefix {
            bytes: Cow::Borrowed(bytes),
            complete,
        })
    }
}

fn final_response(mut raw: &[u8]) -> Option<Response<'_>> {
    for _ in 0..=MAX_INFORMATIONAL_RESPONSES {
        let (end, delimiter_len) = header_end(raw)?;
        let head = &raw[..end];
        let mut parts = head
            .split(|&byte| byte == b'\n')
            .next()?
            .splitn(3, |&byte| byte == b' ');
        if !matches!(parts.next()?, b"HTTP/1.0" | b"HTTP/1.1") {
            return None;
        }
        let code = parts.next()?;
        let code = code.strip_suffix(b"\r").unwrap_or(code);
        if code.len() != 3 || !code.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let status = u16::from(code[0] - b'0') * 100
            + u16::from(code[1] - b'0') * 10
            + u16::from(code[2] - b'0');
        if !(100..=599).contains(&status) || status == 101 {
            // This GET did not request a protocol upgrade.
            return None;
        }
        let framing = body_framing(head, status)?;
        raw = &raw[end + delimiter_len..];
        if status >= 200 {
            return Some(Response {
                head,
                body: raw,
                status,
                framing,
            });
        }
    }
    None
}

fn header_end(raw: &[u8]) -> Option<(usize, usize)> {
    let crlf = raw.windows(4).position(|window| window == b"\r\n\r\n");
    let lf = raw.windows(2).position(|window| window == b"\n\n");
    match (crlf, lf) {
        (Some(crlf), Some(lf)) if lf < crlf => Some((lf, 2)),
        (Some(crlf), _) => Some((crlf, 4)),
        (_, Some(lf)) => Some((lf, 2)),
        _ => None,
    }
}

fn body_framing(head: &[u8], status: u16) -> Option<BodyFraming> {
    let mut length = None;
    let mut chunked = false;
    for line in head.split(|&byte| byte == b'\n').skip(1) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let colon = line.iter().position(|&byte| byte == b':')?;
        let (name, value) = (&line[..colon], &line[colon + 1..]);
        if name.is_empty()
            || !name
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(byte))
        {
            return None;
        }
        if name.eq_ignore_ascii_case(b"content-length") {
            let value = std::str::from_utf8(value).ok()?.trim_matches([' ', '\t']);
            for item in value.split(',') {
                let item = item.trim_matches([' ', '\t']);
                if item.is_empty() || !item.bytes().all(|byte| byte.is_ascii_digit()) {
                    return None;
                }
                let parsed = item.parse::<usize>().ok()?;
                if length.is_some_and(|previous| previous != parsed) {
                    return None;
                }
                length = Some(parsed);
            }
        } else if name.eq_ignore_ascii_case(b"transfer-encoding") {
            let value = std::str::from_utf8(value).ok()?.trim_matches([' ', '\t']);
            if chunked || !value.eq_ignore_ascii_case("chunked") {
                return None;
            }
            chunked = true;
        }
    }
    if chunked && length.is_some() {
        return None;
    }
    Some(if status < 200 || matches!(status, 204 | 304) {
        BodyFraming::Empty
    } else if chunked {
        BodyFraming::Chunked
    } else if let Some(length) = length {
        BodyFraming::Length(length)
    } else {
        BodyFraming::Close
    })
}

fn chunked_prefix(mut raw: &[u8]) -> Option<BodyPrefix<'_>> {
    let mut bytes = Vec::with_capacity(raw.len());
    loop {
        let Some(end) = raw.windows(2).position(|window| window == b"\r\n") else {
            return Some(BodyPrefix {
                bytes: Cow::Owned(bytes),
                complete: false,
            });
        };
        let size = raw[..end].split(|&byte| byte == b';').next()?;
        if size.is_empty() || !size.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        let size = usize::from_str_radix(std::str::from_utf8(size).ok()?, 16).ok()?;
        raw = &raw[end + 2..];
        if size == 0 {
            // Trailers do not contain response content, even if they contain HTML.
            let complete = raw.starts_with(b"\r\n") || header_end(raw).is_some();
            return Some(BodyPrefix {
                bytes: Cow::Owned(bytes),
                complete,
            });
        }
        bytes.extend_from_slice(&raw[..raw.len().min(size)]);
        if raw.len().saturating_sub(size) < 2 {
            return Some(BodyPrefix {
                bytes: Cow::Owned(bytes),
                complete: false,
            });
        }
        if &raw[size..size + 2] != b"\r\n" {
            return None;
        }
        raw = &raw[size + 2..];
    }
}

fn clip(s: &str, max: usize) -> String {
    // Metadata is display-only; redact the entire value before clipping, since truncation
    // can remove the @ or closing syntax needed to recognize a credential-bearing URL.
    let s = crate::redact::line(s);
    if s.chars().count() <= max {
        s
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
    fn http_metadata_redacts_credentials_before_clipping_or_exporting() {
        let raw = b"HTTP/1.1 302 token=reason-private\r\nServer: token=server-private\r\nLocation: http://developer:url-private@localhost/callback?%74oken=query-private&view=ports\r\n\r\n<title>Agent token=title-private &amp; Ready</title>";
        let info = parse_response(3000, raw).unwrap();
        assert_eq!(info.status, 302);
        assert_eq!(info.reason, format!("token={}", crate::redact::MASK));
        assert_eq!(info.server.as_deref(), Some("token=••••"));
        assert_eq!(info.title.as_deref(), Some("Agent token=•••• & Ready"));
        assert_eq!(
            info.location.as_deref(),
            Some("http://developer:••••@localhost/callback?%74oken=••••&view=ports")
        );
        let exported = serde_json::to_string(&info).unwrap();
        let summary = info.summary();
        for secret in [
            "reason-private",
            "server-private",
            "url-private",
            "query-private",
            "title-private",
        ] {
            assert!(
                !exported.contains(secret),
                "JSON leaked {secret}: {exported}"
            );
            assert!(
                !summary.contains(secret),
                "summary leaked {secret}: {summary}"
            );
        }
        // Long values must be redacted in full before display limits truncate them.
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://developer:{}@localhost\r\n\r\n",
            "private-url-password".repeat(20)
        );
        assert_eq!(
            parse_response(3000, response.as_bytes())
                .unwrap()
                .location
                .as_deref(),
            Some("http://developer:••••@localhost")
        );
        let info = parse_response(
            3000,
            b"HTTP/1.1 302 Found\r\nLocation: /callback?view=ports#access_token=fragment-private&state=ok\r\n\r\n",
        )
        .unwrap();
        assert_eq!(
            info.location.as_deref(),
            Some("/callback?view=ports#access_token=••••&state=ok")
        );
    }

    #[test]
    fn real_http_probe_never_returns_raw_redirect_credentials() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream
                .write_all(b"HTTP/1.1 302 Found\r\nLocation: http://dev:probe-private@localhost/?access_token=probe-token-private\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });
        let info = probe(port, "/", Duration::from_secs(2)).unwrap();
        server.join().unwrap();
        let exported = serde_json::to_string(&info).unwrap();
        assert!(!exported.contains("probe-private"), "{exported}");
        assert!(!exported.contains("probe-token-private"), "{exported}");
        assert_eq!(
            info.location.as_deref(),
            Some("http://dev:••••@localhost/?access_token=••••")
        );
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
        assert!(parse_response(1, b"HTTP/unknown 200 OK\r\n\r\n").is_none());
        assert!(parse_response(1, b"HTTP/1.1 999 OK\r\n\r\n").is_none());
    }

    #[test]
    fn incomplete_headers_and_invalid_status_codes_do_not_report_health() {
        for raw in [
            &b"HTTP/1.1 200 OK"[..],
            &b"HTTP/1.1 200 OK\r\nServer: unfinished"[..],
            &b"HTTP/1.1 0200 OK\r\n\r\n"[..],
            &b"HTTP/1.1 +200 OK\r\n\r\n"[..],
        ] {
            assert!(parse_response(1, raw).is_none(), "{raw:?}");
        }
    }

    #[test]
    fn informational_responses_require_a_final_response() {
        let hints = b"HTTP/1.1 103 Early Hints\r\nServer: hint-server\r\n\r\n";
        assert!(parse_response(1, hints).is_none());
        let mut response = hints.to_vec();
        response.extend_from_slice(
            b"HTTP/1.1 200 OK\r\nServer: final-server\r\n\r\n<title>Ready</title>",
        );
        let info = parse_response(1, &response).unwrap();
        assert_eq!(info.status, 200);
        assert_eq!(info.server.as_deref(), Some("final-server"));
        assert_eq!(info.title.as_deref(), Some("Ready"));
        assert!(parse_response(1, b"HTTP/1.1 101 Switching Protocols\r\n\r\n").is_none());
    }

    #[test]
    fn informational_response_chains_are_bounded() {
        let hints = b"HTTP/1.1 103 Early Hints\r\n\r\n";
        let mut response = hints.repeat(MAX_INFORMATIONAL_RESPONSES);
        response.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        assert_eq!(parse_response(1, &response).unwrap().status, 200);
        let mut excessive = hints.to_vec();
        excessive.extend_from_slice(&response);
        assert!(parse_response(1, &excessive).is_none());
    }

    #[test]
    fn body_framing_excludes_chunk_metadata_and_bytes_outside_the_body() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n<ti\r\n11\r\ntle>Hello</title>\r\n0\r\nTrailer: <title>Wrong</title>\r\n\r\n";
        let info = parse_response(1, raw).unwrap();
        assert_eq!(info.title.as_deref(), Some("Hello"));
        let info = parse_response(
            1,
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK<title>Wrong</title>",
        )
        .unwrap();
        assert_eq!(info.title, None);
        let info =
            parse_response(1, b"HTTP/1.1 204 No Content\r\n\r\n<title>Wrong</title>").unwrap();
        assert_eq!(info.title, None);
    }

    #[test]
    fn ambiguous_and_invalid_body_framing_is_refused() {
        for raw in [
            &b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nOK"[..],
            &b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
                [..],
            &b"HTTP/1.1 200 OK\r\nContent-Length: +2\r\n\r\nOK"[..],
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nnot-a-size\r\n"[..],
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nOKwrong"[..],
        ] {
            assert!(parse_response(1, raw).is_none(), "{raw:?}");
        }
    }

    #[test]
    fn partial_bodies_preserve_available_content_within_the_probe_budget() {
        for raw in [
            &b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\n\r\n<title>Ready</title>"[..],
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n186a0\r\n<title>Ready</title>"
                [..],
        ] {
            assert_eq!(
                parse_response(1, raw).unwrap().title.as_deref(),
                Some("Ready")
            );
        }
        assert!(parse_response(
            1,
            b"HTTP/1.1 200 OK\r\nContent-Length: 2, 2\r\nContent-Length: 2\r\n\r\nOK"
        )
        .is_some());
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
    fn probe_waits_for_split_final_headers_and_decodes_split_title_chunks() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream
                .write_all(b"HTTP/1.1 103 Early Hints\r\n\r\nHTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nServer: </title>\r\n")
                .unwrap();
            std::thread::sleep(Duration::from_millis(20));
            stream.write_all(b"\r\n3\r\n<ti\r\n").unwrap();
            std::thread::sleep(Duration::from_millis(20));
            stream
                .write_all(b"11\r\ntle>Hello</title>\r\n0\r\n\r\n")
                .unwrap();
        });
        let info = probe(port, "/", Duration::from_secs(2)).unwrap();
        assert_eq!(info.status, 200);
        assert_eq!(info.title.as_deref(), Some("Hello"));
        server.join().unwrap();
    }

    #[test]
    fn probe_finishes_completed_bodies_without_waiting_for_connection_close() {
        for response in [
            &b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nOK"[..],
            &b"HTTP/1.1 204 No Content\r\nConnection: keep-alive\r\n\r\n"[..],
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n2\r\nOK\r\n0\r\n\r\n"[..],
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let (release, wait) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request = [0; 4096];
                assert!(stream.read(&mut request).unwrap() > 0);
                stream.write_all(response).unwrap();
                let _ = wait.recv_timeout(Duration::from_secs(3));
            });
            let started = Instant::now();
            let info = probe(port, "/", Duration::from_secs(1)).unwrap();
            let elapsed = started.elapsed();
            release.send(()).unwrap();
            server.join().unwrap();
            assert!(info.healthy());
            assert!(elapsed < Duration::from_millis(700), "{elapsed:?}");
        }
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

    #[test]
    fn probe_keeps_a_total_deadline_when_the_server_drips_bytes() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            let (mut stream, _) = l.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
            std::thread::sleep(Duration::from_millis(70));
            if stream.write_all(b"a").is_ok() {
                std::thread::sleep(Duration::from_millis(200));
            }
        });
        let started = Instant::now();
        assert!(probe(port, "/", Duration::from_millis(100)).is_some());
        assert!(started.elapsed() < Duration::from_millis(160));
        t.join().unwrap();
    }

    #[test]
    fn zero_timeout_does_not_open_a_connection() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        assert!(probe(l.local_addr().unwrap().port(), "/", Duration::ZERO).is_none());
        assert_eq!(
            l.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
