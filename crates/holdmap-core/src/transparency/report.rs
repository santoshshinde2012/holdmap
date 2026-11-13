//! Per-agent memory: headline, platform counters, process groups, share of RAM and trend.
//!
//! The headline is the same sum holdmap already shows (`Agent::memory_bytes`): Linux `VmRSS`,
//! macOS physical footprint, Windows working set. Other counters come from [`super::MemoryProbe`]
//! and are never filled in from the headline. Peaks are kept per process. They are not added
//! into a tree peak, because those peaks did not happen at the same time.

use super::figure::{sum_observed, ObservedU64};
use super::probe::{MemoryProbe, ProcessReading};
use crate::agents::{Agent, AgentTool, Evidence, ProcessRole, ToolKind};
use crate::process::ProcessTable;
use serde::{Deserialize, Serialize};

/// Which headline the process table stored for this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadlineCounter {
    /// Linux `VmRSS`.
    Resident,
    /// macOS `ri_phys_footprint`.
    PhysicalFootprint,
    /// Windows working set.
    WorkingSet,
}

impl HeadlineCounter {
    /// The counter this build's process table uses as its one memory number.
    pub fn on_this_os() -> Self {
        if cfg!(target_os = "macos") {
            Self::PhysicalFootprint
        } else if cfg!(target_os = "windows") {
            Self::WorkingSet
        } else {
            Self::Resident
        }
    }

    /// One sentence naming the counter, for every surface.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Resident => {
                "Sum of Linux VmRSS samples taken with the process table. This is the agent's memory total."
            }
            Self::PhysicalFootprint => {
                "Sum of macOS physical-footprint samples taken with the process table. This is the agent's memory total and matches Activity Monitor's Memory column."
            }
            Self::WorkingSet => {
                "Sum of Windows working-set samples taken with the process table. This is the agent's memory total. It is not the private working set."
            }
        }
    }
}

/// How a process counts toward its agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRole {
    /// The agent's own process.
    Agent,
    /// A product helper that is not classified as a language server.
    Helper,
    /// A language server or similar worker, inferred from the process name.
    LanguageServer,
    /// A child whose command looks like an MCP server. The role is inferred.
    McpServer,
    /// Another process the agent started.
    Child,
}

impl MemoryRole {
    /// Short label shared by text surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Helper => "helper",
            Self::LanguageServer => "language server",
            Self::McpServer => "MCP server",
            Self::Child => "child",
        }
    }
}

/// One process in the agent's tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessMemoryRow {
    /// Process id.
    pub pid: u32,
    /// Process name. Command lines are not copied here.
    pub name: String,
    /// How it was grouped.
    pub role: MemoryRole,
    /// Whether the role was read from the process tree or inferred from a name.
    pub evidence: Evidence,
    /// The process-table sample included in the agent headline.
    pub scan_bytes: u64,
    /// Platform counters from a separate read.
    pub reading: ProcessReading,
    /// The process holds at least half of the headline and at least 256 MiB, and it is not
    /// the agent process itself.
    pub dominant: bool,
    /// The rule that set [`Self::dominant`], when it is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dominant_reason: Option<String>,
}

/// Headline and platform counters for one role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryGroup {
    /// Which processes were added.
    pub role: MemoryRole,
    /// Processes in the group.
    pub processes: u32,
    /// Sum of process-table samples. This always adds, including zeros the table stored.
    pub scan_bytes: u64,
    /// Platform counters. Unknown when any process in the group did not expose that counter.
    pub resident: ObservedU64,
    /// macOS physical footprint, summed only when every process exposed it.
    pub physical_footprint: ObservedU64,
    /// Private resident bytes, summed only when every process exposed them.
    pub private_bytes: ObservedU64,
    /// Shared resident bytes, summed only when every process exposed them.
    pub shared_bytes: ObservedU64,
    /// Linux proportional set size, summed only when every process exposed it.
    pub proportional: ObservedU64,
    /// Compressed bytes, summed only when every process exposed them.
    pub compressed: ObservedU64,
    /// Virtual size, summed only when every process exposed it.
    pub virtual_bytes: ObservedU64,
}

/// Whether retained samples moved, by a fixed rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrendDirection {
    /// The newest sample is more than 10% and more than 1 MiB above the oldest.
    Rising,
    /// The newest sample is more than 10% and more than 1 MiB below the oldest.
    Falling,
    /// At least two samples, inside that band.
    Flat,
    /// Fewer than two samples, or the series could not be kept.
    Unknown,
}

/// Samples holdmap itself retained for one process. Not an operating-system peak.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryTrend {
    /// Headline samples, oldest first. Bytes and timestamps only.
    pub samples: Vec<MemorySample>,
    /// Result of the fixed comparison, or unknown.
    pub direction: TrendDirection,
    /// Why the direction is unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown: Option<String>,
    /// The comparison, so a surface can show it instead of a hidden threshold.
    pub rule: String,
}

/// One retained headline sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySample {
    /// When the sample was taken, Unix milliseconds.
    pub at_ms: u64,
    /// Headline bytes at that time.
    pub bytes: u64,
}

const TREND_RULE: &str = "Direction compares the oldest and newest retained headline samples for this process id and start time. Rising or falling needs more than 10% and more than 1 MiB of change. Peaks reported by the OS stay on each process and are not added together.";

impl MemoryTrend {
    /// A trend from one current sample and no history.
    pub fn current_only(sample: MemorySample) -> Self {
        Self {
            samples: vec![sample],
            direction: TrendDirection::Unknown,
            unknown: Some(
                "Only this sample is available. A direction needs an earlier sample of the same process.".into(),
            ),
            rule: TREND_RULE.into(),
        }
    }

    /// Compare a series that is already oldest-first and deduplicated.
    pub fn from_samples(samples: Vec<MemorySample>) -> Self {
        if samples.len() < 2 {
            let sample = samples
                .first()
                .copied()
                .unwrap_or(MemorySample { at_ms: 0, bytes: 0 });
            let mut trend = Self::current_only(sample);
            if samples.is_empty() {
                trend.samples.clear();
                trend.unknown = Some("No headline sample was recorded for this process.".into());
            }
            return trend;
        }
        let oldest = samples[0].bytes;
        let newest = samples[samples.len() - 1].bytes;
        let band = std::cmp::max(oldest / 10, 1024 * 1024);
        let direction = if newest > oldest.saturating_add(band) {
            TrendDirection::Rising
        } else if oldest > newest.saturating_add(band) {
            TrendDirection::Falling
        } else {
            TrendDirection::Flat
        };
        Self {
            samples,
            direction,
            unknown: None,
            rule: TREND_RULE.into(),
        }
    }
}

/// Memory for one running agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMemoryReport {
    /// `Agent::id`.
    pub agent_id: String,
    /// Process id of the agent, paired with [`Self::started_at`] so a reused pid is not mixed.
    pub pid: u32,
    /// Process start time, Unix seconds.
    pub started_at: u64,
    /// Same number as `Agent::memory_bytes`.
    pub headline_bytes: u64,
    /// Which OS counter the headline is.
    pub headline_counter: HeadlineCounter,
    /// What the headline means on this operating system.
    pub headline_meaning: String,
    /// Physical RAM of the machine.
    pub machine_bytes: ObservedU64,
    /// `headline_bytes * 1_000_000 / machine_bytes` when the machine size is known and non-zero.
    pub share_ppm: ObservedU64,
    /// Summed platform counters for the whole tree. Unknown when any process missed that counter.
    pub resident: ObservedU64,
    /// Summed physical footprint.
    pub physical_footprint: ObservedU64,
    /// Summed private resident bytes.
    pub private_bytes: ObservedU64,
    /// Summed shared resident bytes.
    pub shared_bytes: ObservedU64,
    /// Summed proportional set size.
    pub proportional: ObservedU64,
    /// Summed compressed bytes.
    pub compressed: ObservedU64,
    /// Summed virtual size.
    pub virtual_bytes: ObservedU64,
    /// Summed swap.
    pub swap_bytes: ObservedU64,
    /// Not a sum. Peaks are not simultaneous.
    pub peak_resident: ObservedU64,
    /// Not a sum. Peaks are not simultaneous.
    pub peak_physical: ObservedU64,
    /// Summed Windows commit charge.
    pub commit_bytes: ObservedU64,
    /// One row per role that has a process.
    pub groups: Vec<MemoryGroup>,
    /// One row per process, agent first.
    pub processes: Vec<ProcessMemoryRow>,
    /// Retained headline samples. The builder starts this at the current sample.
    pub trend: MemoryTrend,
    /// Coverage notes. These do not contain command lines or file contents.
    pub limits: Vec<String>,
}

impl AgentMemoryReport {
    /// One line for text surfaces: headline, share of RAM, and how many counters are unknown.
    pub fn summary_line(&self) -> String {
        let share = match self.share_ppm.get() {
            Some(ppm) => format!("{:.1}% of RAM", ppm as f64 / 10_000.0),
            None => format!("share unknown ({})", self.share_ppm.reason()),
        };
        format!(
            "{} · {share} · resident {} · private {} · shared {}",
            crate::util::human_bytes(self.headline_bytes),
            self.resident.bytes_text(),
            self.private_bytes.bytes_text(),
            self.shared_bytes.bytes_text()
        )
    }

    /// Role totals and every process, largest scan sample first.
    ///
    /// Unknown platform counters are left off each process line. The summary line already
    /// says which tree-level counters are unknown, and a missing counter is not printed as zero.
    pub fn breakdown_lines(&self) -> Vec<String> {
        let mut lines = vec![self.summary_line()];
        let trend = match self.trend.direction {
            TrendDirection::Rising => "rising",
            TrendDirection::Falling => "falling",
            TrendDirection::Flat => "flat",
            TrendDirection::Unknown => "unknown",
        };
        let why = self
            .trend
            .unknown
            .as_deref()
            .map(|reason| format!(" ({reason})"))
            .unwrap_or_default();
        lines.push(format!("trend {trend}{why}"));
        for group in &self.groups {
            let noun = if group.processes == 1 {
                "process"
            } else {
                "processes"
            };
            lines.push(format!(
                "{}: {} {noun} · {}",
                group.role.label(),
                group.processes,
                crate::util::human_bytes(group.scan_bytes)
            ));
        }
        let mut rows: Vec<&ProcessMemoryRow> = self.processes.iter().collect();
        rows.sort_by(|a, b| b.scan_bytes.cmp(&a.scan_bytes).then(a.pid.cmp(&b.pid)));
        for row in rows {
            let mut bits = Vec::new();
            for (label, figure) in [
                ("resident", &row.reading.resident),
                ("footprint", &row.reading.physical_footprint),
                ("private", &row.reading.private_bytes),
                ("swap", &row.reading.swap_bytes),
                ("peak", &row.reading.peak_physical),
            ] {
                if let Some(value) = figure.get() {
                    bits.push(format!("{label} {}", crate::util::human_bytes(value)));
                }
            }
            if row.dominant {
                bits.push("large".into());
            }
            let extra = if bits.is_empty() {
                String::new()
            } else {
                format!(" · {}", bits.join(" · "))
            };
            lines.push(format!(
                "{} pid {} · {} · {}{extra}",
                row.name,
                row.pid,
                row.role.label(),
                crate::util::human_bytes(row.scan_bytes)
            ));
            if row.dominant {
                if let Some(reason) = &row.dominant_reason {
                    lines.push(format!("  {reason}"));
                }
            }
        }
        lines
    }
}

const DOMINANT_BYTES: u64 = 256 * 1024 * 1024;
const PEAK_REASON: &str = "Each process has its own peak. Those peaks are not simultaneous, so holdmap does not add them into a tree peak.";

/// Build the memory report for `agent` from the process table and `probe`.
pub fn agent_memory(
    agent: &Agent,
    table: &ProcessTable,
    probe: &dyn MemoryProbe,
    taken_at_ms: u64,
) -> AgentMemoryReport {
    let machine_bytes = probe.machine_bytes();
    let mut rows = Vec::new();
    let mut missing = 0u32;
    for pid in &agent.process_ids {
        let Some(process) = table.get(*pid) else {
            missing += 1;
            continue;
        };
        let (role, evidence) = classify(agent, process.pid, &process.name);
        rows.push(ProcessMemoryRow {
            pid: process.pid,
            name: process.name.clone(),
            role,
            evidence,
            scan_bytes: process.memory_bytes,
            reading: probe.reading(process.pid),
            dominant: false,
            dominant_reason: None,
        });
    }
    rows.sort_by_key(|row| (role_order(row.role), row.pid));
    let headline_bytes = rows
        .iter()
        .map(|row| row.scan_bytes)
        .fold(0, u64::saturating_add);
    for row in &mut rows {
        if row.role != MemoryRole::Agent
            && row.scan_bytes >= DOMINANT_BYTES
            && headline_bytes > 0
            && row.scan_bytes.saturating_mul(2) >= headline_bytes
        {
            row.dominant = true;
            row.dominant_reason = Some(
                "This process is not the agent itself, and its process-table sample is at least 256 MiB and at least half of the agent total.".into(),
            );
        }
    }
    let share_ppm = match machine_bytes.get() {
        Some(machine) if machine > 0 => {
            ObservedU64::observed(headline_bytes.saturating_mul(1_000_000) / machine)
        }
        Some(_) => ObservedU64::unknown("The machine reported zero bytes of physical memory."),
        None => ObservedU64::unknown(machine_bytes.reason()),
    };
    let mut limits = vec![
        "Headline bytes are the process-table sample already shown as the agent's memory. Other counters are a separate read and stay unknown when that read fails. They are never copied from the headline.".into(),
        PEAK_REASON.into(),
    ];
    if missing > 0 {
        limits.push(format!(
            "{missing} attributed process id(s) were no longer in the process table, so they are omitted rather than counted as zero."
        ));
    }
    if rows.iter().any(|row| row.dominant) {
        limits.push(
            "A dominant process is a helper, language server, MCP server or child whose sample is at least half the agent total and at least 256 MiB. It is a size comparison, not a verdict that the process is stuck.".into(),
        );
    }
    let grouped = |pick: fn(&ProcessReading) -> &ObservedU64| {
        sum_observed(
            &rows
                .iter()
                .map(|row| pick(&row.reading).clone())
                .collect::<Vec<_>>(),
            "The agent has no processes in the process table.",
        )
    };
    AgentMemoryReport {
        agent_id: agent.id.clone(),
        pid: agent.pid,
        started_at: agent.started_at,
        headline_bytes,
        headline_counter: HeadlineCounter::on_this_os(),
        headline_meaning: HeadlineCounter::on_this_os().meaning().into(),
        machine_bytes,
        share_ppm,
        resident: grouped(|reading| &reading.resident),
        physical_footprint: grouped(|reading| &reading.physical_footprint),
        private_bytes: grouped(|reading| &reading.private_bytes),
        shared_bytes: grouped(|reading| &reading.shared_bytes),
        proportional: grouped(|reading| &reading.proportional),
        compressed: grouped(|reading| &reading.compressed),
        virtual_bytes: grouped(|reading| &reading.virtual_bytes),
        swap_bytes: grouped(|reading| &reading.swap_bytes),
        peak_resident: ObservedU64::unknown(PEAK_REASON),
        peak_physical: ObservedU64::unknown(PEAK_REASON),
        commit_bytes: grouped(|reading| &reading.commit_bytes),
        groups: groups(&rows),
        trend: MemoryTrend::current_only(MemorySample {
            at_ms: taken_at_ms,
            bytes: headline_bytes,
        }),
        processes: rows,
        limits,
    }
}

/// Replace the current-only trend with samples from `log`, including this report's headline.
pub fn apply_trend(report: &mut AgentMemoryReport, samples: Vec<MemorySample>) {
    report.trend = MemoryTrend::from_samples(samples);
}

fn groups(rows: &[ProcessMemoryRow]) -> Vec<MemoryGroup> {
    let mut out = Vec::new();
    for role in [
        MemoryRole::Agent,
        MemoryRole::McpServer,
        MemoryRole::LanguageServer,
        MemoryRole::Helper,
        MemoryRole::Child,
    ] {
        let members: Vec<_> = rows.iter().filter(|row| row.role == role).collect();
        if members.is_empty() {
            continue;
        }
        let pick = |f: fn(&ProcessReading) -> &ObservedU64| {
            sum_observed(
                &members
                    .iter()
                    .map(|row| f(&row.reading).clone())
                    .collect::<Vec<_>>(),
                "empty group",
            )
        };
        out.push(MemoryGroup {
            role,
            processes: members.len() as u32,
            scan_bytes: members
                .iter()
                .map(|row| row.scan_bytes)
                .fold(0, u64::saturating_add),
            resident: pick(|reading| &reading.resident),
            physical_footprint: pick(|reading| &reading.physical_footprint),
            private_bytes: pick(|reading| &reading.private_bytes),
            shared_bytes: pick(|reading| &reading.shared_bytes),
            proportional: pick(|reading| &reading.proportional),
            compressed: pick(|reading| &reading.compressed),
            virtual_bytes: pick(|reading| &reading.virtual_bytes),
        });
    }
    out
}

fn role_order(role: MemoryRole) -> u8 {
    match role {
        MemoryRole::Agent => 0,
        MemoryRole::McpServer => 1,
        MemoryRole::LanguageServer => 2,
        MemoryRole::Helper => 3,
        MemoryRole::Child => 4,
    }
}

fn classify(agent: &Agent, pid: u32, name: &str) -> (MemoryRole, Evidence) {
    if pid == agent.pid {
        return (MemoryRole::Agent, Evidence::Observed);
    }
    if agent.tools.iter().any(|tool| mcp_process(tool, pid)) {
        return (MemoryRole::McpServer, Evidence::Inferred);
    }
    if language_server(name) {
        return (MemoryRole::LanguageServer, Evidence::Inferred);
    }
    match agent.processes.iter().find(|process| process.pid == pid) {
        Some(process) if process.role == ProcessRole::Helper => {
            (MemoryRole::Helper, Evidence::Observed)
        }
        Some(_) => (MemoryRole::Child, Evidence::Observed),
        // The pid is in the owned set but was past the display cap, so its role was not stored.
        None => (MemoryRole::Child, Evidence::Inferred),
    }
}

fn mcp_process(tool: &AgentTool, pid: u32) -> bool {
    tool.pid == pid && tool.kind == ToolKind::McpServer
}

/// Names and suffixes of language-server processes. The match is inferred, not a protocol check.
fn language_server(name: &str) -> bool {
    const EXACT: &[&str] = &[
        "rust-analyzer",
        "rust-analyzer-proc-macro-srv",
        "tsserver",
        "typescript-language-server",
        "vtsls",
        "pyright",
        "pyright-langserver",
        "basedpyright",
        "pylsp",
        "jedi-language-server",
        "gopls",
        "clangd",
        "jdtls",
        "eslint",
        "vue-language-server",
        "elixir-ls",
        "solargraph",
        "ruby-lsp",
        "zls",
        "sourcekit-lsp",
        "lua-language-server",
        "intelephense",
        "haskell-language-server",
    ];
    let name = name.to_ascii_lowercase();
    EXACT.iter().any(|exact| name == *exact)
        || name.contains("language-server")
        || name.contains("language_server")
}
