//! `portwise up`, `down`, `status` and `init` (project file): manage a project's services from
//! its `.portwise.toml`. Parsing, ordering and ownership live in `portwise_core::stack`; this
//! module starts processes, waits for them and renders the results. Stopping always goes through
//! the engine's plans, so the usual safety rules apply.

use crate::commands::{confirm, print_json};
use crate::exit;
use crate::render;
use crate::style::{self, bold, dim, paint, Cell, S};
use anyhow::{bail, Context, Result};
use portwise_core::http::{self, HttpInfo};
use portwise_core::stack::{self, ServiceState, ServiceStatus, Stack};
use portwise_core::store::Store;
use portwise_core::util::{local_url, shell_command, spawn_detached};
use portwise_core::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Options shared by the stack commands.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct StackFileArg {
    /// Use this project file instead of the nearest `.portwise.toml`.
    #[arg(long = "file", value_name = "PATH")]
    pub file: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
pub struct UpArgs {
    /// Services to start (default: all). Their dependencies are started too.
    pub services: Vec<String>,
    #[command(flatten)]
    pub file: StackFileArg,
    /// If a port is held by something outside the project, stop it (after confirmation).
    #[arg(long)]
    pub replace: bool,
    /// Don't ask before stopping a conflicting owner (with --replace).
    #[arg(short, long)]
    pub yes: bool,
    /// Show what would be started without starting anything.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
}

#[derive(clap::Args, Debug)]
pub struct DownArgs {
    /// Services to stop (default: all). Services that depend on them are stopped first.
    pub services: Vec<String>,
    #[command(flatten)]
    pub file: StackFileArg,
    /// Also stop services without a `command` (for example a database container of this project).
    #[arg(short, long)]
    pub all: bool,
    /// Don't ask for confirmation.
    #[arg(short, long)]
    pub yes: bool,
    /// Show the plan but don't do anything.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
    /// Skip SIGTERM: SIGKILL / TerminateProcess immediately.
    #[arg(short, long)]
    pub force: bool,
    /// Grace period before escalating to SIGKILL.
    #[arg(short, long, default_value = "5s", value_parser = humantime::parse_duration)]
    pub timeout: Duration,
}

#[derive(clap::Args, Debug)]
pub struct StatusArgs {
    #[command(flatten)]
    pub file: StackFileArg,
    /// Don't send HTTP requests to the services.
    #[arg(long)]
    pub no_http: bool,
    /// Machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
}

/// Load the stack from `--file` or the nearest `.portwise.toml`.
pub fn load(arg: &StackFileArg) -> Result<Stack> {
    let path = match &arg.file {
        Some(f) if f.is_dir() => f.join(stack::FILE_NAME),
        Some(f) => f.clone(),
        None => {
            let cwd = std::env::current_dir()?;
            stack::find(&cwd).with_context(|| {
                format!(
                    "no {} in {} or its parents; create one with `portwise init`",
                    stack::FILE_NAME,
                    tilde(&cwd)
                )
            })?
        }
    };
    Ok(stack::load(&path)?)
}

/// The nearest stack file's protected ports, if any (used by `portwise stop`).
pub fn protected_ports() -> Vec<u16> {
    std::env::current_dir()
        .ok()
        .and_then(|d| stack::find(&d))
        .and_then(|f| stack::load(&f).ok())
        .map(|s| s.protect)
        .unwrap_or_default()
}

fn engine(docker: bool) -> Result<Engine> {
    Engine::new(&ScanOptions {
        all_states: false,
        docker,
    })
    .context("failed to scan sockets")
}

fn status_of(stack: &Stack, e: &Engine, name: &str) -> ServiceStatus {
    stack
        .status(e.snapshot())
        .into_iter()
        .find(|s| s.name == name)
        .expect("service exists")
}

fn holder_label(st: &ServiceStatus) -> String {
    st.holder
        .as_ref()
        .map(|h| {
            let pid = h.pid.map(|p| format!(", PID {p}")).unwrap_or_default();
            format!("{}{pid}", h.label)
        })
        .unwrap_or_else(|| "an unknown owner".into())
}

fn log_path(stack: &Stack, service: &str) -> PathBuf {
    Store::open_default().logs_dir().join(format!(
        "{}-{}.log",
        stack::service_name(&stack.name),
        service
    ))
}

fn tail(path: &Path, n: usize) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| l.to_string())
        .collect()
}

pub fn up(a: &UpArgs, docker: bool) -> Result<u8> {
    let stack = load(&a.file)?;
    let services = stack.start_order(&a.services)?;
    println!(
        "{} {} {}",
        style::arrow(),
        bold(&stack.name),
        dim(tilde(&stack.file))
    );
    if services.is_empty() {
        println!("{}", dim("No services defined."));
        return Ok(exit::OK);
    }
    let mut started = 0;
    for s in services {
        let Some(port) = s.port else {
            println!(
                "  {} {} {}",
                dim("·"),
                bold(&s.name),
                dim("has no port; skipped")
            );
            continue;
        };
        let e = engine(docker)?;
        let st = status_of(&stack, &e, &s.name);
        match st.state {
            ServiceState::Running => {
                println!(
                    "  {} {} {} {}",
                    style::ok_mark(),
                    bold(&s.name),
                    paint(format!(":{port}"), S::BoldCyan),
                    dim(format!("already running ({})", holder_label(&st)))
                );
                continue;
            }
            ServiceState::Conflict => {
                let who = holder_label(&st);
                if !a.replace {
                    eprintln!(
                        "  {} {} {} is held by {who}, which isn't part of {}.",
                        style::err_mark(),
                        bold(&s.name),
                        paint(format!(":{port}"), S::BoldCyan),
                        stack.name
                    );
                    eprintln!(
                        "    {}",
                        dim(format!("Run `portwise explain {port}`, or `portwise up --replace` to stop it first."))
                    );
                    return Ok(exit::BUSY);
                }
                if stack.protect.contains(&port) {
                    eprintln!(
                        "  {} :{port} is in `protect`; not stopping {who}",
                        style::err_mark()
                    );
                    return Ok(exit::BLOCKED);
                }
                let plan = e.plan(&Target::Port(port), &StopOptions::default());
                print!("{}", render::plan_text(&plan));
                if plan.is_blocked() {
                    return Ok(crate::commands::block_code(&plan).max(exit::BUSY));
                }
                if a.dry_run {
                    println!("{}", dim("Dry run: not stopping it."));
                } else {
                    if !a.yes && !confirm(&format!("Stop {who} to free :{port}?"))? {
                        println!("{}", dim("Cancelled."));
                        return Ok(exit::BUSY);
                    }
                    let report = execute(&plan, &mut |l| println!("    {} {}", dim("·"), dim(l)));
                    println!("{}", render::report_line(&report));
                    crate::state::record(&e, &plan, &report);
                    if !report.success {
                        return Ok(exit::BUSY);
                    }
                }
            }
            ServiceState::Stopped | ServiceState::Unknown => {}
        }
        let Some(cmd) = s.command.as_deref() else {
            eprintln!(
                "  {} {} {} isn't running, and the project file has no `command` for it; start it yourself.",
                style::err_mark(),
                bold(&s.name),
                paint(format!(":{port}"), S::BoldCyan)
            );
            return Ok(exit::BUSY);
        };
        if a.dry_run {
            println!(
                "  {} {} {} {} `{cmd}` {}",
                dim("·"),
                bold(&s.name),
                paint(format!(":{port}"), S::BoldCyan),
                dim("would run"),
                dim(format!("in {}", tilde(&s.cwd)))
            );
            continue;
        }
        if !s.cwd.is_dir() {
            bail!(
                "service `{}`: directory {} doesn't exist",
                s.name,
                tilde(&s.cwd)
            );
        }
        let log = log_path(&stack, &s.name);
        let mut command = shell_command(cmd);
        command
            .current_dir(&s.cwd)
            .env("PORT", port.to_string())
            .envs(&s.env);
        let mut child = spawn_detached(&mut command, &log)
            .with_context(|| format!("failed to start `{cmd}` for {}", s.name))?;
        let t0 = Instant::now();
        let timeout = Duration::from_secs(s.ready_timeout_s);
        let interactive = std::io::IsTerminal::is_terminal(&std::io::stderr());
        loop {
            if tcp_accepting(port) {
                if interactive {
                    eprint!("\r\x1b[2K");
                }
                println!(
                    "  {} {} {} up after {:.1} s {}",
                    style::ok_mark(),
                    bold(&s.name),
                    paint(format!(":{port}"), S::BoldCyan),
                    t0.elapsed().as_secs_f32(),
                    dim(format!("PID {} · log {}", child.id(), tilde(&log)))
                );
                started += 1;
                break;
            }
            if let Ok(Some(status)) = child.try_wait() {
                if interactive {
                    eprint!("\r\x1b[2K");
                }
                eprintln!(
                    "  {} {} exited ({status}) before :{port} was ready. Last lines of {}:",
                    style::err_mark(),
                    bold(&s.name),
                    tilde(&log)
                );
                for l in tail(&log, 12) {
                    eprintln!("    {}", dim(l));
                }
                return Ok(exit::BUSY);
            }
            if t0.elapsed() >= timeout {
                if interactive {
                    eprint!("\r\x1b[2K");
                }
                eprintln!(
                    "  {} {} didn't accept connections on :{port} within {} (PID {} is still running; see {}).",
                    style::warn_mark(),
                    bold(&s.name),
                    humantime::format_duration(timeout),
                    child.id(),
                    tilde(&log)
                );
                return Ok(exit::BUSY);
            }
            if interactive {
                eprint!(
                    "\r  {} starting {}… {}",
                    paint("◌", S::Cyan),
                    s.name,
                    dim(format!("{:.1}s", t0.elapsed().as_secs_f32()))
                );
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }
    if !a.dry_run {
        println!(
            "{} {}",
            style::ok_mark(),
            bold(format!("{} is up", stack.name))
        );
        if started > 0 {
            println!(
                "{}",
                dim("`portwise status` shows the services, `portwise down` stops them.")
            );
        }
    }
    Ok(exit::OK)
}

pub fn down(a: &DownArgs, docker: bool) -> Result<u8> {
    let stack = load(&a.file)?;
    let services = stack.stop_order(&a.services)?;
    let e = engine(docker)?;
    let statuses = stack.status(e.snapshot());
    let opts = StopOptions {
        force: a.force,
        timeout_ms: a.timeout.as_millis() as u64,
        ..Default::default()
    };
    let mut plans = Vec::new();
    for s in services {
        let st = statuses.iter().find(|x| x.name == s.name).expect("service");
        let Some(port) = s.port else { continue };
        let tag = format!(
            "{} {}",
            bold(&s.name),
            paint(format!(":{port}"), S::BoldCyan)
        );
        match st.state {
            ServiceState::Stopped | ServiceState::Unknown => {
                println!("  {} {tag} {}", dim("·"), dim("not running"));
            }
            ServiceState::Conflict => println!(
                "  {} {tag} {}",
                style::warn_mark(),
                dim(format!(
                    "held by {}, which isn't part of {}; leaving it alone",
                    holder_label(st),
                    stack.name
                ))
            ),
            ServiceState::Running if stack.protect.contains(&port) => println!(
                "  {} {tag} {}",
                dim("·"),
                dim("in `protect`; leaving it running")
            ),
            ServiceState::Running
                if s.command.is_none()
                    && a.all
                    && !st.holder.as_ref().is_some_and(|h| stack.owns(h)) =>
            {
                println!(
                    "  {} {tag} {}",
                    dim("·"),
                    dim(format!(
                        "held by {}, which isn't part of {}; leaving it alone",
                        holder_label(st),
                        stack.name
                    ))
                )
            }
            ServiceState::Running if s.command.is_none() && !a.all => println!(
                "  {} {tag} {}",
                dim("·"),
                dim("has no `command` (started elsewhere); use --all to stop it too")
            ),
            ServiceState::Running => {
                let plan = e.plan(&Target::Port(port), &opts);
                println!("  {} {tag}", paint("●", S::Green));
                print!("{}", render::plan_text(&plan));
                plans.push(plan);
            }
        }
    }
    let runnable: Vec<&ActionPlan> = plans.iter().filter(|p| !p.is_blocked()).collect();
    if runnable.is_empty() {
        println!("{}", dim("Nothing to stop."));
        return Ok(plans
            .iter()
            .map(crate::commands::block_code)
            .max()
            .unwrap_or(exit::OK));
    }
    if a.dry_run {
        println!("{}", dim("Dry run: nothing was changed."));
        return Ok(exit::OK);
    }
    if !a.yes
        && !confirm(&format!(
            "Stop {} service(s) of {}?",
            runnable.len(),
            stack.name
        ))?
    {
        println!("{}", dim("Cancelled."));
        return Ok(exit::BUSY);
    }
    let mut code = exit::OK;
    for plan in plans.iter() {
        if plan.is_blocked() {
            code = code.max(crate::commands::block_code(plan));
            continue;
        }
        let report = execute(plan, &mut |l| println!("    {} {}", dim("·"), dim(l)));
        println!("{}", render::report_line(&report));
        crate::state::record(&e, plan, &report);
        if !report.success {
            code = code.max(exit::BUSY);
        }
    }
    Ok(code)
}

#[derive(serde::Serialize)]
struct StatusRow<'a> {
    name: &'a str,
    port: Option<u16>,
    state: ServiceState,
    pid: Option<u32>,
    holder: Option<&'a str>,
    url: Option<String>,
    http: Option<HttpInfo>,
}

pub fn status(a: &StatusArgs, docker: bool) -> Result<u8> {
    let stack = load(&a.file)?;
    let e = engine(docker)?;
    let statuses = stack.status(e.snapshot());
    // Probe the running services in parallel: a slow one mustn't hold up the rest.
    let http: Vec<Option<HttpInfo>> = std::thread::scope(|sc| {
        let handles: Vec<_> = statuses
            .iter()
            .map(|st| {
                let health = stack
                    .service(&st.name)
                    .and_then(|s| s.health.clone())
                    .unwrap_or_else(|| "/".into());
                let probe = !a.no_http && st.state == ServiceState::Running;
                let port = st.port;
                sc.spawn(move || {
                    port.filter(|_| probe)
                        .and_then(|p| http::probe(p, &health, Duration::from_millis(1500)))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().ok().flatten())
            .collect()
    });
    let all_up = statuses
        .iter()
        .all(|s| s.port.is_none() || s.state == ServiceState::Running);
    if a.json {
        let rows: Vec<StatusRow> = statuses
            .iter()
            .zip(http)
            .map(|(st, h)| StatusRow {
                name: &st.name,
                port: st.port,
                state: st.state,
                pid: st.holder.as_ref().and_then(|h| h.pid),
                holder: st.holder.as_ref().map(|h| h.label.as_str()),
                url: st.port.map(local_url),
                http: h,
            })
            .collect();
        print_json(&serde_json::json!({
            "name": stack.name,
            "file": stack.file,
            "services": rows,
        }))?;
        return Ok(if all_up { exit::OK } else { exit::BUSY });
    }
    println!("{} {}", bold(&stack.name), dim(tilde(&stack.file)));
    if statuses.is_empty() {
        println!("{}", dim("No services defined."));
        return Ok(exit::OK);
    }
    let cols = [
        style::col("SERVICE"),
        style::rcol("PORT"),
        style::col("STATE"),
        style::rcol("PID"),
        style::flex("PROCESS", 8),
        style::flex("HTTP", 10),
    ];
    let rows: Vec<Vec<Cell>> = statuses
        .iter()
        .zip(&http)
        .map(|(st, h)| {
            let (state, sty) = match st.state {
                ServiceState::Running => ("running", S::Green),
                ServiceState::Stopped => ("stopped", S::Dim),
                ServiceState::Conflict => ("conflict", S::BoldYellow),
                ServiceState::Unknown => ("no port", S::Dim),
            };
            let holder = st.holder.as_ref();
            vec![
                Cell::new(st.name.clone(), S::Bold),
                Cell::new(
                    st.port.map(|p| p.to_string()).unwrap_or_default(),
                    S::BoldCyan,
                ),
                Cell::new(state, sty),
                Cell::new(
                    holder
                        .and_then(|h| h.pid)
                        .map(|p| p.to_string())
                        .unwrap_or_default(),
                    S::Plain,
                ),
                Cell::new(holder.map(|h| h.label.clone()).unwrap_or_default(), S::Dim),
                Cell::new(
                    h.as_ref().map(|h| h.summary()).unwrap_or_default(),
                    match h {
                        Some(h) if h.healthy() => S::Plain,
                        Some(_) => S::Yellow,
                        None => S::Dim,
                    },
                ),
            ]
        })
        .collect();
    print!("{}", style::table(&cols, &rows, style::term_width()));
    for st in statuses
        .iter()
        .filter(|s| s.state == ServiceState::Conflict)
    {
        if let Some(p) = st.port {
            println!(
                "{} :{p} is held by {}; `portwise explain {p}` says why.",
                style::warn_mark(),
                holder_label(st)
            );
        }
    }
    Ok(if all_up { exit::OK } else { exit::BUSY })
}

/// Quote a word for a POSIX shell when needed.
pub fn shell_quote(w: &str) -> String {
    if !w.is_empty()
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c))
    {
        w.to_string()
    } else {
        format!("'{}'", w.replace('\'', r"'\''"))
    }
}

/// `portwise init` without a shell: write a starter `.portwise.toml` from the dev servers running
/// under the current directory.
pub fn init_project(force: bool, print: bool, docker: bool) -> Result<u8> {
    let cwd = std::env::current_dir()?;
    let root = std::fs::canonicalize(&cwd).unwrap_or(cwd.clone());
    let path = cwd.join(stack::FILE_NAME);
    if path.exists() && !force && !print {
        bail!("{} already exists (use --force to overwrite)", tilde(&path));
    }
    let e = engine(docker)?;
    let t = e.table();
    let mut seen_roots = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    let mut services = Vec::new();
    let mut entries: Vec<&PortEntry> = e
        .snapshot()
        .entries
        .iter()
        .filter(|x| {
            x.protocol == Protocol::Tcp && x.state.is_listening() && x.is_mine && !x.protected
        })
        .filter(|x| {
            x.process
                .as_ref()
                .and_then(|p| p.cwd.as_deref())
                .is_some_and(|c| c.starts_with(&root))
        })
        .collect();
    entries.sort_by_key(|x| x.port);
    for x in entries {
        let Some(pid) = x.pid else { continue };
        let tree = e.tree_root(pid);
        if !seen_roots.insert(tree) {
            continue;
        }
        let proc_ = t.get(tree).or(x.process.as_ref());
        let base = x
            .project
            .as_ref()
            .map(|p| p.name.clone())
            .or_else(|| x.framework.as_ref().map(|f| f.name.clone()))
            .unwrap_or_else(|| {
                x.process
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_default()
            });
        let mut name = stack::service_name(&base);
        let mut n = 2;
        while !names.insert(name.clone()) {
            name = format!("{}-{n}", stack::service_name(&base));
            n += 1;
        }
        let command = proc_
            .filter(|p| !p.cmdline.is_empty())
            .map(|p| match p.cmdline.as_slice() {
                // A process that rewrote its title (`npm run dev`) shows one argument with spaces.
                [one] => one.clone(),
                many => many
                    .iter()
                    .map(|w| shell_quote(w))
                    .collect::<Vec<_>>()
                    .join(" "),
            });
        let rel = proc_
            .and_then(|p| p.cwd.as_ref())
            .and_then(|c| c.strip_prefix(&root).ok())
            .map(|r| r.to_string_lossy().replace('\\', "/"));
        services.push((name, x.port, command, rel));
    }
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let text = stack::scaffold(&name, &services);
    // Never write a file we can't read back.
    stack::parse(&text, &root, &path)?;
    if print {
        print!("{text}");
        return Ok(exit::OK);
    }
    std::fs::write(&path, &text).with_context(|| format!("can't write {}", tilde(&path)))?;
    if services.is_empty() {
        println!(
            "{} wrote {} with an example service (no dev servers are running under this directory).",
            style::ok_mark(),
            bold(tilde(&path))
        );
    } else {
        println!(
            "{} wrote {} with {} service(s): {}",
            style::ok_mark(),
            bold(tilde(&path)),
            services.len(),
            services
                .iter()
                .map(|s| s.0.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!("{}", dim("Check the commands, then `portwise up` starts the stack and `portwise status` shows it."));
    Ok(exit::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("npm"), "npm");
        assert_eq!(shell_quote("--port=3000"), "--port=3000");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
    }
}
