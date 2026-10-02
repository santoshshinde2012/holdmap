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
    Tcp,
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
    V4,
    V6,
}

/// Socket state. TCP states follow the kernel; unconnected UDP sockets are `Bound`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocketState {
    Listen,
    /// An unconnected UDP socket: the UDP equivalent of "listening".
    Bound,
    Established,
    SynSent,
    SynRecv,
    FinWait1,
    FinWait2,
    TimeWait,
    Close,
    CloseWait,
    LastAck,
    Closing,
    Unknown,
}

impl SocketState {
    /// True for sockets that accept traffic: TCP LISTEN and unconnected UDP.
    pub fn is_listening(self) -> bool {
        matches!(self, SocketState::Listen | SocketState::Bound)
    }

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
    pub protocol: Protocol,
    pub family: Family,
    pub local_addr: IpAddr,
    pub local_port: u16,
    pub remote_addr: Option<IpAddr>,
    pub remote_port: Option<u16>,
    pub state: SocketState,
    /// Owning uid if the platform reports it (Linux does, even for other users).
    pub uid: Option<u32>,
    /// Kernel inode (Linux) — used to map sockets to processes.
    pub inode: Option<u64>,
    /// Processes holding this socket open (may be empty if not visible).
    pub pids: Vec<u32>,
}

/// Information about one process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub exe: Option<PathBuf>,
    pub cmdline: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub uid: Option<u32>,
    pub user: Option<String>,
    /// Process start time as seconds since the Unix epoch.
    pub start_time: u64,
    /// Platform-specific high-resolution start token used for PID-reuse checks
    /// (Linux: `starttime` clock ticks from `/proc/<pid>/stat`; elsewhere: start_time).
    pub start_token: u64,
    pub memory_bytes: u64,
}

impl ProcessInfo {
    /// Command line joined for display.
    pub fn command(&self) -> String {
        if self.cmdline.is_empty() {
            self.name.clone()
        } else {
            self.cmdline.join(" ")
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
    DevServer,
    AppServer,
    Database,
    Cache,
    Queue,
    WebServer,
    Tool,
    Container,
    /// A desktop application (IDE helper, chat app, music player…).
    App,
    System,
}

impl FrameworkCategory {
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
    pub name: String,
    pub category: FrameworkCategory,
}

/// The project a process belongs to (from its working directory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub root: PathBuf,
    /// Manifest kind that identified the project: `package.json`, `Cargo.toml`, …
    pub kind: String,
    pub git_branch: Option<String>,
}

/// A container publishing a port (Docker / Podman / OrbStack / Colima).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub runtime: String,
    pub compose_project: Option<String>,
    pub compose_service: Option<String>,
    /// Port inside the container.
    pub private_port: u16,
}

/// One row: a port held by one owner (IPv4/IPv6 pairs are grouped into a single row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortEntry {
    /// Stable identifier for UI selection: `proto:port:pid:state[:remote]`.
    pub id: String,
    pub port: u16,
    pub protocol: Protocol,
    pub state: SocketState,
    /// Local bind addresses, e.g. `["0.0.0.0", "::"]`.
    pub addresses: Vec<String>,
    pub families: Vec<Family>,
    pub remote: Option<String>,
    pub exposure: Exposure,
    /// Primary owning process (the parent when several processes share the socket).
    pub pid: Option<u32>,
    /// Every process holding the socket (e.g. pre-fork servers, reloaders).
    pub pids: Vec<u32>,
    pub uid: Option<u32>,
    /// Owning user name, when known.
    pub user: Option<String>,
    pub process: Option<ProcessInfo>,
    pub project: Option<ProjectInfo>,
    pub framework: Option<Framework>,
    pub container: Option<ContainerInfo>,
    /// Short human label such as "Next.js · shop-web".
    pub label: String,
    /// Likely a development server / dev dependency (heuristic).
    pub is_dev: bool,
    /// Owned by the current user.
    pub is_mine: bool,
    /// Protected by safety policy (system / IDE / terminal / self).
    pub protected: bool,
}

/// A full scan of the machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub entries: Vec<PortEntry>,
    /// Listening sockets whose owning process is not visible to us (other users / root).
    pub hidden_sockets: usize,
    pub platform: String,
    /// Unix epoch milliseconds.
    pub taken_at_ms: u64,
    pub scan_ms: u64,
    pub docker_available: bool,
    /// Non-fatal problems hit while scanning.
    pub warnings: Vec<String>,
}
