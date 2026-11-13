//! Linux memory counters from `/proc`.
//!
//! Parsing lives in [`super::counters`] so tests can feed status text without a live process.
//! This module only reads the files.

use super::counters::reading_from_proc;
use super::figure::ObservedU64;
use super::probe::ProcessReading;
use std::fs;

pub(super) fn read(pid: u32) -> ProcessReading {
    if pid == 0 {
        return ProcessReading::all_unknown("Process id 0 has no /proc memory counters.");
    }
    let status = match fs::read_to_string(format!("/proc/{pid}/status")) {
        Ok(text) => text,
        Err(error) => {
            return ProcessReading::all_unknown(format!(
                "Could not read /proc/{pid}/status ({})",
                io_reason(&error)
            ));
        }
    };
    let rollup = fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok();
    reading_from_proc(&status, rollup.as_deref())
}

pub(super) fn machine_bytes() -> ObservedU64 {
    let text = match fs::read_to_string("/proc/meminfo") {
        Ok(text) => text,
        Err(error) => {
            return ObservedU64::unknown(format!(
                "Could not read /proc/meminfo ({})",
                io_reason(&error)
            ));
        }
    };
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("MemTotal:") else {
            continue;
        };
        let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
        return digits
            .parse::<u64>()
            .map(|kb| ObservedU64::observed(kb.saturating_mul(1024)))
            .unwrap_or_else(|_| {
                ObservedU64::unknown("MemTotal in /proc/meminfo was not a number.")
            });
    }
    ObservedU64::unknown("MemTotal was not in /proc/meminfo.")
}

fn io_reason(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => "access denied",
        std::io::ErrorKind::NotFound => "not found",
        _ => "read failed",
    }
}
