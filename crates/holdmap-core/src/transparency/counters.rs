//! Parse Linux `/proc` counter text. The collector and the tests share this so a missing
//! field stays unknown on every platform the tests run on.

use super::figure::ObservedU64;
use super::probe::ProcessReading;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ProcCounters {
    values: Vec<(&'static str, u64)>,
}

impl ProcCounters {
    fn kb(&self, key: &str) -> Option<u64> {
        self.values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, kb)| kb.saturating_mul(1024))
    }
}

/// Parse `Key:\t123 kB` lines. The first value for a key wins.
pub(crate) fn parse_proc_counters(text: &str) -> ProcCounters {
    const KEYS: &[&str] = &[
        "VmRSS",
        "VmSize",
        "VmHWM",
        "VmSwap",
        "Rss",
        "Pss",
        "Private_Clean",
        "Private_Dirty",
        "Shared_Clean",
        "Shared_Dirty",
        "Swap",
    ];
    let mut values = Vec::new();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(key) = KEYS.iter().copied().find(|name| *name == key) else {
            continue;
        };
        if values.iter().any(|(name, _)| *name == key) {
            continue;
        }
        let digits: String = rest
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(kb) = digits.parse::<u64>() {
            values.push((key, kb));
        }
    }
    ProcCounters { values }
}

fn bytes_of(fields: &ProcCounters, key: &str, missing: &str) -> ObservedU64 {
    fields
        .kb(key)
        .map(ObservedU64::observed)
        .unwrap_or_else(|| ObservedU64::unknown(missing))
}

fn paired(fields: &ProcCounters, a: &str, b: &str, missing: &str) -> ObservedU64 {
    match (fields.kb(a), fields.kb(b)) {
        (Some(left), Some(right)) => ObservedU64::observed(left.saturating_add(right)),
        _ => ObservedU64::unknown(missing),
    }
}

/// Build a Linux reading from status text and optional `smaps_rollup` text.
pub(crate) fn reading_from_proc(status_text: &str, rollup_text: Option<&str>) -> ProcessReading {
    let status = parse_proc_counters(status_text);
    let rollup = rollup_text.map(parse_proc_counters);
    let rollup_missing = "smaps_rollup was not readable. Private, shared and proportional bytes are not taken from status, because RssFile and RssShmem mix private and shared pages.";
    let (private_bytes, shared_bytes, proportional) = match &rollup {
        Some(fields) => (
            paired(
                fields,
                "Private_Clean",
                "Private_Dirty",
                "smaps_rollup did not include both Private_Clean and Private_Dirty.",
            ),
            paired(
                fields,
                "Shared_Clean",
                "Shared_Dirty",
                "smaps_rollup did not include both Shared_Clean and Shared_Dirty.",
            ),
            bytes_of(fields, "Pss", "smaps_rollup did not include Pss."),
        ),
        None => (
            ObservedU64::unknown(rollup_missing),
            ObservedU64::unknown(rollup_missing),
            ObservedU64::unknown(rollup_missing),
        ),
    };
    ProcessReading {
        resident: bytes_of(&status, "VmRSS", "VmRSS was not in /proc status."),
        physical_footprint: ObservedU64::unknown(
            "Linux has no physical-footprint counter. Resident memory is VmRSS.",
        ),
        private_bytes,
        shared_bytes,
        proportional,
        compressed: ObservedU64::unknown(
            "Linux does not publish per-process compressed memory. zswap and zram are accounted for the whole machine.",
        ),
        virtual_bytes: bytes_of(&status, "VmSize", "VmSize was not in /proc status."),
        swap_bytes: bytes_of(&status, "VmSwap", "VmSwap was not in /proc status."),
        peak_resident: bytes_of(&status, "VmHWM", "VmHWM was not in /proc status."),
        peak_physical: ObservedU64::unknown(
            "Linux records a peak resident size (VmHWM), not a peak physical footprint.",
        ),
        commit_bytes: ObservedU64::unknown(
            "Linux commit charge is not reported as Windows private commit.",
        ),
    }
}
