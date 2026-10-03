//! Implementations of the non-interactive commands.

use crate::exit;
use crate::render;
use crate::style::{self, bold, dim, paint, S};
use crate::{FreePortArgs, ListArgs, PortArgs, RunArgs, SortKey, StopArgs, WaitArgs};
use anyhow::{bail, Context, Result};
use portwise_core::http;
use portwise_core::*;
use std::io::{IsTerminal, Write};
use std::time::Instant;

fn engine(docker: bool, all_states: bool) -> Result<Engine> {
    Engine::new(&ScanOptions { all_states, docker }).context("failed to scan sockets")
}

pub(crate) fn print_json<T: serde::Serialize>(v: &T) -> Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, v)?;
    writeln!(out)?;
    Ok(())
}

pub fn sort_entries(v: &mut [&PortEntry], key: SortKey) {
    match key {
        SortKey::Port => v.sort_by_key(|e| (e.port, e.protocol)),
        SortKey::Pid => v.sort_by_key(|e| (e.pid.unwrap_or(u32::MAX), e.port)),
        SortKey::Name => v.sort_by_key(|e| {
            (
                e.process
                    .as_ref()
                    .map(|p| p.name.to_ascii_lowercase())
                    .unwrap_or_else(|| "~".into()),
                e.port,
            )
        }),
        SortKey::Proto => v.sort_by_key(|e| (e.protocol, e.port)),
        SortKey::Memory => v.sort_by_key(|e| {
            std::cmp::Reverse(e.process.as_ref().map(|p| p.memory_bytes).unwrap_or(0))
        }),
        SortKey::Uptime => {
            v.sort_by_key(|e| e.process.as_ref().map(|p| p.start_time).unwrap_or(u64::MAX))
        }
    }
}

/// HTTP summaries for the TCP listeners among `entries`, probed in parallel.
pub fn probe_http(entries: &[&PortEntry]) -> std::collections::BTreeMap<u16, http::HttpInfo> {
    let mut ports: Vec<u16> = entries
        .iter()
        .filter(|e| e.protocol == Protocol::Tcp && e.state.is_listening())
        .map(|e| e.port)
        .collect();
    ports.sort_unstable();
    ports.dedup();
    let mut out = std::collections::BTreeMap::new();
    // Bounded fan-out: a few dozen ports at a time, each with a short timeout.
    for chunk in ports.chunks(32) {
        std::thread::scope(|sc| {
            let hs: Vec<_> = chunk
                .iter()
                .map(|p| {
                    sc.spawn(move || http::probe(*p, "/", std::time::Duration::from_millis(800)))
                })
                .collect();
            for h in hs {
                if let Ok(Some(info)) = h.join() {
                    out.insert(info.port, info);
                }
            }
        });
    }
    out
}

pub fn list(a: &ListArgs, docker: bool) -> Result<u8> {
    let e = engine(docker, a.all)?;
    let filter = Filter {
        query: a.query.join(" "),
        protocol: if a.tcp {
            Some(Protocol::Tcp)
        } else if a.udp {
            Some(Protocol::Udp)
        } else {
            None
        },
        listening_only: !a.all,
        dev_only: a.dev,
        mine_only: a.mine,
        exposed_only: a.exposed,
        range: a.range,
    };
    let mut shown = filter.apply(&e.snapshot().entries);
    sort_entries(&mut shown, a.sort);
    if a.json {
        let mut snap = e.snapshot().clone();
        snap.entries = shown.iter().map(|x| (*x).clone()).collect();
        if a.http {
            let http = probe_http(&shown);
            let mut v = serde_json::to_value(&snap)?;
            v["http"] = serde_json::to_value(http.values().collect::<Vec<_>>())?;
            print_json(&v)?;
        } else {
            print_json(&snap)?;
        }
        return Ok(exit::OK);
    }
    let mut out = std::io::stdout().lock();
    if shown.is_empty() {
        writeln!(out, "{}", dim("No ports match."))?;
        return Ok(exit::BUSY);
    }
    let pins: Vec<u16> = store::Store::open_default()
        .config()
        .pins
        .iter()
        .map(|p| p.port)
        .collect();
    let http = if a.http {
        probe_http(&shown)
    } else {
        Default::default()
    };
    write!(
        out,
        "{}",
        render::list_table_with(&shown, a.wide, a.all, &pins, a.http.then_some(&http))
    )?;
    writeln!(out, "\n{}", render::summary_line(e.snapshot(), &shown))?;
    Ok(exit::OK)
}

fn port_opts(udp: bool) -> StopOptions {
    StopOptions {
        protocol: udp.then_some(Protocol::Udp),
        ..Default::default()
    }
}

pub fn explain(a: &PortArgs, docker: bool) -> Result<u8> {
    let e = engine(docker, true)?;
    let ex = e.explain(a.port, &port_opts(a.udp));
    if a.json {
        print_json(&ex)?;
    } else {
        print!("{}", render::explanation(&ex, true));
    }
    Ok(match ex.status {
        PortStatus::Free => exit::OK,
        _ => exit::BUSY,
    })
}

pub fn inspect(a: &PortArgs, docker: bool) -> Result<u8> {
    let e = engine(docker, true)?;
    let ex = e.explain(a.port, &port_opts(a.udp));
    if a.json {
        print_json(&ex)?;
        return Ok(if ex.status == PortStatus::Free {
            exit::OK
        } else {
            exit::BUSY
        });
    }
    let http_info = ex
        .entries
        .iter()
        .any(|x| x.protocol == Protocol::Tcp && x.state.is_listening())
        .then(|| http::probe(a.port, "/", std::time::Duration::from_millis(1000)))
        .flatten();
    let mut out = std::io::stdout().lock();
    for (i, entry) in ex.entries.iter().enumerate() {
        writeln!(
            out,
            "{}",
            bold(format!("Socket {} of {}", i + 1, ex.entries.len()))
        )?;
        write!(out, "{}", render::inspect_entry(entry))?;
        if let Some(p) = &entry.process {
            let t = e.table();
            let chain: Vec<String> = std::iter::once(p.pid)
                .chain(t.ancestors(p.pid))
                .filter_map(|pid| t.get(pid))
                .map(|p| format!("{} ({})", p.name, p.pid))
                .collect();
            writeln!(
                out,
                "  {}  {}",
                dim(format!("{:>11}", "Ancestry")),
                chain.join(&format!(" {} ", dim("←")))
            )?;
            let kids: Vec<String> = t
                .descendants(p.pid)
                .iter()
                .filter_map(|c| t.get(*c))
                .map(|c| format!("{} ({})", c.name, c.pid))
                .collect();
            if !kids.is_empty() {
                writeln!(
                    out,
                    "  {}  {}",
                    dim(format!("{:>11}", "Children")),
                    kids.join(", ")
                )?;
            }
        }
        if let Some(h) = http_info
            .as_ref()
            .filter(|_| entry.protocol == Protocol::Tcp)
        {
            writeln!(
                out,
                "  {}  {} {}",
                dim(format!("{:>11}", "HTTP")),
                h.summary(),
                dim(format!("({} ms)", h.elapsed_ms))
            )?;
        }
        writeln!(out)?;
    }
    write!(out, "{}", render::explanation(&ex, true))?;
    let others: Vec<&PortEntry> = e
        .snapshot()
        .entries
        .iter()
        .filter(|x| x.port == a.port && !x.state.is_listening())
        .collect();
    if !others.is_empty() {
        writeln!(
            out,
            "\n{}",
            bold(format!(
                "{} on this port",
                portwise_core::util::count(others.len(), "other socket", "other sockets")
            ))
        )?;
        write!(out, "{}", render::list_table(&others, false, true))?;
    }
    Ok(if ex.status == PortStatus::Free {
        exit::OK
    } else {
        exit::BUSY
    })
}

pub(crate) fn confirm(question: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        bail!("refusing to act without confirmation in a non-interactive session; pass --yes (or --dry-run to preview)");
    }
    eprint!("{} {} ", paint("?", S::BoldCyan), bold(question));
    eprint!("{} ", dim("[y/N]"));
    std::io::stderr().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub(crate) fn block_code(plan: &ActionPlan) -> u8 {
    match plan.blocked.as_ref().map(|b| b.kind) {
        Some(BlockKind::NeedsElevation) => exit::ELEVATION,
        Some(BlockKind::NothingToStop) => exit::BUSY,
        Some(_) => exit::BLOCKED,
        None => exit::OK,
    }
}

#[derive(serde::Serialize)]
struct StopResult {
    plan: ActionPlan,
    report: Option<StopReport>,
}

pub fn stop(a: &StopArgs, docker: bool) -> Result<u8> {
    // A number is always meant as a port: reject 0 and >65535 up front (like every other
    // command) instead of planning for port 0 or hunting for a process named "99999".
    for t in &a.targets {
        let n = t.trim();
        let n = n.strip_prefix(':').unwrap_or(n);
        if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
            crate::parse_port(n).map_err(anyhow::Error::msg)?;
        }
    }
    let mut targets: Vec<Target> = a.targets.iter().map(|t| Target::parse(t)).collect();
    targets.extend(a.pids.iter().map(|p| Target::Pid(*p)));
    targets.extend(a.names.iter().map(|n| Target::Name(n.clone())));
    targets.extend(a.clusters.iter().map(|c| Target::Cluster(c.clone())));
    let e = engine(docker, true)?;
    if a.all_dev {
        targets.push(Target::AllDev);
    }
    if targets.is_empty() {
        bail!(
            "nothing to stop: give a port (e.g. `portwise stop 3000`), --pid, --name, --cluster or --all-dev"
        );
    }
    if a.json && !a.yes && !a.dry_run {
        bail!("--json needs --yes (or --dry-run): JSON mode never prompts");
    }
    let opts = StopOptions {
        force: a.force,
        timeout_ms: a.timeout.as_millis() as u64,
        allow_protected: a.allow_protected,
        tree: !a.no_tree,
        protocol: a.udp.then_some(Protocol::Udp),
        ..Default::default()
    };
    let yes = a.yes;
    let protect = crate::stack::protected_ports();
    let mut code = exit::OK;
    let mut results = Vec::new();
    for t in &targets {
        let plan = e.plan(t, &opts);
        if let Target::Port(p) = t {
            if protect.contains(p) && !a.allow_protected {
                if !a.json {
                    println!(
                        "{} :{p} is listed in `protect` in {}; pass --allow-protected to stop it anyway",
                        style::err_mark(),
                        portwise_core::stack::FILE_NAME
                    );
                }
                code = code.max(exit::BLOCKED);
                results.push(StopResult { plan, report: None });
                continue;
            }
        }
        if !a.json {
            if let Target::Port(p) = t {
                let ex = e.explain(*p, &opts);
                if ex.status != PortStatus::Free {
                    println!("{} {}", render::status_dot(ex.status), bold(&ex.headline));
                }
            }
            print!("{}", render::plan_text(&plan));
        }
        if plan.is_blocked() {
            code = code.max(block_code(&plan));
            results.push(StopResult { plan, report: None });
            continue;
        }
        if a.dry_run {
            if !a.json {
                println!("{}", dim("Dry run: nothing was changed."));
            }
            results.push(StopResult { plan, report: None });
            continue;
        }
        if !yes {
            let q = if plan.risk == Risk::High {
                "This is high risk. Proceed?"
            } else {
                "Proceed?"
            };
            if !confirm(q)? {
                println!("{}", dim("Cancelled."));
                code = code.max(exit::BUSY);
                results.push(StopResult { plan, report: None });
                continue;
            }
        }
        let json = a.json;
        let report = execute(&plan, &mut |line| {
            if !json {
                println!("  {} {}", dim("·"), dim(line));
            }
        });
        if !a.json {
            println!("{}", render::report_line(&report));
        }
        crate::state::record(&e, &plan, &report);
        if !report.success {
            code = code.max(exit::BUSY);
        }
        results.push(StopResult {
            plan,
            report: Some(report),
        });
    }
    if a.json {
        print_json(&results)?;
    }
    Ok(code)
}

fn listening_ports(e: &Engine) -> std::collections::HashSet<u16> {
    e.snapshot()
        .entries
        .iter()
        .filter(|x| x.state.is_listening())
        .map(|x| x.port)
        .collect()
}

pub fn find_free_ports(e: &Engine, start: u16, end: u16, count: usize) -> Vec<u16> {
    let used = listening_ports(e);
    (start..=end)
        .filter(|p| !used.contains(p) && probe_tcp(*p) == ProbeResult::Free)
        .take(count)
        .collect()
}

pub fn free_port(a: &FreePortArgs, docker: bool) -> Result<u8> {
    let count = a.count.max(1);
    let ports: Vec<u16> = match (a.near, a.range) {
        (Some(n), _) => find_free_ports(&engine(docker, false)?, n, u16::MAX, count),
        (None, Some((lo, hi))) => find_free_ports(&engine(docker, false)?, lo, hi, count),
        (None, None) => {
            let mut v = Vec::new();
            let mut guards = Vec::new();
            while v.len() < count {
                // Hold each listener until we're done so the OS hands out distinct ports.
                let l = std::net::TcpListener::bind(("127.0.0.1", 0))?;
                v.push(l.local_addr()?.port());
                guards.push(l);
            }
            v
        }
    };
    if ports.is_empty() {
        if a.json {
            print_json(&serde_json::json!({"ports": []}))?;
        } else {
            eprintln!("{} no free port found", style::err_mark());
        }
        return Ok(exit::BUSY);
    }
    if a.json {
        print_json(&serde_json::json!({"ports": ports, "port": ports[0]}))?;
    } else {
        for p in &ports {
            println!("{p}");
        }
    }
    Ok(exit::OK)
}

pub fn wait(a: &WaitArgs) -> Result<u8> {
    let started = Instant::now();
    let check = || {
        if a.free {
            !port_busy(a.port, Protocol::Tcp)
        } else {
            tcp_accepting(a.port)
        }
    };
    let interactive = !a.quiet && !a.json && std::io::stderr().is_terminal();
    let spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let mut i = 0;
    loop {
        if check() {
            let ms = started.elapsed().as_millis() as u64;
            if interactive {
                eprint!("\r\x1b[2K");
            }
            if a.json {
                print_json(&serde_json::json!({"port": a.port, "ready": true, "elapsed_ms": ms}))?;
            } else if !a.quiet {
                let what = if a.free {
                    "is free"
                } else {
                    "is accepting connections"
                };
                println!(
                    "{} {} {}",
                    style::ok_mark(),
                    bold(format!("Port {} {what}", a.port)),
                    dim(format!("after {ms} ms"))
                );
            }
            return Ok(exit::OK);
        }
        if started.elapsed() >= a.timeout {
            if interactive {
                eprint!("\r\x1b[2K");
            }
            if a.json {
                print_json(
                    &serde_json::json!({"port": a.port, "ready": false, "elapsed_ms": started.elapsed().as_millis() as u64}),
                )?;
            } else if !a.quiet {
                eprintln!(
                    "{} timed out after {} waiting for port {}",
                    style::err_mark(),
                    humantime::format_duration(a.timeout),
                    a.port
                );
            }
            return Ok(exit::BUSY);
        }
        if interactive {
            let what = if a.free {
                "to be free"
            } else {
                "to accept connections"
            };
            eprint!(
                "\r{} waiting for port {} {what}… {}",
                paint(spinner[i % spinner.len()].to_string(), S::Cyan),
                a.port,
                dim(format!("{:.1}s", started.elapsed().as_secs_f32()))
            );
            i += 1;
        }
        std::thread::sleep(a.interval);
    }
}

pub fn run(a: &RunArgs, docker: bool) -> Result<u8> {
    let mut port = a.port;
    if port_busy(port, Protocol::Tcp) {
        let e = engine(docker, true)?;
        let opts = StopOptions {
            force: a.force,
            timeout_ms: a.timeout.as_millis() as u64,
            ..Default::default()
        };
        let ex = e.explain(port, &opts);
        eprintln!("{} {}", render::status_dot(ex.status), bold(&ex.headline));
        let plan = e.plan(&Target::Port(port), &opts);
        let mut freed = false;
        if !plan.is_blocked() {
            eprint!("{}", render::plan_text(&plan));
            let go = a.yes
                || (plan.risk != Risk::High
                    && std::io::stdin().is_terminal()
                    && confirm("Stop it and continue?")?);
            if go {
                let report = execute(&plan, &mut |l| eprintln!("  {} {}", dim("·"), dim(l)));
                eprintln!("{}", render::report_line(&report));
                freed = report.success;
            } else if plan.risk == Risk::High {
                eprintln!(
                    "{} high-risk plan: not stopping it without --yes",
                    style::arrow()
                );
            } else if !std::io::stdin().is_terminal() {
                eprintln!(
                    "{} not stopping it without confirmation in a non-interactive session (pass --yes)",
                    style::arrow()
                );
            }
        } else {
            eprint!("{}", render::plan_text(&plan));
        }
        if !freed {
            if a.fallback {
                let next = find_free_ports(&e, port.saturating_add(1), u16::MAX, 1);
                let Some(n) = next.first().copied() else {
                    bail!("no free port found after {port}")
                };
                eprintln!("{} using port {n} instead of {port}", style::arrow());
                port = n;
            } else {
                eprintln!("{} port {port} is still busy; not starting the command (use --fallback to pick another port)", style::err_mark());
                return Ok(block_code(&plan).max(exit::BUSY));
            }
        }
    }
    eprintln!(
        "{} {} {}",
        style::arrow(),
        bold(a.command.join(" ")),
        dim(format!("({}={port})", a.env))
    );
    let mut cmd = std::process::Command::new(&a.command[0]);
    cmd.args(&a.command[1..]).env(&a.env, port.to_string());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec(); // only returns on failure
        Err(anyhow::Error::new(err).context(format!("failed to run `{}`", a.command[0])))
    }
    #[cfg(not(unix))]
    {
        let status = cmd
            .status()
            .with_context(|| format!("failed to run `{}`", a.command[0]))?;
        Ok(status
            .code()
            .map(|c| c.clamp(0, 255) as u8)
            .unwrap_or(exit::ERROR))
    }
}
