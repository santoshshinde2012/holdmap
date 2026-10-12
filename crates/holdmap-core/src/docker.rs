//! Minimal, synchronous Docker Engine API client (also works with Podman, OrbStack, Colima and
//! Rancher Desktop, which expose the same API). Speaks HTTP/1.1 over a Unix socket, a Windows
//! named pipe, or plain TCP, with short timeouts so a stopped daemon never stalls a scan.

use crate::model::ContainerInfo;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_millis(1500);
const MAX_HEADER: usize = 64 * 1024;
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

/// Where a container runtime API is reachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// Unix domain socket (`/var/run/docker.sock`, OrbStack, Colima, Podman).
    Unix(PathBuf),
    /// TCP endpoint from `DOCKER_HOST=tcp://…`.
    Tcp(String),
    /// Windows named pipe (`//./pipe/docker_engine`).
    Pipe(String),
}

/// A container runtime endpoint plus a human label ("Docker Desktop", "OrbStack", …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    /// Where the runtime API is reachable.
    pub endpoint: Endpoint,
    /// Human label such as "Docker Desktop" or "OrbStack".
    pub label: String,
}

/// A container publishing one host port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedPort {
    /// Port published on the host.
    pub host_port: u16,
    /// Protocol as reported by the runtime (`tcp` / `udp`).
    pub protocol: String,
    /// Host address the port is bound to, when not all interfaces.
    pub host_ip: Option<String>,
    /// The container that publishes the port.
    pub container: ContainerInfo,
    /// The runtime that owns the container.
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

/// Parse a `DOCKER_HOST` value (`unix://…`, `tcp://…`, `npipe://…`) into an endpoint.
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

trait Stream: Read + Write {
    fn read_before(&mut self, buf: &mut [u8], deadline: Instant) -> io::Result<usize>;
    fn write_before(&mut self, buf: &[u8], deadline: Instant) -> io::Result<()>;
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "runtime request timed out",
        ))
    } else {
        Ok(left)
    }
}

/// The OS cannot guarantee that opening a pipe can be cancelled. Keep the caller
/// within its deadline and bound the number of workers an unresponsive driver can retain.
#[cfg(any(windows, test))]
fn pipe_open_before<T: Send + 'static>(
    active: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
    deadline: Instant,
    open: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc};
    remaining(deadline)?;
    let mut count = active.load(Ordering::Acquire);
    loop {
        if count >= 4 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "runtime pipe opens are busy",
            ));
        }
        match active.compare_exchange_weak(count, count + 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(current) => count = current,
        }
    }
    struct OpenWorker(Arc<AtomicUsize>);
    impl Drop for OpenWorker {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let worker = OpenWorker(Arc::clone(active));
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("holdmap-runtime-pipe-open".into())
        .spawn(move || {
            let _worker = worker;
            // If the caller timed out, sending fails and drops the late handle here.
            let _ = sender.send(open());
        })?;
    match receiver.recv_timeout(remaining(deadline)?) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "runtime pipe open timed out",
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(io::Error::other("runtime pipe open worker failed"))
        }
    }
}

/// A PIPE_NOWAIT write may make partial progress or succeed with zero bytes written.
/// Retry backpressure within the caller's deadline, preserving every byte already sent.
#[cfg(any(windows, test))]
fn write_nonblocking_before(
    writer: &mut impl Write,
    mut bytes: &[u8],
    deadline: Instant,
) -> io::Result<()> {
    while !bytes.is_empty() {
        remaining(deadline)?;
        match writer.write(bytes) {
            Ok(0) => std::thread::sleep(remaining(deadline)?.min(Duration::from_millis(5))),
            Ok(written) => bytes = &bytes[written..],
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(remaining(deadline)?.min(Duration::from_millis(5)));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

impl Stream for std::net::TcpStream {
    fn read_before(&mut self, buf: &mut [u8], deadline: Instant) -> io::Result<usize> {
        self.set_read_timeout(Some(remaining(deadline)?))?;
        self.read(buf)
    }
    fn write_before(&mut self, buf: &[u8], deadline: Instant) -> io::Result<()> {
        self.set_write_timeout(Some(remaining(deadline)?))?;
        self.write_all(buf)
    }
}

#[cfg(unix)]
impl Stream for std::os::unix::net::UnixStream {
    fn read_before(&mut self, buf: &mut [u8], deadline: Instant) -> io::Result<usize> {
        use std::os::fd::AsRawFd;
        loop {
            remaining(deadline)?;
            match self.read(buf) {
                Ok(read) => return Ok(read),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_unix(self.as_raw_fd(), libc::POLLIN, deadline)?;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
    }
    fn write_before(&mut self, mut buf: &[u8], deadline: Instant) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        while !buf.is_empty() {
            remaining(deadline)?;
            match self.write(buf) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(written) => buf = &buf[written..],
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_unix(self.as_raw_fd(), libc::POLLOUT, deadline)?;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

/// Connected Unix streams remain nonblocking. Updating SO_RCVTIMEO after a peer closes
/// fails on macOS even when response bytes are still buffered, so wait for readiness instead.
#[cfg(unix)]
fn wait_unix(fd: std::os::fd::RawFd, events: libc::c_short, deadline: Instant) -> io::Result<()> {
    loop {
        let mut poll = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        let timeout_ms = remaining(deadline)?.as_millis().clamp(1, i32::MAX as u128) as i32;
        // SAFETY: the caller keeps its owned stream alive; poll refers to one initialized fd.
        let result = unsafe { libc::poll(&mut poll, 1, timeout_ms) };
        if result > 0 {
            if poll.revents & libc::POLLNVAL != 0 {
                return Err(io::Error::from_raw_os_error(libc::EBADF));
            }
            // Read/write determines the outcome of POLLHUP/POLLERR, including draining
            // response bytes before EOF when the daemon closed immediately after sending.
            return Ok(());
        }
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}

impl Stream for std::fs::File {
    fn write_before(&mut self, buf: &[u8], deadline: Instant) -> io::Result<()> {
        #[cfg(windows)]
        {
            write_nonblocking_before(self, buf, deadline)
        }
        #[cfg(not(windows))]
        {
            remaining(deadline)?;
            self.write_all(buf)
        }
    }
    fn read_before(&mut self, buf: &mut [u8], deadline: Instant) -> io::Result<usize> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::{
                GetLastError, ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED,
            };
            loop {
                remaining(deadline)?;
                let mut available = 0;
                // SAFETY: this file owns the live named-pipe handle. Checking availability
                // before reading keeps a daemon holding the pipe open within the deadline.
                if unsafe {
                    windows_sys::Win32::System::Pipes::PeekNamedPipe(
                        self.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                } == 0
                {
                    // SAFETY: get this thread's error immediately after the failed call.
                    let error = unsafe { GetLastError() };
                    return if matches!(error, ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED) {
                        Ok(0)
                    } else {
                        Err(io::Error::from_raw_os_error(error as i32))
                    };
                }
                if available > 0 {
                    let len = buf.len().min(available as usize);
                    return self.read(&mut buf[..len]);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (buf, deadline);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "named pipes unsupported",
            ))
        }
    }
}

#[cfg(unix)]
fn connect_unix(
    path: &std::path::Path,
    deadline: Instant,
) -> io::Result<std::os::unix::net::UnixStream> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: sockaddr_un is a plain C structure for which all-zero bytes are valid.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.is_empty() || bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid runtime socket path",
        ));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (dest, byte) in address.sun_path.iter_mut().zip(bytes) {
        *dest = *byte as libc::c_char;
    }
    let length = std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1;
    #[cfg(target_os = "macos")]
    {
        address.sun_len = length as u8;
    }
    // SAFETY: socket has no pointer arguments and returns a new descriptor.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: socket returned a new owned descriptor; the stream closes it on every error.
    let stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
    // SAFETY: fd is owned and live; only change its close-on-exec descriptor flag.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    #[cfg(target_vendor = "apple")]
    {
        let enabled: libc::c_int = 1;
        // SAFETY: the socket is live and the option points to a correctly sized integer.
        // Match std's socket initialization: writes to a closed peer must not send SIGPIPE.
        if unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_NOSIGPIPE,
                (&enabled as *const libc::c_int).cast(),
                std::mem::size_of_val(&enabled) as libc::socklen_t,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    stream.set_nonblocking(true)?;
    loop {
        remaining(deadline)?;
        // SAFETY: address is initialized, length fits it, and stream owns the descriptor.
        if unsafe {
            libc::connect(
                fd,
                (&address as *const libc::sockaddr_un).cast(),
                length as libc::socklen_t,
            )
        } == 0
        {
            break;
        }
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            continue;
        }
        if error.kind() == io::ErrorKind::WouldBlock {
            // Linux reports EAGAIN for a full Unix listener backlog without beginning a
            // connection. Retry it within the same deadline instead of blocking in connect.
            std::thread::sleep(remaining(deadline)?.min(Duration::from_millis(5)));
            continue;
        }
        if !matches!(
            error.raw_os_error(),
            Some(libc::EINPROGRESS | libc::EALREADY)
        ) {
            return Err(error);
        }
        loop {
            let mut poll = libc::pollfd {
                fd: stream.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            let timeout_ms = remaining(deadline)?.as_millis().clamp(1, i32::MAX as u128) as i32;
            // SAFETY: poll points to one initialized descriptor for this owned stream.
            let result = unsafe { libc::poll(&mut poll, 1, timeout_ms) };
            if result > 0 {
                if let Some(error) = stream.take_error()? {
                    return Err(error);
                }
                return Ok(stream);
            }
            if result == 0 {
                remaining(deadline)?;
                continue;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
    Ok(stream)
}

fn resolve_tcp(addr: &str, deadline: Instant) -> io::Result<Vec<std::net::SocketAddr>> {
    if let Ok(addr) = addr.parse() {
        return Ok(vec![addr]);
    }
    use std::net::ToSocketAddrs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    // The system resolver has no cancellation API. Bound outstanding lookups so a broken
    // directory/DNS service cannot stall scans or accumulate an unlimited number of threads.
    static LOOKUPS: AtomicUsize = AtomicUsize::new(0);
    let mut count = LOOKUPS.load(Ordering::Acquire);
    loop {
        if count >= 4 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "runtime DNS lookups are busy",
            ));
        }
        match LOOKUPS.compare_exchange_weak(count, count + 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(current) => count = current,
        }
    }
    let addr = addr.to_string();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let task = std::thread::Builder::new()
        .name("holdmap-runtime-dns".into())
        .spawn(move || {
            struct Lookup;
            impl Drop for Lookup {
                fn drop(&mut self) {
                    LOOKUPS.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _lookup = Lookup;
            let result = addr
                .to_socket_addrs()
                .map(|addresses| addresses.take(16).collect());
            let _ = sender.send(result);
        });
    if let Err(error) = task {
        LOOKUPS.fetch_sub(1, Ordering::AcqRel);
        return Err(error);
    }
    receiver
        .recv_timeout(remaining(deadline)?)
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "runtime DNS lookup timed out"))?
}

fn connect(ep: &Endpoint, deadline: Instant) -> io::Result<Box<dyn Stream>> {
    match ep {
        #[cfg(unix)]
        Endpoint::Unix(p) => {
            let s = connect_unix(p, deadline)?;
            Ok(Box::new(s))
        }
        #[cfg(not(unix))]
        Endpoint::Unix(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unix sockets unsupported",
        )),
        Endpoint::Tcp(addr) => {
            let mut error = io::Error::new(io::ErrorKind::InvalidInput, "bad DOCKER_HOST");
            for address in resolve_tcp(addr, deadline)? {
                match std::net::TcpStream::connect_timeout(&address, remaining(deadline)?) {
                    Ok(stream) => return Ok(Box::new(stream)),
                    Err(failed) => error = failed,
                }
            }
            Err(error)
        }
        #[cfg(windows)]
        Endpoint::Pipe(name) => {
            use std::os::windows::io::AsRawHandle;
            use std::sync::{atomic::AtomicUsize, Arc, OnceLock};
            use windows_sys::Win32::System::Pipes::{
                SetNamedPipeHandleState, PIPE_NOWAIT, PIPE_READMODE_BYTE,
            };
            static OPENS: OnceLock<Arc<AtomicUsize>> = OnceLock::new();
            let active = OPENS.get_or_init(|| Arc::new(AtomicUsize::new(0)));
            let name = name.clone();
            let f = pipe_open_before(active, deadline, move || {
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(name)
            })?;
            let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
            // SAFETY: f owns the live client pipe handle with GENERIC_WRITE access. Byte
            // reads and nonblocking waits keep both reads and writes within our deadline.
            if unsafe {
                SetNamedPipeHandleState(
                    f.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
                    &mode,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(Box::new(f))
        }
        #[cfg(not(windows))]
        Endpoint::Pipe(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "named pipes unsupported",
        )),
    }
}

/// Perform one HTTP request; returns (status, body).
pub fn request(ep: &Endpoint, method: &str, path: &str) -> io::Result<(u16, Vec<u8>)> {
    request_with_timeout(ep, method, path, TIMEOUT)
}

fn request_with_timeout(
    ep: &Endpoint,
    method: &str,
    path: &str,
    timeout: Duration,
) -> io::Result<(u16, Vec<u8>)> {
    if method.is_empty()
        || method.len() > 32
        || !method.bytes().all(|b| b.is_ascii_uppercase())
        || !crate::http::is_safe_path(path)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid runtime request",
        ));
    }
    let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "runtime timeout is too large")
    })?;
    let mut s = connect(ep, deadline)?;
    remaining(deadline)?;
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: docker\r\nUser-Agent: holdmap\r\nAccept: application/json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    s.write_before(req.as_bytes(), deadline)?;
    s.flush()?;
    read_response(s.as_mut(), deadline)
}

fn read_response(s: &mut dyn Stream, deadline: Instant) -> io::Result<(u16, Vec<u8>)> {
    let mut buf = Vec::new();
    let mut framing = None;
    let mut header_end = 0;
    let mut header_scan = 0;
    let mut chunks = ChunkProgress::default();
    // Read until EOF (Connection: close) or until a full response has been received.
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match s.read_before(&mut chunk, deadline) {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() + n > MAX_RESPONSE {
                    return Err(bad_response("runtime response is too large"));
                }
                buf.extend_from_slice(&chunk[..n]);
                if framing.is_none() {
                    if let Some(pos) = buf[header_scan..].windows(4).position(|w| w == b"\r\n\r\n")
                    {
                        header_end = header_scan + pos + 4;
                        if header_end > MAX_HEADER {
                            return Err(bad_response("runtime headers are too large"));
                        }
                        let head = std::str::from_utf8(&buf[..header_end - 4])
                            .map_err(|_| bad_response("malformed HTTP headers"))?;
                        framing = Some(parse_head(head)?.1);
                    } else {
                        if buf.len() >= MAX_HEADER {
                            return Err(bad_response("runtime headers are too large"));
                        }
                        header_scan = buf.len().saturating_sub(3);
                    }
                }
                let complete = match framing {
                    Some(Framing::Length(len)) => buf.len() - header_end >= len,
                    Some(Framing::Chunked) => chunks.complete(&buf[header_end..])?,
                    _ => false,
                };
                if complete {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    parse_response(&buf)
}

fn split_head(buf: &[u8]) -> Option<(&str, &[u8])> {
    let pos = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    Some((std::str::from_utf8(&buf[..pos]).ok()?, &buf[pos + 4..]))
}

fn bad_response(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Clone, Copy)]
enum Framing {
    Length(usize),
    Chunked,
    Close,
}

fn parse_head(head: &str) -> io::Result<(u16, Framing)> {
    let mut lines = head.split("\r\n");
    let mut status_line = lines.next().unwrap_or("").split_whitespace();
    let version = status_line.next().unwrap_or("");
    let status = status_line
        .next()
        .filter(|s| s.len() == 3)
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|s| (100..=599).contains(s))
        .ok_or_else(|| bad_response("malformed HTTP status"))?;
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(bad_response("unsupported HTTP version"));
    }
    let mut length = None;
    let mut chunked = false;
    for line in lines {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| bad_response("malformed HTTP header"))?;
        let value = value.trim();
        if key.eq_ignore_ascii_case("content-length") {
            if length.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad_response("invalid Content-Length"));
            }
            let len = value
                .parse::<usize>()
                .map_err(|_| bad_response("invalid Content-Length"))?;
            if len > MAX_RESPONSE {
                return Err(bad_response("runtime response is too large"));
            }
            length = Some(len);
        } else if key.eq_ignore_ascii_case("transfer-encoding") {
            if chunked || !value.eq_ignore_ascii_case("chunked") {
                return Err(bad_response("unsupported Transfer-Encoding"));
            }
            chunked = true;
        }
    }
    if chunked && length.is_some() {
        return Err(bad_response("ambiguous HTTP response framing"));
    }
    let framing = if status == 204 || status == 304 || (100..200).contains(&status) {
        Framing::Length(0)
    } else if chunked {
        Framing::Chunked
    } else if let Some(length) = length {
        Framing::Length(length)
    } else {
        Framing::Close
    };
    Ok((status, framing))
}

/// Walk chunk boundaries only once as bytes arrive, including optional trailer headers.
#[derive(Default)]
struct ChunkProgress {
    cursor: usize,
    pending_end: Option<usize>,
    trailers: bool,
    trailer_scan: usize,
}

impl ChunkProgress {
    fn complete(&mut self, body: &[u8]) -> io::Result<bool> {
        loop {
            if self.trailers {
                let trailers = &body[self.cursor..];
                if trailers.starts_with(b"\r\n") {
                    return Ok(true);
                }
                if let Some(end) = trailers[self.trailer_scan..]
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                {
                    if self.trailer_scan + end + 4 > MAX_HEADER {
                        return Err(bad_response("runtime trailers are too large"));
                    }
                    return Ok(true);
                }
                if trailers.len() >= MAX_HEADER {
                    return Err(bad_response("runtime trailers are too large"));
                }
                self.trailer_scan = trailers.len().saturating_sub(3);
                return Ok(false);
            }
            if let Some(end) = self.pending_end {
                if body.len() < end {
                    return Ok(false);
                }
                if &body[end - 2..end] != b"\r\n" {
                    return Err(bad_response("malformed HTTP chunk terminator"));
                }
                self.cursor = end;
                self.pending_end = None;
            }
            let rest = &body[self.cursor..];
            let Some(line_end) = rest.windows(2).position(|w| w == b"\r\n") else {
                if rest.len() >= MAX_HEADER {
                    return Err(bad_response("runtime chunk header is too large"));
                }
                return Ok(false);
            };
            if line_end + 2 > MAX_HEADER {
                return Err(bad_response("runtime chunk header is too large"));
            }
            let size_str = std::str::from_utf8(&rest[..line_end])
                .map_err(|_| bad_response("malformed HTTP chunk size"))?;
            let size_str = size_str.split(';').next().unwrap_or("");
            if size_str.is_empty() || !size_str.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(bad_response("malformed HTTP chunk size"));
            }
            let size = usize::from_str_radix(size_str, 16)
                .map_err(|_| bad_response("malformed HTTP chunk size"))?;
            self.cursor += line_end + 2;
            if size == 0 {
                self.trailers = true;
                continue;
            }
            let end = self
                .cursor
                .checked_add(size)
                .and_then(|n| n.checked_add(2))
                .filter(|n| *n <= MAX_RESPONSE)
                .ok_or_else(|| bad_response("runtime response is too large"))?;
            self.pending_end = Some(end);
        }
    }
}

#[cfg(test)]
fn response_complete(buf: &[u8]) -> bool {
    let Some((head, body)) = split_head(buf) else {
        return false;
    };
    match parse_head(head).map(|(_, framing)| framing) {
        Ok(Framing::Length(len)) => body.len() >= len,
        Ok(Framing::Chunked) => ChunkProgress::default().complete(body).unwrap_or(false),
        _ => false,
    }
}

/// Parse an HTTP/1.1 response, de-chunking if necessary.
pub fn parse_response(buf: &[u8]) -> io::Result<(u16, Vec<u8>)> {
    let bad = || bad_response("malformed HTTP response");
    if buf.len() > MAX_RESPONSE {
        return Err(bad_response("runtime response is too large"));
    }
    let (head, body) = split_head(buf).ok_or_else(bad)?;
    if head.len() + 4 > MAX_HEADER {
        return Err(bad_response("runtime headers are too large"));
    }
    let (status, framing) = parse_head(head)?;
    match framing {
        Framing::Length(length) => {
            return Ok((status, body.get(..length).ok_or_else(bad)?.to_vec()));
        }
        Framing::Close => return Ok((status, body.to_vec())),
        Framing::Chunked => {}
    }
    if !ChunkProgress::default().complete(body)? {
        return Err(bad());
    }
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let line_end = rest.windows(2).position(|w| w == b"\r\n").ok_or_else(bad)?;
        let size_str = std::str::from_utf8(&rest[..line_end]).map_err(|_| bad())?;
        let size = usize::from_str_radix(size_str.split(';').next().unwrap_or(""), 16)
            .map_err(|_| bad())?;
        rest = &rest[line_end + 2..];
        if size == 0 {
            break;
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
    Ok((status, out))
}

/// Parse `GET /containers/json` output into published ports.
pub fn parse_containers(body: &[u8], runtime: &Runtime) -> io::Result<Vec<PublishedPort>> {
    let list: Vec<ApiContainer> =
        serde_json::from_slice(body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
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
            if seen.insert((public, p.kind.clone(), c.id.clone())) {
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
    let mut seen = HashSet::new();
    let mut any = false;
    for rt in discover() {
        if let Ok((200, body)) = request(&rt.endpoint, "GET", "/containers/json") {
            any = true;
            if let Ok(ports) = parse_containers(&body, &rt) {
                for p in ports {
                    if seen.insert((p.container.id.clone(), p.host_port, p.protocol.clone())) {
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
    if id.is_empty()
        || id.len() > 255
        || matches!(id, "." | "..")
        || !id
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
        Duration::from_secs(timeout_s.checked_add(5).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "container timeout is too large",
            )
        })?),
    )?;
    match status {
        204 | 304 => Ok(()),
        404 => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "container not found",
        )),
        _ => Err(io::Error::other(format!(
            "runtime returned HTTP {status}: {}",
            crate::redact::line(&String::from_utf8_lossy(&body))
                .chars()
                .take(2000)
                .collect::<String>()
                .trim()
        ))),
    }
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

    fn wait_for_pipe_workers(active: &std::sync::atomic::AtomicUsize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while active.load(std::sync::atomic::Ordering::Acquire) != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(active.load(std::sync::atomic::Ordering::Acquire), 0);
    }

    #[test]
    fn pipe_open_results_and_expired_deadlines_preserve_errors_without_starting_work() {
        use std::sync::{atomic::AtomicUsize, Arc};
        let active = Arc::new(AtomicUsize::new(0));
        assert_eq!(
            pipe_open_before(&active, Instant::now() + TIMEOUT, || Ok(42)).unwrap(),
            42
        );
        assert_eq!(
            pipe_open_before::<()>(&active, Instant::now() + TIMEOUT, || {
                Err(io::Error::from(io::ErrorKind::NotFound))
            })
            .unwrap_err()
            .kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            pipe_open_before(&active, Instant::now(), || -> io::Result<()> {
                panic!("an expired request must not start an OS open")
            })
            .unwrap_err()
            .kind(),
            io::ErrorKind::TimedOut
        );
        wait_for_pipe_workers(&active);
    }

    #[test]
    fn pipe_open_timeout_drops_late_handles_and_releases_capacity() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::{mpsc, Arc};
        #[derive(Debug)]
        struct Handle(Arc<AtomicBool>);
        impl Drop for Handle {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let active = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let late_handle = Handle(Arc::clone(&dropped));
        let (release, released) = mpsc::channel();
        let started = Instant::now();
        let error = pipe_open_before(&active, started + Duration::from_millis(30), move || {
            released.recv_timeout(Duration::from_secs(2)).unwrap();
            Ok(late_handle)
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_millis(300));
        assert!(!dropped.load(Ordering::Acquire));
        assert_eq!(active.load(Ordering::Acquire), 1);
        release.send(()).unwrap();
        wait_for_pipe_workers(&active);
        assert!(dropped.load(Ordering::Acquire));
    }

    #[test]
    fn pipe_open_workers_cannot_accumulate_when_the_os_remains_stalled() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::{mpsc, Arc};
        let active = Arc::new(AtomicUsize::new(0));
        let mut releases = Vec::new();
        for _ in 0..4 {
            let (release, released) = mpsc::channel();
            releases.push(release);
            assert_eq!(
                pipe_open_before::<()>(
                    &active,
                    Instant::now() + Duration::from_millis(20),
                    move || {
                        released.recv_timeout(Duration::from_secs(2)).unwrap();
                        Ok(())
                    }
                )
                .unwrap_err()
                .kind(),
                io::ErrorKind::TimedOut
            );
        }
        assert_eq!(active.load(Ordering::Acquire), 4);
        let called = Arc::new(AtomicBool::new(false));
        let attempted = Arc::clone(&called);
        assert_eq!(
            pipe_open_before(&active, Instant::now() + TIMEOUT, move || {
                attempted.store(true, Ordering::Release);
                Ok(())
            })
            .unwrap_err()
            .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(!called.load(Ordering::Acquire));
        for release in releases {
            release.send(()).unwrap();
        }
        wait_for_pipe_workers(&active);
        assert!(pipe_open_before(&active, Instant::now() + TIMEOUT, || Ok(())).is_ok());
        wait_for_pipe_workers(&active);
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
    fn rejects_truncated_ambiguous_and_malformed_responses() {
        for raw in [
            b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nabc".as_slice(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nab",
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabcXX0\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFFFFFFFFFF\r\n",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nabc",
            b"NOTHTTP 200 OK\r\n\r\nabc",
            b"HTTP/1.1 999 OK\r\n\r\nabc",
        ] {
            assert!(parse_response(raw).is_err(), "must reject {raw:?}");
        }
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabcignored";
        assert_eq!(parse_response(raw).unwrap().1, b"abc");
    }

    #[test]
    fn incrementally_tracks_chunk_boundaries_and_trailers() {
        // A chunk can itself end in the terminal chunk's byte pattern. Only framing
        // boundaries may terminate the response, and trailer headers may follow it.
        let body = b"5\r\n0\r\n\r\n\r\n3;extension=value\r\nabc\r\n0\r\nX-Checksum: ok\r\n\r\n";
        let mut progress = ChunkProgress::default();
        for end in 0..body.len() {
            assert!(!progress.complete(&body[..end]).unwrap(), "ended at {end}");
        }
        assert!(progress.complete(body).unwrap());
        let mut response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        response.extend_from_slice(body);
        assert_eq!(parse_response(&response).unwrap().1, b"0\r\n\r\nabc");
    }

    struct FixtureStream(std::io::Cursor<Vec<u8>>);

    impl Read for FixtureStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl Write for FixtureStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Stream for FixtureStream {
        fn read_before(&mut self, buf: &mut [u8], deadline: Instant) -> io::Result<usize> {
            remaining(deadline)?;
            self.read(buf)
        }
        fn write_before(&mut self, buf: &[u8], deadline: Instant) -> io::Result<()> {
            remaining(deadline)?;
            self.write_all(buf)
        }
    }

    #[test]
    fn response_and_header_memory_are_bounded() {
        let head = b"HTTP/1.1 200 OK\r\n\r\n";
        let mut response = head.to_vec();
        response.resize(MAX_RESPONSE + 1, b'x');
        let mut stream = FixtureStream(std::io::Cursor::new(response));
        assert_eq!(
            read_response(&mut stream, Instant::now() + Duration::from_secs(1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let mut response = b"HTTP/1.1 200 OK\r\nX-Large: ".to_vec();
        response.resize(MAX_HEADER + 1, b'x');
        let mut stream = FixtureStream(std::io::Cursor::new(response));
        assert!(read_response(&mut stream, Instant::now() + Duration::from_secs(1)).is_err());
        let raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            MAX_RESPONSE + 1
        );
        assert!(parse_response(raw.as_bytes()).is_err());
    }

    #[test]
    fn complete_chunk_headers_and_trailers_enforce_the_exact_size_boundary() {
        let header = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        for size in [MAX_HEADER, MAX_HEADER + 1] {
            let bodies = [
                format!("1;{}\r\na\r\n0\r\n\r\n", "x".repeat(size - 4)),
                format!("1\r\na\r\n0\r\nX:{}\r\n\r\n", "x".repeat(size - 6)),
            ];
            for body in bodies {
                let mut response = header.to_vec();
                response.extend_from_slice(body.as_bytes());
                let parsed = parse_response(&response);
                let mut stream = FixtureStream(std::io::Cursor::new(response));
                let streamed = read_response(&mut stream, Instant::now() + Duration::from_secs(1));
                if size == MAX_HEADER {
                    assert_eq!(parsed.unwrap().1, b"a");
                    assert_eq!(streamed.unwrap().1, b"a");
                } else {
                    assert_eq!(parsed.unwrap_err().kind(), io::ErrorKind::InvalidData);
                    assert_eq!(streamed.unwrap_err().kind(), io::ErrorKind::InvalidData);
                }
            }
        }
    }

    #[test]
    fn nonblocking_writes_preserve_bytes_across_partial_and_zero_progress() {
        struct Writer {
            steps: std::collections::VecDeque<Result<usize, io::ErrorKind>>,
            bytes: Vec<u8>,
        }
        impl Write for Writer {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                let size = self
                    .steps
                    .pop_front()
                    .unwrap_or(Ok(bytes.len()))
                    .map_err(io::Error::from)?
                    .min(bytes.len());
                self.bytes.extend_from_slice(&bytes[..size]);
                Ok(size)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut writer = Writer {
            steps: [
                Err(io::ErrorKind::Interrupted),
                Ok(0),
                Ok(2),
                Err(io::ErrorKind::WouldBlock),
                Ok(1),
            ]
            .into(),
            bytes: Vec::new(),
        };
        write_nonblocking_before(
            &mut writer,
            b"abcdef",
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(writer.bytes, b"abcdef");
        writer.steps.push_back(Err(io::ErrorKind::BrokenPipe));
        assert_eq!(
            write_nonblocking_before(
                &mut writer,
                b"remaining",
                Instant::now() + Duration::from_secs(1)
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(writer.bytes, b"abcdef");
    }

    #[test]
    fn nonblocking_writes_time_out_when_the_peer_makes_no_progress() {
        struct Stalled;
        impl Write for Stalled {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Ok(0)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let started = Instant::now();
        let error = write_nonblocking_before(
            &mut Stalled,
            b"request",
            started + Duration::from_millis(30),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_millis(300));
        assert!(write_nonblocking_before(&mut Stalled, b"", started).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn windows_byte_pipe_writes_time_out_when_the_server_does_not_read() {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::Foundation::{
            GetLastError, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
        };
        use windows_sys::Win32::System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
        };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let name = format!(
            r"\\.\pipe\holdmap-core-stalled-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: the name is NUL-terminated and all pointer parameters are optional. The
        // returned handle is exclusively owned by this fixture until OwnedHandle closes it.
        let handle = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT,
                1,
                1024,
                1024,
                0,
                std::ptr::null(),
            )
        };
        assert_ne!(
            handle,
            INVALID_HANDLE_VALUE,
            "{}",
            io::Error::last_os_error()
        );
        // SAFETY: CreateNamedPipeW returned a new, valid owned handle.
        let server = unsafe { OwnedHandle::from_raw_handle(handle.cast()) };
        let mut client = connect(
            &Endpoint::Pipe(name),
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        // SAFETY: the server handle is live, synchronous and uses nonblocking wait mode.
        if unsafe { ConnectNamedPipe(server.as_raw_handle().cast(), std::ptr::null_mut()) } == 0 {
            // SAFETY: get this thread's error immediately after ConnectNamedPipe failed.
            assert_eq!(unsafe { GetLastError() }, ERROR_PIPE_CONNECTED);
        }
        let (release, released) = std::sync::mpsc::channel();
        // A watchdog closes the fixture even if a future regression blocks WriteFile, so
        // CI fails with a bounded assertion rather than hanging the test process forever.
        let watchdog = std::thread::spawn(move || {
            let _ = released.recv_timeout(Duration::from_millis(500));
            drop(server);
        });
        let started = Instant::now();
        let error = client
            .write_before(
                &vec![b'x'; 1024 * 1024],
                started + Duration::from_millis(80),
            )
            .unwrap_err();
        let _ = release.send(());
        watchdog.join().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_millis(350));
    }

    #[test]
    fn rejects_request_injection_before_connecting() {
        let ep = Endpoint::Tcp("127.0.0.1:1".into());
        for (method, path) in [("GET\r\nInjected:", "/"), ("GET", "/\r\nInjected: x")] {
            assert_eq!(
                request(&ep, method, path).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        for id in ["", ".", "..", "../../etc"] {
            assert_eq!(
                stop_container(&ep, id, 1).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        assert_eq!(
            stop_container(&ep, "fixture", u64::MAX).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn slow_tcp_responses_cannot_extend_the_total_deadline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::Tcp(listener.local_addr().unwrap().to_string());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            for byte in b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc" {
                if stream.write_all(&[*byte]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let started = Instant::now();
        let error =
            request_with_timeout(&endpoint, "GET", "/", Duration::from_millis(80)).unwrap_err();
        assert!(matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ));
        assert!(started.elapsed() < Duration::from_millis(400));
        server.join().unwrap();
    }

    #[test]
    fn complete_tcp_responses_do_not_wait_for_eof() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::Tcp(listener.local_addr().unwrap().to_string());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\n\r\n")
                .unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });
        assert_eq!(
            request_with_timeout(&endpoint, "POST", "/", Duration::from_millis(100)).unwrap(),
            (204, vec![])
        );
        server.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unix_responses_remain_readable_after_the_daemon_closes() {
        for response in [
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc".as_slice(),
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n",
            b"HTTP/1.1 200 OK\r\n\r\nabc",
        ] {
            let (mut client, mut server) = std::os::unix::net::UnixStream::pair().unwrap();
            client.set_nonblocking(true).unwrap();
            server.write_all(response).unwrap();
            drop(server);
            // On macOS changing a socket timeout now fails with EINVAL, even though the
            // complete response is buffered and can be read. Include EOF-delimited framing.
            assert_eq!(
                read_response(&mut client, Instant::now() + TIMEOUT).unwrap(),
                (200, b"abc".to_vec())
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_reads_and_backpressured_writes_keep_the_total_deadline() {
        let (mut client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        client.set_nonblocking(true).unwrap();
        let (release, released) = std::sync::mpsc::channel();
        // Close only this fixture if a regression blocks a syscall, so the suite cannot hang.
        let watchdog = std::thread::spawn(move || {
            let _ = released.recv_timeout(Duration::from_millis(500));
            drop(server);
        });
        let started = Instant::now();
        let read_error = client
            .read_before(&mut [0; 1], started + Duration::from_millis(40))
            .unwrap_err();
        let read_elapsed = started.elapsed();
        let started = Instant::now();
        let write_error = client
            .write_before(
                &vec![b'x'; 16 * 1024 * 1024],
                started + Duration::from_millis(60),
            )
            .unwrap_err();
        let write_elapsed = started.elapsed();
        let _ = release.send(());
        watchdog.join().unwrap();
        assert_eq!(read_error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(write_error.kind(), io::ErrorKind::TimedOut);
        assert!(read_elapsed < Duration::from_millis(350));
        assert!(write_elapsed < Duration::from_millis(350));
    }

    #[cfg(target_vendor = "apple")]
    #[test]
    fn apple_raw_unix_socket_closed_peer_cannot_raise_sigpipe() {
        const CHILD: &str = "HOLDMAP_TEST_UNIX_SIGPIPE_CHILD";
        if let Ok(mode) = std::env::var(CHILD) {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("runtime.sock");
            let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
            let mut client = connect_unix(&path, Instant::now() + TIMEOUT).unwrap();
            if mode == "unprotected" {
                use std::os::fd::AsRawFd;
                let disabled: libc::c_int = 0;
                // SAFETY: only the child's owned fixture socket is changed. The negative
                // control proves default SIGPIPE would terminate it without our option.
                assert_eq!(
                    unsafe {
                        libc::setsockopt(
                            client.as_raw_fd(),
                            libc::SOL_SOCKET,
                            libc::SO_NOSIGPIPE,
                            (&disabled as *const libc::c_int).cast(),
                            std::mem::size_of_val(&disabled) as libc::socklen_t,
                        )
                    },
                    0
                );
            }
            let (server, _) = listener.accept().unwrap();
            drop(server);
            assert_eq!(
                client
                    .read_before(&mut [0; 1], Instant::now() + TIMEOUT)
                    .unwrap(),
                0
            );
            // Rust main ignores SIGPIPE. Restore its terminating OS disposition only in
            // this isolated child, so missing raw-socket initialization kills the fixture.
            // SAFETY: the child owns its signal disposition and does not install a handler.
            assert_ne!(
                unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) },
                libc::SIG_ERR
            );
            assert_eq!(
                client
                    .write_before(b"request", Instant::now() + TIMEOUT)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::BrokenPipe
            );
            return;
        }
        for mode in ["protected", "unprotected"] {
            use std::os::unix::process::ExitStatusExt;
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "docker::tests::apple_raw_unix_socket_closed_peer_cannot_raise_sigpipe",
                    "--nocapture",
                ])
                .env(CHILD, mode)
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    if mode == "protected" {
                        assert!(status.success(), "closed-peer child failed: {status}");
                    } else {
                        assert_eq!(status.signal(), Some(libc::SIGPIPE));
                    }
                    break;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    let _ = child.wait();
                    panic!("closed-peer child exceeded its deadline");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_runtime_connections_and_responses_honor_the_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\n\r\n")
                .unwrap();
            std::thread::sleep(Duration::from_millis(100));
        });
        assert_eq!(
            request_with_timeout(
                &Endpoint::Unix(path),
                "POST",
                "/",
                Duration::from_millis(80)
            )
            .unwrap(),
            (204, vec![])
        );
        server.join().unwrap();
        assert!(connect_unix(std::path::Path::new(""), Instant::now() + TIMEOUT).is_err());
        assert!(connect_unix(std::path::Path::new("/nonexistent"), Instant::now()).is_err());
    }

    #[test]
    fn container_errors_hide_credentials_and_bound_displayed_output() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = Endpoint::Tcp(listener.local_addr().unwrap().to_string());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let body = format!("API_TOKEN=super-secret {}", "x".repeat(10_000));
            let response = format!(
                "HTTP/1.1 500 Error\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let error = stop_container(&endpoint, "fixture", 1)
            .unwrap_err()
            .to_string();
        assert!(!error.contains("super-secret"));
        assert!(error.chars().count() < 2100);
        server.join().unwrap();
    }

    #[test]
    fn tcp_name_resolution_has_a_deadline_and_numeric_endpoints_need_no_lookup() {
        let address: std::net::SocketAddr = "127.0.0.1:2375".parse().unwrap();
        assert_eq!(
            resolve_tcp("127.0.0.1:2375", Instant::now()).unwrap(),
            [address]
        );
        let addresses =
            resolve_tcp("localhost:2375", Instant::now() + Duration::from_secs(1)).unwrap();
        assert!(!addresses.is_empty());
        assert!(addresses.iter().all(|address| address.ip().is_loopback()));
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
