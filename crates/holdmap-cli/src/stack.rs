//! `holdmap up`, `down`, `status` and `init` (project file): manage a project's services from
//! its `.holdmap.toml`. Parsing, ordering and ownership live in `holdmap_core::stack`; this
//! module starts processes, waits for them and renders the results. Stopping always goes through
//! the engine's plans, so the usual safety rules apply.

use crate::commands::{confirm, print_json};
use crate::exit;
use crate::render;
use crate::style::{self, bold, dim, paint, Cell, S};
use anyhow::{bail, Context, Result};
use holdmap_core::http::{self, HttpInfo};
use holdmap_core::stack::{self, ServiceState, ServiceStatus, Stack};
use holdmap_core::store::Store;
use holdmap_core::util::{local_url, shell_command, spawn_detached};
use holdmap_core::*;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Options shared by the stack commands.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct StackFileArg {
    /// Use this project file instead of the nearest `.holdmap.toml`.
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

/// Load the stack from `--file` or the nearest `.holdmap.toml`.
pub fn load(arg: &StackFileArg) -> Result<Stack> {
    let path = match &arg.file {
        Some(f) if f.is_dir() => f.join(stack::FILE_NAME),
        Some(f) => f.clone(),
        None => {
            let cwd = std::env::current_dir()?;
            stack::find(&cwd).with_context(|| {
                format!(
                    "no {} in {} or its parents; create one with `holdmap init`",
                    stack::FILE_NAME,
                    tilde(&cwd)
                )
            })?
        }
    };
    Ok(stack::load(&path)?)
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

const MAX_LOG_TAIL_BYTES: u64 = 64 * 1024;
const MAX_HEALTH_PROBES: usize = 32;

fn tail(path: &Path, n: usize) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: inspect the link rather than its target.
        options.custom_flags(0x0020_0000);
    }
    let Ok(mut file) = options.open(path) else {
        return Vec::new();
    };
    let Ok(metadata) = file.metadata() else {
        return Vec::new();
    };
    if !metadata.is_file() {
        return Vec::new();
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT includes junctions and other redirects.
        if metadata.file_attributes() & 0x400 != 0 {
            return Vec::new();
        }
    }
    let offset = metadata.len().saturating_sub(MAX_LOG_TAIL_BYTES);
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    // A growing log must not turn this diagnostic read into an unbounded allocation.
    if file
        .take(MAX_LOG_TAIL_BYTES)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Vec::new();
    }
    let bytes = if offset > 0 {
        // Discard the partial first line, including any split UTF-8 or credential prefix.
        bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|newline| &bytes[newline + 1..])
            .unwrap_or_default()
    } else {
        bytes.as_slice()
    };
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|line| holdmap_core::redact::line(line))
        .collect()
}

pub fn up(a: &UpArgs, docker: bool) -> Result<u8> {
    let stack = load(&a.file)?;
    crate::project_policy::ProjectProtection::for_stack(&stack)?;
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
    let mut failed: Vec<&str> = Vec::new();
    let mut code = exit::OK;
    for s in services {
        // Dependents of a service that didn't come up are skipped; unrelated ones still start.
        if let Some(dep) = s.depends_on.iter().find(|d| failed.contains(&d.as_str())) {
            eprintln!(
                "  {} {} {}",
                style::err_mark(),
                bold(&s.name),
                dim(format!("skipped: it depends on {dep}, which didn't start"))
            );
            failed.push(&s.name);
            continue;
        }
        match up_one(&stack, s, a, docker)? {
            UpStep::Started => started += 1,
            UpStep::Done => {}
            UpStep::Failed(c) => {
                failed.push(&s.name);
                code = code.max(c);
            }
        }
    }
    if !failed.is_empty() {
        eprintln!(
            "{} {}",
            style::err_mark(),
            bold(format!(
                "{} didn't start: {}",
                stack.name,
                failed.join(", ")
            ))
        );
        return Ok(code);
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
                dim("`holdmap status` shows the services, `holdmap down` stops them.")
            );
        }
    }
    Ok(exit::OK)
}

/// What happened to one service in `up`.
enum UpStep {
    Started,
    /// Already running, skipped or a dry run: nothing to wait for.
    Done,
    /// Didn't come up; the exit code to report.
    Failed(u8),
}

fn up_one(stack: &Stack, s: &stack::Service, a: &UpArgs, docker: bool) -> Result<UpStep> {
    let Some(port) = s.port else {
        println!(
            "  {} {} {}",
            dim("·"),
            bold(&s.name),
            dim("has no port; skipped")
        );
        return Ok(UpStep::Done);
    };
    let e = engine(docker)?;
    let st = status_of(stack, &e, &s.name);
    match st.state {
        ServiceState::Running => {
            println!(
                "  {} {} {} {}",
                style::ok_mark(),
                bold(&s.name),
                paint(format!(":{port}"), S::BoldCyan),
                dim(format!("already running ({})", holder_label(&st)))
            );
            return Ok(UpStep::Done);
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
                    dim(format!(
                        "Run `holdmap explain {port}`, or `holdmap up --replace` to stop it first."
                    ))
                );
                return Ok(UpStep::Failed(exit::BUSY));
            }
            let plan = crate::project_policy::ProjectProtection::for_stack(stack)?
                .apply(&e, e.plan(&Target::Port(port), &StopOptions::default()));
            print!("{}", render::plan_text(&plan));
            if plan.is_blocked() {
                return Ok(UpStep::Failed(
                    crate::commands::block_code(&plan).max(exit::BUSY),
                ));
            }
            if a.dry_run {
                println!("{}", dim("Dry run: not stopping it."));
            } else {
                if !a.yes && !confirm(&format!("Stop {who} to free :{port}?"))? {
                    println!("{}", dim("Cancelled."));
                    return Ok(UpStep::Failed(exit::BUSY));
                }
                let fresh =
                    crate::project_policy::checked_engine(docker, &plan, Some(&stack.file))?;
                let report = execute(&plan, &mut |l| println!("    {} {}", dim("·"), dim(l)));
                println!("{}", render::report_line(&report));
                crate::state::record(&fresh, &plan, &report);
                if !report.success {
                    return Ok(UpStep::Failed(exit::BUSY));
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
        return Ok(UpStep::Failed(exit::BUSY));
    };
    let displayed_command = holdmap_core::redact::line(cmd);
    if a.dry_run {
        println!(
            "  {} {} {} {} `{displayed_command}` {}",
            dim("·"),
            bold(&s.name),
            paint(format!(":{port}"), S::BoldCyan),
            dim("would run"),
            dim(format!("in {}", tilde(&s.cwd)))
        );
        return Ok(UpStep::Done);
    }
    if !s.cwd.is_dir() {
        bail!(
            "service `{}`: directory {} doesn't exist",
            s.name,
            tilde(&s.cwd)
        );
    }
    let log = log_path(stack, &s.name);
    let mut command = shell_command(cmd);
    command
        .current_dir(&s.cwd)
        .env("PORT", port.to_string())
        .envs(&s.env);
    let mut child = spawn_detached(&mut command, &log)
        .with_context(|| format!("failed to start `{displayed_command}` for {}", s.name))?;
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
            return Ok(UpStep::Started);
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
            return Ok(UpStep::Failed(exit::BUSY));
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
            return Ok(UpStep::Failed(exit::BUSY));
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

pub fn down(a: &DownArgs, docker: bool) -> Result<u8> {
    let stack = load(&a.file)?;
    let services = stack.stop_order(&a.services)?;
    let e = engine(docker)?;
    let protection = crate::project_policy::ProjectProtection::for_stack(&stack)?;
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
                let plan = protection.apply(&e, e.plan(&Target::Port(port), &opts));
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
            "Stop {} of {}?",
            holdmap_core::util::count(runnable.len(), "service", "services"),
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
        let fresh = crate::project_policy::checked_engine(docker, plan, Some(&stack.file))?;
        let report = execute(plan, &mut |l| println!("    {} {}", dim("·"), dim(l)));
        println!("{}", render::report_line(&report));
        crate::state::record(&fresh, plan, &report);
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
    let http = probe_status_http(&stack, &statuses, !a.no_http, |port, health| {
        http::probe(port, health, Duration::from_millis(1500))
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
                "{} :{p} is held by {}; `holdmap explain {p}` says why.",
                style::warn_mark(),
                holder_label(st)
            );
        }
    }
    Ok(if all_up { exit::OK } else { exit::BUSY })
}

/// Bound concurrent probes and preserve service order, including skipped services.
fn probe_status_http<F>(
    stack: &Stack,
    statuses: &[ServiceStatus],
    enabled: bool,
    probe: F,
) -> Vec<Option<HttpInfo>>
where
    F: Fn(u16, &str) -> Option<HttpInfo> + Sync,
{
    let mut results = vec![None; statuses.len()];
    if !enabled {
        return results;
    }
    let health: std::collections::HashMap<_, _> = stack
        .services
        .iter()
        .map(|service| {
            (
                service.name.as_str(),
                service.health.as_deref().unwrap_or("/"),
            )
        })
        .collect();
    let tasks: Vec<_> = statuses
        .iter()
        .enumerate()
        .filter(|(_, status)| status.state == ServiceState::Running)
        .filter_map(|(index, status)| {
            Some((
                index,
                status.port?,
                health.get(status.name.as_str()).copied().unwrap_or("/"),
            ))
        })
        .collect();
    for chunk in tasks.chunks(MAX_HEALTH_PROBES) {
        let probe = &probe;
        std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|&(index, port, health)| {
                    (
                        index,
                        std::thread::Builder::new()
                            .spawn_scoped(scope, move || probe(port, health)),
                    )
                })
                .collect();
            for (index, handle) in handles {
                results[index] = handle.ok().and_then(|handle| handle.join().ok()).flatten();
            }
        });
    }
    results
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

/// `holdmap init` without a shell: write a starter `.holdmap.toml` from the dev servers running
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
            // Never copy a password or token from a running command into a file that tends
            // to get committed; `••••` makes the gap obvious.
            .map(|p| holdmap_core::redact::args(&p.cmdline))
            .map(|cmdline| match cmdline.as_slice() {
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
    let mut file = if force {
        // A project checkout must not redirect an authorized config overwrite to an
        // unrelated file through a symlink or hard link.
        holdmap_core::util::create_private(&path)
    } else {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path)
    }
    .with_context(|| format!("can't write {}", tilde(&path)))?;
    file.write_all(text.as_bytes())?;
    if services.is_empty() {
        println!(
            "{} wrote {} with an example service (no dev servers are running under this directory).",
            style::ok_mark(),
            bold(tilde(&path))
        );
    } else {
        println!(
            "{} wrote {} with {}: {}",
            style::ok_mark(),
            bold(tilde(&path)),
            holdmap_core::util::count(services.len(), "service", "services"),
            services
                .iter()
                .map(|s| s.0.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!(
        "{}",
        dim(
            "Check the commands, then `holdmap up` starts the stack and `holdmap status` shows it."
        )
    );
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

    #[test]
    fn log_tail_reads_only_the_bounded_suffix_and_redacts_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("service.log");
        let mut file = std::fs::File::create(&path).unwrap();
        // A sparse multi-gigabyte log must use the same bounded read as a small log.
        file.set_len(4 * 1024 * 1024 * 1024).unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        writeln!(file, "\nfirst\nsecond\n--password fixture-secret\nlast").unwrap();
        let lines = tail(&path, 3);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "second");
        assert!(lines[1].contains("••••"));
        assert!(!lines[1].contains("fixture-secret"));
        assert_eq!(lines[2], "last");
        assert!(tail(&path, 0).is_empty());
        assert!(tail(directory.path(), 3).is_empty());
        assert!(tail(&directory.path().join("missing"), 3).is_empty());
    }

    #[test]
    fn log_tail_handles_invalid_utf8_and_never_shows_a_truncated_first_line() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("service.log");
        std::fs::write(&path, b"first\ninvalid \xff\nlast\n").unwrap();
        assert_eq!(tail(&path, 2), vec!["invalid \u{fffd}", "last"]);
        std::fs::write(&path, vec![b'x'; MAX_LOG_TAIL_BYTES as usize + 1]).unwrap();
        assert!(tail(&path, 12).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn log_tail_refuses_symlinks_and_fifos_without_blocking() {
        use std::os::unix::ffi::OsStrExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("service.log");
        std::fs::write(&path, "private fixture\n").unwrap();
        let link = directory.path().join("link.log");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(tail(&link, 12).is_empty());
        let fifo = directory.path().join("fifo.log");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: name is a valid NUL-terminated path in this test's private directory.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(tail(&fifo, 12).is_empty());
    }

    #[test]
    fn health_probes_bound_concurrency_preserve_order_and_skip_inactive_services() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let text: String = (0..70)
            .map(|index| {
                format!(
                    "[services.service{index}]\nport = {}\nhealth = '/health{index}'\n",
                    10000 + index
                )
            })
            .collect();
        let stack = stack::parse(
            &text,
            Path::new("/fixture"),
            Path::new("/fixture/.holdmap.toml"),
        )
        .unwrap();
        let mut statuses: Vec<_> = stack
            .services
            .iter()
            .map(|service| ServiceStatus {
                name: service.name.clone(),
                port: service.port,
                state: ServiceState::Running,
                holder: None,
            })
            .collect();
        statuses[1].state = ServiceState::Stopped;
        statuses[2].port = None;
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let calls = AtomicUsize::new(0);
        let results = probe_status_http(&stack, &statuses, true, |port, path| {
            let count = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(count, Ordering::SeqCst);
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(path, format!("/health{}", port - 10000));
            std::thread::sleep(Duration::from_millis(5));
            active.fetch_sub(1, Ordering::SeqCst);
            Some(HttpInfo {
                port,
                status: 200,
                reason: "OK".into(),
                title: None,
                server: None,
                location: None,
                elapsed_ms: 5,
            })
        });
        assert_eq!(calls.load(Ordering::SeqCst), 68);
        assert!(peak.load(Ordering::SeqCst) <= MAX_HEALTH_PROBES);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        for (index, result) in results.iter().enumerate() {
            if index == 1 || index == 2 {
                assert!(result.is_none());
            } else {
                assert_eq!(result.as_ref().unwrap().port, statuses[index].port.unwrap());
            }
        }
        let disabled = probe_status_http(&stack, &statuses, false, |_, _| {
            panic!("disabled HTTP must not probe")
        });
        assert!(disabled.iter().all(Option::is_none));
    }
}
