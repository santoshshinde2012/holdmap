//! `portwise watch`: stream port events (opened / closed / conflict) as they happen.

use crate::style::{self, paint, S};
use anyhow::Result;
use portwise_core::events::{PortEvent, Watcher};
use portwise_core::store::Store;
use portwise_core::{ScanOptions, Scanner};
use std::io::Write;
use std::time::Duration;

#[derive(clap::Args, Debug)]
pub struct WatchArgs {
    /// Report every listener, not only dev servers and pinned ports.
    #[arg(short, long)]
    pub all: bool,
    /// Polling interval.
    #[arg(short, long, default_value = "1s", value_parser = humantime::parse_duration)]
    pub interval: Duration,
    /// Stop after this many polls (default: run until interrupted).
    #[arg(long, hide = true)]
    pub polls: Option<usize>,
    /// One JSON object per event (NDJSON).
    #[arg(long)]
    pub json: bool,
}

fn line(ev: &PortEvent) -> String {
    let s = ev.summary();
    match ev {
        PortEvent::Opened { .. } => paint(s, S::Green),
        PortEvent::Closed { .. } => style::dim(s),
        PortEvent::Conflict { .. } => paint(s, S::BoldYellow),
    }
}

pub fn run(a: &WatchArgs, docker: bool) -> Result<u8> {
    let scanner = Scanner::system();
    let opts = ScanOptions {
        all_states: false,
        docker,
    };
    let pins: Vec<u16> = Store::open_default()
        .config()
        .pins
        .iter()
        .map(|p| p.port)
        .collect();
    let mut w = Watcher::new();
    let mut polls = 0usize;
    if !a.json {
        eprintln!(
            "{} watching {} (Ctrl-C to stop)…",
            style::arrow(),
            if a.all {
                "all listeners"
            } else {
                "dev servers and pinned ports"
            }
        );
    }
    loop {
        let snap = scanner.scan(&opts)?.snapshot;
        for ev in w.observe(&snap) {
            if !ev.notable(!a.all, &pins) {
                continue;
            }
            let mut out = std::io::stdout().lock();
            if a.json {
                serde_json::to_writer(&mut out, &ev)?;
                writeln!(out)?;
            } else {
                let t = time_now();
                writeln!(out, "{} {}", style::dim(t), line(&ev))?;
            }
            out.flush()?;
        }
        polls += 1;
        if a.polls.is_some_and(|n| polls >= n) {
            return Ok(crate::exit::OK);
        }
        std::thread::sleep(a.interval);
    }
}

fn time_now() -> String {
    portwise_core::util::local_hms(portwise_core::util::now_secs())
}
