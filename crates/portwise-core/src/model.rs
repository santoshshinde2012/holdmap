//! The data model shared by every portwise surface (CLI, TUI, desktop, MCP).
//!
//! Everything here is `Serialize`/`Deserialize`; the JSON produced from these types
//! (snake_case) is the public, stable contract of `portwise --json`.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;

/// Transport protocol of a socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// Transmission Control Protocol.
    Tcp,
    /// User Datagram Protocol.
    Udp,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Protocol::Tcp => "TCP",
            Protocol::Udp => "UDP",
        })
    }
}

/// IP family of a socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// IPv4.
    V4,
    /// IPv6.
    V6,
}

/// Socket state. TCP states follow the kernel; unconnected UDP sockets are `Bound`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocketState {
    /// Listening for connections (TCP) or bound (UDP).
    Listen,
    /// An unconnected UDP socket: the UDP equivalent of "listening".
    Bound,
    /// Connected.
    Established,
    /// Connection being opened (client side).
    SynSent,
    /// Connection being opened (server side).
    SynRecv,
    /// Closing (FIN sent).
    FinWait1,
    /// Closing (FIN acknowledged).
    FinWait2,
    /// Closed; waiting for stray packets.
    TimeWait,
    /// Closed.
    Close,
    /// Remote side closed; local side still open.
    CloseWait,
    /// Waiting for the final ACK.
    LastAck,
    /// Both sides closing at once.
    Closing,
    /// State not reported by the OS.
    Unknown,
}

impl SocketState {
    /// True for sockets that accept traffic: TCP LISTEN and unconnected UDP.
    pub fn is_listening(self) -> bool {
        matches!(self, SocketState::Listen | SocketState::Bound)
    }

    /// Lower-case name as shown by `ss` and the CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            SocketState::Listen => "LISTEN",
            SocketState::Bound => "BOUND",
            SocketState::Established => "ESTABLISHED",
            SocketState::SynSent => "SYN_SENT",
            SocketState::SynRecv => "SYN_RECV",
            SocketState::FinWait1 => "FIN_WAIT1",
            SocketState::FinWait2 => "FIN_WAIT2",
            SocketState::TimeWait => "TIME_WAIT",
            SocketState::Close => "CLOSE",
            SocketState::CloseWait => "CLOSE_WAIT",
            SocketState::LastAck => "LAST_ACK",
            SocketState::Closing => "CLOSING",
            SocketState::Unknown => "UNKNOWN",
        }
    }
}

impl fmt::Display for SocketState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A raw socket as reported by the platform backend, before grouping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawSocket {
    /// Transport protocol.
    pub protocol: Protocol,
    /// Address family.
    pub family: Family,
    /// Local bind address.
    pub local_addr: IpAddr,
    /// Local port.
    pub local_port: u16,
    /// Remote address (connected sockets).
    pub remote_addr: Option<IpAddr>,
    /// Remote port (connected sockets).
    pub remote_port: Option<u16>,
    /// Socket state.
    pub state: SocketState,
    /// Owning uid if the platform reports it (Linux does, even for other users).
    pub uid: Option<u32>,
    /// Kernel inode (Linux) — used to map sockets to processes.
    pub inode: Option<u64>,
    /// Processes holding this socket open (may be empty if not visible).
    pub pids: Vec<u32>,
}

/// Information about one process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// Process ID.
    pub pid: u32,
    /// Parent process ID.
    pub ppid: Option<u32>,
    /// Process name.
    pub name: String,
    /// Executable path, when readable.
    pub exe: Option<PathBuf>,
    /// Full command line (serialized with secrets hidden, see [`crate::redact`]).
    #[serde(serialize_with = "crate::redact::serialize_args")]
    pub cmdline: Vec<String>,
    /// Working directory, when readable.
    pub cwd: Option<PathBuf>,
    /// Owner UID.
    pub uid: Option<u32>,
    /// Owner user name.
    pub user: Option<String>,
    /// Process start time as seconds since the Unix epoch.
    pub start_time: u64,
    /// Platform-specific high-resolution start token used for PID-reuse checks
    /// (Linux: `starttime` clock ticks from `/proc/<pid>/stat`; elsewhere: start_time).
    pub start_token: u64,
    /// Resident memory in bytes.
    pub memory_bytes: u64,
    /// CPU usage since the previous scan in percent of one core (0 on the first scan).
    #[serde(default)]
    pub cpu_percent: f32,
}

impl ProcessInfo {
    /// Command line joined for display, with secrets hidden.
    pub fn command(&self) -> String {
        if self.cmdline.is_empty() {
            self.name.clone()
        } else {
            crate::redact::args(&self.cmdline).join(" ")
        }
    }
}

/// Where a listener can be reached from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exposure {
    /// Bound to loopback only (127.0.0.0/8, ::1) — reachable from this machine only.
    Loopback,
    /// Bound to all interfaces (0.0.0.0 / ::) — reachable from the network.
    AllInterfaces,
    /// Bound to a specific non-loopback address.
    Specific,
}

/// Broad category of what is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameworkCategory {
    /// Development server (Vite, Next.js, webpack…).
    DevServer,
    /// Application server (Django, Rails, FastAPI…).
    AppServer,
    /// Database (Postgres, MySQL, MongoDB…).
    Database,
    /// Cache (Redis, Memcached).
    Cache,
    /// Message queue (RabbitMQ, Kafka, NATS…).
    Queue,
    /// Web server or proxy (nginx, Caddy…).
    WebServer,
    /// Developer tool (language server, debugger, docs server…).
    Tool,
    /// A container.
    Container,
    /// A desktop application (IDE helper, chat app, music player…).
    App,
    /// Operating-system service.
    System,
}

impl FrameworkCategory {
    /// True for categories that count as development servers.
    pub fn is_dev(self) -> bool {
        matches!(
            self,
            FrameworkCategory::DevServer
                | FrameworkCategory::AppServer
                | FrameworkCategory::Database
                | FrameworkCategory::Cache
                | FrameworkCategory::Queue
                | FrameworkCategory::Tool
                | FrameworkCategory::Container
        )
    }
}

/// A detected framework/runtime label such as "Next.js" or "PostgreSQL".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Framework {
    /// Framework or product name, e.g. "Next.js".
    pub name: String,
    /// Broad category.
    pub category: FrameworkCategory,
}

/// The project a process belongs to (from its working directory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Project name (manifest name or directory name).
    pub name: String,
    /// Project root directory.
    pub root: PathBuf,
    /// Manifest kind that identified the project: `package.json`, `Cargo.toml`, …
    pub kind: String,
    /// Current git branch, when in a repository.
    pub git_branch: Option<String>,
    /// Enclosing monorepo / workspace (pnpm, turbo, nx, Cargo workspace, compose, …).
    #[serde(default)]
    pub workspace: Option<Workspace>,
    /// Root of the git repository the project lives in.
    #[serde(default)]
    pub git_root: Option<PathBuf>,
}

/// A directory that groups several projects/services.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Workspace {
    /// Workspace name.
    pub name: String,
    /// Workspace root directory.
    pub root: PathBuf,
    /// `pnpm`, `turbo`, `nx`, `lerna`, `npm-workspaces`, `cargo`, `go-work`, `compose`.
    pub kind: String,
}

/// A container publishing a port (Docker / Podman / OrbStack / Colima).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerInfo {
    /// Container ID.
    pub id: String,
    /// Container name.
    pub name: String,
    /// Image reference, e.g. `postgres:16`.
    pub image: String,
    /// Runtime label (Docker, OrbStack, Podman…).
    pub runtime: String,
    /// docker compose project, from the container labels.
    pub compose_project: Option<String>,
    /// docker compose service, from the container labels.
    pub compose_service: Option<String>,
    /// Port inside the container.
    pub private_port: u16,
}

/// One row: a port held by one owner (IPv4/IPv6 pairs are grouped into a single row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortEntry {
    /// Stable identifier for UI selection: `proto:port:pid:state[:remote]`.
    pub id: String,
    /// Port number.
    pub port: u16,
    /// Transport protocol.
    pub protocol: Protocol,
    /// Socket state.
    pub state: SocketState,
    /// Local bind addresses, e.g. `["0.0.0.0", "::"]`.
    pub addresses: Vec<String>,
    /// Address families the port is bound on.
    pub families: Vec<Family>,
    /// Remote endpoint (connected sockets only).
    pub remote: Option<String>,
    /// Whether the port is reachable from the network.
    pub exposure: Exposure,
    /// Primary owning process (the parent when several processes share the socket).
    pub pid: Option<u32>,
    /// Every process holding the socket (e.g. pre-fork servers, reloaders).
    pub pids: Vec<u32>,
    /// Owner UID.
    pub uid: Option<u32>,
    /// Owning user name, when known.
    pub user: Option<String>,
    /// Owning process, when visible.
    pub process: Option<ProcessInfo>,
    /// Detected project.
    pub project: Option<ProjectInfo>,
    /// Detected framework.
    pub framework: Option<Framework>,
    /// The container that publishes the port, if any.
    pub container: Option<ContainerInfo>,
    /// Short human label such as "Next.js · shop-web".
    pub label: String,
    /// Likely a development server / dev dependency (heuristic).
    pub is_dev: bool,
    /// Owned by the current user.
    pub is_mine: bool,
    /// Protected by safety policy (system / IDE / terminal / self).
    pub protected: bool,
    /// The listener is a tunnel / port-forward (kubectl, ssh -L, …).
    #[serde(default)]
    pub tunnel: Option<crate::tunnel::TunnelInfo>,
}

impl PortEntry {
    /// A listener whose owner portwise can't see (another user or root, without elevation).
    /// Published container ports with no host socket aren't hidden: the container is known.
    pub fn is_hidden(&self) -> bool {
        self.state.is_listening() && self.pids.is_empty() && self.container.is_none()
    }
}

/// A full scan of the machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Port entries, sorted by port.
    pub entries: Vec<PortEntry>,
    /// Listening sockets whose owning process is not visible to us (other users / root).
    pub hidden_sockets: usize,
    /// Platform name (`linux`, `macos`, `windows`).
    pub platform: String,
    /// Unix epoch milliseconds.
    pub taken_at_ms: u64,
    /// How long the scan took, in milliseconds.
    pub scan_ms: u64,
    /// True when a container runtime answered.
    pub docker_available: bool,
    /// Non-fatal problems hit while scanning.
    pub warnings: Vec<String>,
}
