//! Commands backed by the user store: pins, stopped-port history, restart and open-in-browser.

use crate::render;
use crate::style::{self, bold, dim, paint, S};
use anyhow::{bail, Context, Result};
use portwise_core::history::{self, HistoryEntry};
use portwise_core::store::Store;
use portwise_core::util::{human_duration, local_url, now_ms, open_url};
use portwise_core::*;
use std::time::{Duration, Instant};

#[derive(clap::Args, Debug)]
pub struct PinArgs {
    #[arg(value_parser = crate::parse_port)]
    pub port: u16,
    /// A note shown next to the port.
    #[arg(short, long)]
    pub label: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct HistoryArgs {
    /// How many entries to show.
    #[arg(short = 'n', long, default_value_t = 20)]
    pub limit: usize,
    /// Forget the history.
    #[arg(long)]
    pub clear: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct RestartArgs {
    #[arg(value_parser = crate::parse_port)]
    pub port: u16,
    /// Don't ask before stopping the current owner.
    #[arg(short, long)]
    pub yes: bool,
    /// How long to wait for the port to accept connections again.
    #[arg(short, long, default_value = "30s", value_parser = humantime::parse_duration)]
    pub timeout: Duration,
}

#[derive(clap::Args, Debug)]
pub struct OpenArgs {
    #[arg(value_parser = crate::parse_port)]
    pub port: u16,
    /// Print the URL instead of opening it.
    #[arg(long)]
    pub print: bool,
}

fn store() -> Store {
    Store::open_default()
}

pub fn pin(a: &PinArgs, on: bool) -> Result<u8> {
    let s = store();
    let (c, _) = s.update_config(|c| {
        if c.is_pinned(a.port) != on {
            c.toggle_pin(a.port, a.label.clone());
        } else if on {
            if let Some(p) = c.pins.iter_mut().find(|p| p.port == a.port) {
                p.label = a.label.clone().or(p.label.take());
            }
        }
    })?;
    println!(
        "{} {} :{} {}",
        style::ok_mark(),
        if on { "Pinned" } else { "Unpinned" },
        a.port,
        dim(format!("({} pinned)", c.pins.len()))
    );
    Ok(crate::exit::OK)
}

pub fn pins(json: bool, docker: bool) -> Result<u8> {
    let c = store().config();
    if json {
        println!("{}", serde_json::to_string_pretty(&c.pins)?);
        return Ok(crate::exit::OK);
    }
    if c.pins.is_empty() {
        println!(
            "{}",
            dim("No pinned ports. Pin one with `portwise pin 3000`.")
        );
        return Ok(crate::exit::OK);
    }
    let e = Engine::new(&ScanOptions {
        all_states: false,
        docker,
    })?;
    for p in &c.pins {
        let holder = e
            .snapshot()
            .entries
            .iter()
            .find(|x| x.port == p.port && x.state.is_listening());
        let state = match holder {
            Some(h) => paint(format!("● {}", h.label), S::Green),
            None => dim("○ free"),
        };
        println!(
            "{} {:>5}  {}{}",
            paint("★", S::Yellow),
            paint(p.port.to_string(), S::BoldCyan),
            state,
            p.label
                .as_ref()
                .map(|l| dim(format!("  — {l}")))
                .unwrap_or_default()
        );
    }
    Ok(crate::exit::OK)
}

pub fn history_cmd(a: &HistoryArgs) -> Result<u8> {
    let s = store();
    if a.clear {
        s.clear_history()?;
        println!("{} History cleared.", style::ok_mark());
        return Ok(crate::exit::OK);
    }
    let h = s.history(a.limit);
    if a.json {
        println!("{}", serde_json::to_string_pretty(&h)?);
        return Ok(crate::exit::OK);
    }
    if h.is_empty() {
        println!("{}", dim("Nothing stopped yet."));
        return Ok(crate::exit::OK);
    }
    let now = now_ms();
    for e in &h {
        println!(
            "{:>5}  {}  {}  {}",
            paint(e.port.to_string(), S::BoldCyan),
            bold(&e.label),
            dim(format!(
                "{} ago",
                human_duration(now.saturating_sub(e.at_ms) / 1000)
            )),
            dim(if e.restartable() {
                format!("`{}`", e.command_line())
            } else {
                "(no command recorded)".into()
            })
        );
    }
    println!(
        "\n{}",
        dim("Start one again with `portwise restart <port>`.")
    );
    Ok(crate::exit::OK)
}

/// Record a successful stop in the history (best effort: never fails the stop).
pub fn record(e: &Engine, plan: &ActionPlan, report: &StopReport) {
    let entries = history::entries_from_plan(&e.scan, plan, report);
    let _ = store().record(&entries);
}

fn start(entry: &HistoryEntry, timeout: Duration) -> Result<u8> {
    let s = store();
    let (pid, log) = history::restart(entry, &s.logs_dir())
        .with_context(|| format!("failed to start `{}`", entry.command_line()))?;
    println!(
        "{} started `{}` {}",
        style::arrow(),
        bold(entry.command_line()),
        dim(format!(
            "(PID {pid}{}; log {})",
            entry
                .cwd
                .as_ref()
                .map(|c| format!(", in {}", tilde(c)))
                .unwrap_or_default(),
            tilde(&log)
        ))
    );
    let started = Instant::now();
    while started.elapsed() < timeout {
        if tcp_accepting(entry.port) {
            println!(
                "{} {} {}",
                style::ok_mark(),
                bold(format!("Port {} is back", entry.port)),
                dim(format!("after {} ms", started.elapsed().as_millis()))
            );
            return Ok(crate::exit::OK);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    eprintln!(
        "{} port {} isn't accepting connections after {}; see {}",
        style::warn_mark(),
        entry.port,
        humantime::format_duration(timeout),
        tilde(&log)
    );
    Ok(crate::exit::BUSY)
}

pub fn restart(a: &RestartArgs, docker: bool) -> Result<u8> {
    let e = Engine::new(&ScanOptions {
        all_states: true,
        docker,
    })?;
    let opts = StopOptions::default();
    if port_busy(a.port, Protocol::Tcp) {
        let plan = e.plan(&Target::Port(a.port), &opts);
        print!("{}", render::plan_text(&plan));
        if plan.is_blocked() {
            bail!("can't restart: {}", plan.summary);
        }
        let entry = history::entries_from_plan(
            &e.scan,
            &plan,
            &StopReport {
                success: true,
                ..Default::default()
            },
        )
        .into_iter()
        .find(|h| h.port == a.port && h.restartable())
        .context("this owner can't be restarted by portwise (no command to re-run, e.g. a container or service — use its own restart)")?;
        if !a.yes && !crate::commands::confirm("Restart it?")? {
            println!("{}", dim("Cancelled."));
            return Ok(crate::exit::BUSY);
        }
        let report = execute(&plan, &mut |l| println!("  {} {}", dim("·"), dim(l)));
        println!("{}", render::report_line(&report));
        if !report.success {
            return Ok(crate::exit::BUSY);
        }
        let _ = store().record(std::slice::from_ref(&entry));
        return start(&entry, a.timeout);
    }
    let Some(entry) = store().last_for_port(a.port).filter(|h| h.restartable()) else {
        bail!(
            "port {} is free and portwise has no record of what ran there (see `portwise history`)",
            a.port
        );
    };
    start(&entry, a.timeout)
}

pub fn open(a: &OpenArgs) -> Result<u8> {
    let url = local_url(a.port);
    if a.print {
        println!("{url}");
        return Ok(crate::exit::OK);
    }
    if !tcp_accepting(a.port) {
        eprintln!(
            "{} nothing is accepting connections on port {}",
            style::warn_mark(),
            a.port
        );
        return Ok(crate::exit::BUSY);
    }
    open_url(&url).with_context(|| format!("failed to open {url}"))?;
    println!("{} opened {url}", style::ok_mark());
    Ok(crate::exit::OK)
}
