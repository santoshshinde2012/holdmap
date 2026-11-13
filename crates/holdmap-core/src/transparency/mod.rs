//! Agent transparency: process memory counters and documented capability locations.
//!
//! Memory collectors are operating-system specific and sit behind [`MemoryProbe`].
//! Location census is a separate inspector that stats paths and does not read bodies.
//! Reports share these types across the CLI, MCP server and desktop app.

mod counters;
mod figure;
#[cfg(target_os = "linux")]
mod linux;
mod log;
#[cfg(target_os = "macos")]
mod macos;
mod places;
mod probe;
mod report;
#[cfg(target_os = "windows")]
mod windows;

pub use figure::ObservedU64;
pub use log::{persist_trends, persist_trends_default, FileMemoryLog, MemoryLog, NoMemoryLog};
pub use places::{census, CapabilityPlace, PlaceEntry, PlaceKind, PlaceScope, PlaceStatus};
pub use probe::{HostProbe, MemoryProbe, ProcessReading};
pub use report::{
    agent_memory, AgentMemoryReport, HeadlineCounter, MemoryGroup, MemoryRole, MemorySample,
    MemoryTrend, ProcessMemoryRow, TrendDirection,
};

#[cfg(test)]
mod tests;
