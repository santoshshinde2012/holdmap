use crate::{
    credentials::decode_token, normalize, now_ms, protocol::Route, store::ActivityStore,
    ActivityError, CollectorCredentials, Provider, COLLECTOR_LIMITS,
};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

const MAX_HEADERS: usize = 32;
const MAX_HEADER_BYTES: usize = 8 * 1024;
const IO_DEADLINE: Duration = Duration::from_secs(1);

pub(crate) struct Runtime {
    pub endpoint: String,
    pub started_at_ms: u64,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn start(
        store: Arc<Mutex<ActivityStore>>,
    ) -> Result<(Self, CollectorCredentials), ActivityError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| ActivityError::StartFailed)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| ActivityError::StartFailed)?;
        let address = listener
            .local_addr()
            .map_err(|_| ActivityError::StartFailed)?;
        let endpoint = format!("http://{address}");
        let mut secret = [0_u8; 32];
        getrandom::fill(&mut secret).map_err(|_| ActivityError::StartFailed)?;
        let token = secret.iter().map(|byte| format!("{byte:02x}")).collect();
        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let (sender, receiver) =
            mpsc::sync_channel::<TcpStream>(COLLECTOR_LIMITS.max_pending_connections);
        let receiver = Arc::new(Mutex::new(receiver));
        let rate = Arc::new(Mutex::new(RateLimit::new()));
        let mut workers = Vec::with_capacity(COLLECTOR_LIMITS.max_workers);
        for index in 0..COLLECTOR_LIMITS.max_workers {
            let running = Arc::clone(&running);
            let queue = Arc::clone(&receiver);
            let store = Arc::clone(&store);
            let rate = Arc::clone(&rate);
            match thread::Builder::new()
                .name(format!("holdmap-activity-worker-{index}"))
                .spawn(move || {
                    while running.load(Ordering::Acquire) {
                        let stream = queue
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .recv();
                        let Ok(mut stream) = stream else { break };
                        if !running.load(Ordering::Acquire) {
                            break;
                        }
                        handle(&mut stream, address, &secret, &store, &rate);
                    }
                }) {
                Ok(worker) => workers.push(worker),
                Err(_) => {
                    server_running.store(false, Ordering::Release);
                    drop(sender);
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(ActivityError::StartFailed);
                }
            }
        }
        let started_at_ms = now_ms();
        let thread_running = Arc::clone(&server_running);
        let thread = thread::Builder::new()
            .name("holdmap-activity-listener".into())
            .spawn(move || {
                while thread_running.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, peer)) if peer.ip().is_loopback() => {
                            // Accepted sockets can inherit O_NONBLOCK on some platforms.
                            if stream.set_nonblocking(false).is_ok() {
                                let _ = sender.try_send(stream);
                            }
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                thread_running.store(false, Ordering::Release);
                drop(listener);
                drop(sender);
                for worker in workers {
                    let _ = worker.join();
                }
            })
            .map_err(|_| {
                server_running.store(false, Ordering::Release);
                ActivityError::StartFailed
            })?;
        Ok((
            Self {
                endpoint: endpoint.clone(),
                started_at_ms,
                running: server_running,
                thread: Some(thread),
            },
            CollectorCredentials { endpoint, token },
        ))
    }

    pub fn active(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop();
    }
}

struct RateLimit {
    window: Instant,
    count: usize,
}

impl RateLimit {
    fn new() -> Self {
        Self {
            window: Instant::now(),
            count: 0,
        }
    }
    fn allow(&mut self) -> bool {
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.count = 0;
        }
        if self.count >= COLLECTOR_LIMITS.requests_per_second {
            return false;
        }
        self.count += 1;
        true
    }
}

struct Request {
    route: Route,
    body: Value,
}

fn timeout(stream: &TcpStream, deadline: Instant) -> Result<(), u16> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(408_u16)?;
    stream
        .set_read_timeout(Some(remaining))
        .map_err(|_| 408_u16)?;
    stream
        .set_write_timeout(Some(remaining))
        .map_err(|_| 408_u16)
}

fn read_request(
    stream: &mut TcpStream,
    address: SocketAddr,
    secret: &[u8; 32],
    deadline: Instant,
) -> Result<Request, u16> {
    let mut buffer = Vec::with_capacity(MAX_HEADER_BYTES);
    let header_end = loop {
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            let end = end + 4;
            if end > MAX_HEADER_BYTES {
                return Err(431);
            }
            break end;
        }
        if buffer.len() >= MAX_HEADER_BYTES {
            return Err(431);
        }
        timeout(stream, deadline)?;
        let mut chunk = [0_u8; 1024];
        let count = match stream.read(&mut chunk) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(408),
        };
        if count == 0 {
            return Err(400);
        }
        buffer.extend_from_slice(&chunk[..count]);
    };
    // httparse supplies the HTTP grammar; these stricter framing rules remove ambiguity.
    if buffer[..header_end]
        .iter()
        .enumerate()
        .any(|(index, byte)| {
            (*byte == b'\n' && (index == 0 || buffer[index - 1] != b'\r'))
                || (*byte == b'\r' && buffer.get(index + 1) != Some(&b'\n'))
        })
    {
        return Err(400);
    }
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut parsed = httparse::Request::new(&mut headers);
    if parsed.parse(&buffer[..header_end]).map_err(|_| 400_u16)?
        != httparse::Status::Complete(header_end)
    {
        return Err(400);
    }
    if parsed.version != Some(1) {
        return Err(400);
    }
    if parsed.method != Some("POST") {
        return Err(405);
    }
    let path = parsed.path.ok_or(400_u16)?;
    let route = Route::from_path(path).ok_or(404_u16)?;
    let mut host = None;
    let mut authorization = None;
    let mut length = None;
    let mut content_type = None;
    let mut names = std::collections::HashSet::new();
    for header in parsed.headers.iter() {
        if header
            .value
            .iter()
            .any(|byte| !(0x20..=0x7e).contains(byte))
        {
            return Err(400);
        }
        let value = std::str::from_utf8(header.value).map_err(|_| 400_u16)?;
        let name = header.name.to_ascii_lowercase();
        if !names.insert(name.clone()) {
            return Err(400);
        }
        match name.as_str() {
            "host" => host = Some(value),
            "authorization" => authorization = Some(value),
            "content-length" => {
                if value.is_empty()
                    || value.len() > 10
                    || !value.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return Err(400);
                }
                length = Some(value.parse::<usize>().map_err(|_| 413_u16)?);
            }
            "content-type" => content_type = Some(value.to_ascii_lowercase()),
            "content-encoding" if value != "identity" => return Err(415),
            "origin" => return Err(403),
            "transfer-encoding" | "expect" => return Err(400),
            _ if name.starts_with("access-control-") => return Err(403),
            _ => {}
        }
    }
    if host != Some(address.to_string().as_str())
        && host != Some(format!("localhost:{}", address.port()).as_str())
    {
        return Err(403);
    }
    let authorized = authorization
        .and_then(|value| value.strip_prefix("Bearer "))
        .and_then(decode_token)
        .is_some_and(|token| bool::from(token.ct_eq(secret)));
    if !authorized {
        return Err(401);
    }
    if !matches!(
        content_type.as_deref(),
        Some(
            "application/json"
                | "application/json; charset=utf-8"
                | "application/json;charset=utf-8"
        )
    ) {
        return Err(415);
    }
    let length = length.ok_or(411_u16)?;
    if length == 0 || length > COLLECTOR_LIMITS.max_body_bytes {
        return Err(413);
    }
    let mut body = buffer[header_end..].to_vec();
    if body.len() > length {
        return Err(400);
    }
    while body.len() < length {
        timeout(stream, deadline)?;
        let mut chunk = [0_u8; 4096];
        let requested = chunk.len().min(length - body.len());
        let count = match stream.read(&mut chunk[..requested]) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(408),
        };
        if count == 0 {
            return Err(400);
        }
        body.extend_from_slice(&chunk[..count]);
    }
    let body = serde_json::from_slice(&body).map_err(|_| 400_u16)?;
    Ok(Request { route, body })
}

fn handle(
    stream: &mut TcpStream,
    address: SocketAddr,
    secret: &[u8; 32],
    store: &Mutex<ActivityStore>,
    rate: &Mutex<RateLimit>,
) {
    let deadline = Instant::now() + IO_DEADLINE;
    if !rate
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .allow()
    {
        respond(
            stream,
            429,
            &json!({"code":8,"message":"Collector limit reached"}),
            deadline,
        );
        return;
    }
    let request = match read_request(stream, address, secret, deadline) {
        Ok(request) => request,
        Err(status) => {
            respond(
                stream,
                status,
                &json!({"code":3,"message":"Invalid request"}),
                deadline,
            );
            return;
        }
    };
    let batch = match request.route {
        Route::Logs => normalize::otlp(&request.body, false, now_ms()),
        Route::Traces => normalize::otlp(&request.body, true, now_ms()),
        Route::Activity => normalize::normalized(&request.body),
        Route::CodexHook => normalize::normalize_hook(Provider::Codex, &request.body, now_ms())
            .map(|spans| normalize::Batch { spans, dropped: 0 }),
        Route::ClaudeHook => normalize::normalize_hook(Provider::Claude, &request.body, now_ms())
            .map(|spans| normalize::Batch { spans, dropped: 0 }),
        Route::Snapshot => {
            if request
                .body
                .as_object()
                .is_none_or(|value| !value.is_empty())
            {
                respond(
                    stream,
                    400,
                    &json!({"code":3,"message":"Invalid request"}),
                    deadline,
                );
                return;
            }
            let snapshot = store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .snapshot();
            if let Ok(snapshot) = serde_json::to_value(snapshot) {
                respond(stream, 200, &snapshot, deadline);
            }
            return;
        }
    };
    let batch = match batch {
        Ok(batch) => batch,
        Err(error) => {
            let status = if matches!(error, ActivityError::LimitExceeded) {
                413
            } else {
                400
            };
            respond(
                stream,
                status,
                &json!({"code":3,"message":"Invalid activity metadata"}),
                deadline,
            );
            return;
        }
    };
    let (accepted, dropped) = store
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .ingest(batch.spans, batch.dropped);
    let response = if let Some(key) = request.route.rejected_record_field() {
        if dropped > 0 {
            json!({"partialSuccess": {key: dropped.to_string(), "errorMessage":"Unsupported or invalid records were rejected"}})
        } else {
            json!({})
        }
    } else {
        json!({"accepted":accepted,"dropped":dropped})
    };
    respond(stream, 200, &response, deadline);
}

fn respond(stream: &mut TcpStream, status: u16, body: &Value, deadline: Instant) {
    let Ok(body) = serde_json::to_vec(body) else {
        return;
    };
    if timeout(stream, deadline).is_err() {
        return;
    }
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        _ => "Bad Request",
    };
    let header = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n", body.len());
    for bytes in [header.as_bytes(), body.as_slice()] {
        let mut remaining = bytes;
        while !remaining.is_empty() {
            if timeout(stream, deadline).is_err() {
                return;
            }
            match stream.write(remaining) {
                Ok(0) => return,
                Ok(written) => remaining = &remaining[written..],
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_caps_each_window_and_resets() {
        let mut rate = RateLimit::new();
        for _ in 0..COLLECTOR_LIMITS.requests_per_second {
            assert!(rate.allow());
        }
        assert!(!rate.allow());
        rate.window = Instant::now() - Duration::from_secs(2);
        assert!(rate.allow());
        assert_eq!(rate.count, 1);
    }
}
