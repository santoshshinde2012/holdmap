//! Opt-in diagnostics via `HOLDMAP_TRACE`.

#[allow(unused_imports)]
use super::*;
#[allow(unused_imports)]
use std::io::Read;
#[allow(unused_imports)]
use std::process::{Command, Stdio};
#[allow(unused_imports)]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Whether `HOLDMAP_TRACE` asks for diagnostics on `topic` (`scan`, …): a comma-separated
/// list of topics, or `1` / `all` for everything.
pub fn tracing(topic: &str) -> bool {
    let var = std::env::var("HOLDMAP_TRACE")
        .or_else(|_| std::env::var("PORTWISE_TRACE"))
        .ok();
    trace_wants(var.as_deref(), topic)
}

pub(crate) fn trace_wants(var: Option<&str>, topic: &str) -> bool {
    var.is_some_and(|v| {
        v.split(',')
            .map(str::trim)
            .any(|t| t == topic || t == "1" || t == "all")
    })
}
