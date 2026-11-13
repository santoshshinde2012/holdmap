//! Platform memory collectors behind one interface.
//!
//! Each collector reports only counters that platform documents. Missing counters stay
//! unknown with a reason. Nothing here copies the process-table headline into another field.

use super::figure::ObservedU64;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// One process's platform counters, each independent of the others.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessReading {
    /// Resident set (Linux `VmRSS`, macOS `ri_resident_size`, Windows working set).
    pub resident: ObservedU64,
    /// macOS `ri_phys_footprint` (Activity Monitor's Memory column).
    pub physical_footprint: ObservedU64,
    /// Private resident memory, only where the OS splits it from shared.
    pub private_bytes: ObservedU64,
    /// Shared resident memory, only where the OS splits it from private.
    pub shared_bytes: ObservedU64,
    /// Linux proportional set size (`Pss`).
    pub proportional: ObservedU64,
    /// Compressed memory, only where a per-process counter exists.
    pub compressed: ObservedU64,
    /// Virtual address space.
    pub virtual_bytes: ObservedU64,
    /// Swapped or compressed-to-disk bytes the OS attributes to the process.
    pub swap_bytes: ObservedU64,
    /// Peak resident size since the process started, when the OS records one.
    pub peak_resident: ObservedU64,
    /// Peak physical footprint since the process started, when the OS records one.
    pub peak_physical: ObservedU64,
    /// Windows private commit charge. Not a working-set figure.
    pub commit_bytes: ObservedU64,
}

impl ProcessReading {
    /// Every counter unknown for the same reason.
    pub fn all_unknown(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        let one = || ObservedU64::unknown(reason.clone());
        Self {
            resident: one(),
            physical_footprint: one(),
            private_bytes: one(),
            shared_bytes: one(),
            proportional: one(),
            compressed: one(),
            virtual_bytes: one(),
            swap_bytes: one(),
            peak_resident: one(),
            peak_physical: one(),
            commit_bytes: one(),
        }
    }
}

/// Reads memory counters for one process and for the machine.
///
/// Implementations are operating-system collectors. Tests supply a fixed reading.
pub trait MemoryProbe: Send + Sync + Debug {
    /// Counters for `pid` from this collector's platform APIs.
    fn reading(&self, pid: u32) -> ProcessReading;
    /// Physical RAM installed on the machine, in bytes.
    fn machine_bytes(&self) -> ObservedU64;
}

/// The collector for the operating system this binary was built for.
#[derive(Debug, Default, Clone, Copy)]
pub struct HostProbe;

impl MemoryProbe for HostProbe {
    fn reading(&self, pid: u32) -> ProcessReading {
        platform::read(pid)
    }

    fn machine_bytes(&self) -> ObservedU64 {
        platform::machine_bytes()
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{ObservedU64, ProcessReading};
    pub(super) fn read(pid: u32) -> ProcessReading {
        super::super::linux::read(pid)
    }
    pub(super) fn machine_bytes() -> ObservedU64 {
        super::super::linux::machine_bytes()
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{ObservedU64, ProcessReading};
    pub(super) fn read(pid: u32) -> ProcessReading {
        super::super::macos::read(pid)
    }
    pub(super) fn machine_bytes() -> ObservedU64 {
        super::super::macos::machine_bytes()
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{ObservedU64, ProcessReading};
    pub(super) fn read(pid: u32) -> ProcessReading {
        super::super::windows::read(pid)
    }
    pub(super) fn machine_bytes() -> ObservedU64 {
        super::super::windows::machine_bytes()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{ObservedU64, ProcessReading};
    pub(super) fn read(_pid: u32) -> ProcessReading {
        ProcessReading::all_unknown(
            "This operating system has no memory-counter collector in holdmap.",
        )
    }
    pub(super) fn machine_bytes() -> ObservedU64 {
        ObservedU64::unknown("This operating system has no memory-counter collector in holdmap.")
    }
}
