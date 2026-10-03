//! Data-source abstractions (dependency inversion): the scanner depends on these traits, not on
//! the OS. System implementations talk to the kernel / sysinfo / container runtimes; static
//! implementations are injected by tests, fixtures and remote (SSH) snapshots.

use crate::docker::{self, PublishedPort};
use crate::model::RawSocket;
use crate::process::ProcessTable;
use std::io;
use std::sync::{Arc, Mutex, OnceLock};
use sysinfo::System;

/// Lists every socket (listening and connected) with the PIDs holding it.
pub trait SocketProvider: Send + Sync {
    /// Every socket on the machine.
    fn sockets(&self) -> io::Result<Vec<RawSocket>>;
}

/// Captures the process table.
pub trait ProcessProvider: Send + Sync {
    /// A snapshot of every process.
    fn processes(&self) -> ProcessTable;
}

/// Published container ports, and whether any runtime answered.
pub trait ContainerProvider: Send + Sync {
    /// Container-published host ports, and whether a runtime answered.
    fn published(&self) -> (Vec<PublishedPort>, bool);
}

/// The platform socket backend (`/proc/net`, `proc_pidfdinfo`, `GetExtendedTcpTable`, …).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemSockets;

impl SocketProvider for SystemSockets {
    fn sockets(&self) -> io::Result<Vec<RawSocket>> {
        crate::sys::list_sockets()
    }
}

/// sysinfo-backed process table. Keeps one `System` alive so CPU usage can be computed as the
/// delta between consecutive scans.
#[derive(Debug, Default)]
pub struct SystemProcesses {
    sys: Mutex<Option<System>>,
}

impl SystemProcesses {
    /// The process-wide instance (so every long-lived frontend gets CPU deltas for free).
    pub fn shared() -> Arc<SystemProcesses> {
        static S: OnceLock<Arc<SystemProcesses>> = OnceLock::new();
        S.get_or_init(|| Arc::new(SystemProcesses::default()))
            .clone()
    }
}

impl ProcessProvider for SystemProcesses {
    fn processes(&self) -> ProcessTable {
        let mut guard = self.sys.lock().unwrap_or_else(|e| e.into_inner());
        let sys = guard.get_or_insert_with(System::new);
        ProcessTable::capture_with(sys)
    }
}

/// Docker-API-compatible runtimes (Docker, Podman, OrbStack, Colima, Rancher).
#[derive(Debug, Default, Clone, Copy)]
pub struct DockerContainers;

impl ContainerProvider for DockerContainers {
    fn published(&self) -> (Vec<PublishedPort>, bool) {
        docker::published_ports()
    }
}

/// Fixed sockets (tests, fixtures, remote snapshots).
#[derive(Debug, Default, Clone)]
pub struct StaticSockets(pub Vec<RawSocket>);

impl SocketProvider for StaticSockets {
    fn sockets(&self) -> io::Result<Vec<RawSocket>> {
        Ok(self.0.clone())
    }
}

/// A fixed process table.
#[derive(Debug, Default, Clone)]
pub struct StaticProcesses(pub ProcessTable);

impl ProcessProvider for StaticProcesses {
    fn processes(&self) -> ProcessTable {
        self.0.clone()
    }
}

/// Fixed container ports; `Default` means "no runtime".
#[derive(Debug, Default, Clone)]
pub struct StaticContainers(pub Vec<PublishedPort>);

impl ContainerProvider for StaticContainers {
    fn published(&self) -> (Vec<PublishedPort>, bool) {
        (self.0.clone(), !self.0.is_empty())
    }
}
