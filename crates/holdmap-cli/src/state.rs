//! Commands backed by the user store: pins, stopped-port history, restart and open-in-browser.

use crate::render;
use crate::style::{self, bold, dim, paint, S};
use anyhow::{bail, Context, Result};
use holdmap_core::history::{self, HistoryEntry};
use holdmap_core::store::Store;
use holdmap_core::util::{human_duration, local_url, now_ms, open_url};
use holdmap_core::*;
use std::time::{Duration, Instant};

#[derive(clap::Args, Debug)]
pub struct PinArgs {
    /// Port number (e.g. 3000 or :3000).
    #[arg(value_parser = crate::parse_port)]
    pub port: u16,
    /// A note shown next to the port.
    #[arg(short, long)]
    pub label: Option<String>,
}

/// Arguments of `holdmap unpin`.
#[derive(clap::Args, Debug)]
pub struct UnpinArgs {
    /// Port number (e.g. 3000 or :3000).
    #[arg(value_parser = crate::parse_port)]
    pub port: u16,
}

#[derive(clap::Args, Debug)]
pub struct HistoryArgs {
    /// How many entries to show.
    #[arg(short = 'n', long, default_value_t = 20)]
    pub limit: usize,
    /// Forget the history.
    #[arg(long)]
    pub clear: bool,
    /// Machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct RestartArgs {
    /// TCP port number (e.g. 3000 or :3000).
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
    /// Port number (e.g. 3000 or :3000).
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
    let (c, changed) = s.update_config(|c| {
        if c.is_pinned(a.port) != on {
            c.toggle_pin(a.port, a.label.clone());
            true
        } else {
            if on {
                if let Some(p) = c.pins.iter_mut().find(|p| p.port == a.port) {
                    p.label = a.label.clone().or(p.label.take());
                }
            }
            false
        }
    })?;
    let count = dim(format!("({} pinned)", c.pins.len()));
    match (on, changed) {
        (false, false) => {
            println!("{} :{} isn't pinned {count}", style::err_mark(), a.port);
            return Ok(crate::exit::BUSY);
        }
        (true, false) => println!("{} :{} is already pinned {count}", style::ok_mark(), a.port),
        (true, true) => println!("{} Pinned :{} {count}", style::ok_mark(), a.port),
        (false, true) => println!("{} Unpinned :{} {count}", style::ok_mark(), a.port),
    }
    Ok(crate::exit::OK)
}

pub fn pins(json: bool, docker: bool) -> Result<u8> {
    let c = store().try_config().context("Could not read saved pins")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&c.pins)?);
        return Ok(crate::exit::OK);
    }
    if c.pins.is_empty() {
        println!(
            "{}",
            dim("No pinned ports. Pin one with `holdmap pin 3000`.")
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
    let h = s
        .try_history(a.limit)
        .context("Could not read service-stop history")?;
    if a.json {
        let shown: Vec<HistoryEntry> = h.iter().map(HistoryEntry::redacted).collect();
        println!("{}", serde_json::to_string_pretty(&shown)?);
        return Ok(crate::exit::OK);
    }
    if h.is_empty() {
        println!("{}", dim("Nothing stopped yet."));
        return Ok(crate::exit::OK);
    }
    let now = now_ms();
    let cmd_width = style::term_width().saturating_sub(45).max(30);
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
                format!("`{}`", style::one_line(&e.command_line(), cmd_width))
            } else {
                "(restart unavailable)".into()
            })
        );
    }
    println!(
        "\n{}",
        dim("Restart supported recordings with `holdmap restart <port>`.")
    );
    Ok(crate::exit::OK)
}

/// Record executed effects without hiding a storage failure from interactive surfaces.
pub(crate) fn record_checked(e: &Engine, plan: &ActionPlan, report: &StopReport) -> Result<()> {
    let entries = history::entries_from_plan(&e.scan, plan, report);
    store()
        .record(&entries)
        .context("Could not save service-stop history")
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
    let protection = crate::project_policy::ProjectProtection::nearest()?;
    protection.check_port(a.port)?;
    let e = Engine::new(&ScanOptions {
        all_states: true,
        docker,
    })?;
    if port_busy(a.port, Protocol::Tcp) {
        let plan = protection.apply(&e, tcp_restart_plan(&e, a.port));
        print!("{}", render::plan_text(&plan));
        if plan.is_blocked() {
            return Ok(crate::commands::block_code(&plan).max(crate::exit::BUSY));
        }
        let entry = planned_tcp_restart_entry(&e, &plan, a.port)?;
        if !a.yes && !crate::commands::confirm("Restart it?")? {
            println!("{}", dim("Cancelled."));
            return Ok(crate::exit::BUSY);
        }
        crate::project_policy::checked_engine(docker, &plan, None)?;
        let report = execute(&plan, &mut |l| println!("  {} {}", dim("·"), dim(l)));
        println!("{}", render::report_line(&report));
        if !report.success {
            return Ok(crate::exit::BUSY);
        }
        store().record(std::slice::from_ref(&entry)).context(
            "The service stopped, but its restart history could not be saved. Restart was not attempted; start it using its normal launcher",
        )?;
        crate::project_policy::ProjectProtection::nearest()?.check_port(a.port)?;
        return start(&entry, a.timeout);
    }
    let Some(entry) = store()
        .last_matching(a.port, Some(Protocol::Tcp), None)?
        .filter(|h| h.restartable())
    else {
        bail!(
            "port {} is free and holdmap has no record it can safely restart (see `holdmap history`)",
            a.port
        );
    };
    crate::project_policy::ProjectProtection::nearest()?.check_port(a.port)?;
    start(&entry, a.timeout)
}

fn tcp_restart_plan(e: &Engine, port: u16) -> ActionPlan {
    e.plan(
        &Target::Port(port),
        &StopOptions {
            protocol: Some(Protocol::Tcp),
            ..Default::default()
        },
    )
}

/// A port restart launches one recorded process. Refuse multiple independent roots before
/// stopping anything, and never choose a UDP recording for this TCP readiness operation.
fn planned_tcp_restart_entry(e: &Engine, plan: &ActionPlan, port: u16) -> Result<HistoryEntry> {
    let roots: std::collections::BTreeSet<_> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::SignalProcesses { processes, .. } => processes.first(),
            _ => None,
        })
        .map(|process| (process.pid, process.start_token))
        .collect();
    if roots.len() > 1 {
        bail!(
            "TCP port {port} has multiple process owners; restart them using their normal launchers"
        );
    }
    history::entries_from_plan(
        &e.scan,
        plan,
        &StopReport {
            success: true,
            ..Default::default()
        },
    )
    .into_iter()
    .find(|h| h.port == port && h.protocol == Protocol::Tcp && h.restartable())
    .context(
        "this owner has no safely restartable command recorded; start it using its normal launcher",
    )
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

#[cfg(test)]
mod restart_tests {
    use super::*;
    use holdmap_core::engine::strategies::{
        OtherUserStrategy, ProcessTreeStrategy, ProtectedStrategy, StrategyRegistry,
    };
    use holdmap_core::process::ProcessTable;
    use holdmap_core::scan::Scanner;

    fn process(pid: u32) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid: None,
            name: "owned-listener".into(),
            exe: None,
            cmdline: vec![format!("owned-listener-{pid}")],
            cwd: None,
            uid: Some(1000),
            user: Some("dev".into()),
            start_time: 1_700_000_000,
            start_token: u64::from(pid) + 1000,
            memory_bytes: 0,
            cpu_percent: 0.0,
        }
    }

    fn socket(pid: u32, protocol: Protocol) -> RawSocket {
        RawSocket {
            protocol,
            family: Family::V4,
            local_addr: "127.0.0.1".parse().unwrap(),
            local_port: 41000,
            remote_addr: None,
            remote_port: None,
            state: if protocol == Protocol::Tcp {
                SocketState::Listen
            } else {
                SocketState::Bound
            },
            uid: Some(1000),
            inode: None,
            pids: vec![pid],
        }
    }

    fn engine(raw: Vec<RawSocket>) -> Engine {
        let processes = [100, 200, 999]
            .into_iter()
            .map(|pid| (pid, process(pid)))
            .collect();
        let table = ProcessTable::from_processes(processes, 999);
        Engine::from_scan(
            Scanner::fixed(raw, table, vec![])
                .scan(&ScanOptions {
                    all_states: true,
                    docker: false,
                })
                .unwrap(),
        )
        // These are synthetic process identities. Retain process ownership/protection
        // and tree planning without querying the host's systemd/PM2/Homebrew services.
        .with_strategies(
            StrategyRegistry::empty()
                .with(ProtectedStrategy)
                .with(OtherUserStrategy)
                .with(ProcessTreeStrategy),
        )
    }

    #[test]
    fn tcp_restart_preserves_a_distinct_udp_owner_in_either_snapshot_order() {
        for raw in [
            vec![socket(200, Protocol::Udp), socket(100, Protocol::Tcp)],
            vec![socket(100, Protocol::Tcp), socket(200, Protocol::Udp)],
        ] {
            let engine = engine(raw);
            let plan = tcp_restart_plan(&engine, 41000);
            assert!(!plan.is_blocked(), "{plan:?}");
            let signalled: Vec<_> = plan
                .steps
                .iter()
                .flat_map(|step| match step {
                    Step::SignalProcesses { processes, .. } => processes.iter(),
                    _ => [].iter(),
                })
                .map(|process| process.pid)
                .collect();
            assert_eq!(signalled, [100]);
            assert!(plan.steps.iter().all(|step| !matches!(
                step,
                Step::VerifyFree {
                    protocol: Protocol::Udp,
                    ..
                }
            )));
            let entry = planned_tcp_restart_entry(&engine, &plan, 41000).unwrap();
            assert_eq!((entry.protocol, entry.pid), (Protocol::Tcp, 100));
        }
    }

    #[test]
    fn multiple_independent_tcp_roots_cannot_be_restarted_as_one_recording() {
        let engine = engine(vec![socket(100, Protocol::Tcp), socket(200, Protocol::Tcp)]);
        let plan = tcp_restart_plan(&engine, 41000);
        assert!(!plan.is_blocked(), "{plan:?}");
        let roots: std::collections::BTreeSet<_> = plan
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::SignalProcesses { processes, .. } => {
                    processes.first().map(|process| process.pid)
                }
                _ => None,
            })
            .collect();
        assert_eq!(roots, [100, 200].into_iter().collect());
        let error = planned_tcp_restart_entry(&engine, &plan, 41000).unwrap_err();
        assert!(error.to_string().contains("multiple process owners"));
    }
}
