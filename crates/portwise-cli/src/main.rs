//! portwise — see which ports are in use, *why*, and stop the right thing safely.

mod commands;
mod graph;
mod remote;
mod render;
mod state;
mod style;
mod tui;
mod watch;

use clap::{Args, CommandFactory, Parser, Subcommand};
use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::Duration;
use style::ColorChoice;

/// Exit codes (stable contract for scripts and agents).
pub mod exit {
    pub const OK: u8 = 0;
    /// Port busy / not found / still busy after stop / timeout.
    pub const BUSY: u8 = 1;
    pub const ERROR: u8 = 2;
    /// Blocked by the safety policy (protected process, OS service).
    pub const BLOCKED: u8 = 3;
    /// Needs elevated privileges (another user's or root's process).
    pub const ELEVATION: u8 = 4;
}

#[derive(Parser, Debug)]
#[command(
    name = "portwise",
    version,
    about = "See which ports are in use, why, and stop the right thing, safely.",
    long_about = "portwise shows every listening port with its owning process, project and framework, \
explains in plain English why a port is busy (dev-server tree, Docker container, systemd/pm2/brew \
service, OS feature, TIME_WAIT, another user) and stops the correct thing gracefully, verifying \
the port is free afterwards.\n\nRun without arguments in a terminal to open the interactive TUI.",
    after_help = "EXAMPLES:\n  portwise                      Open the interactive TUI\n  portwise list --dev           Only dev servers\n  portwise explain 3000         Why is 3000 busy?\n  portwise stop 3000            Gracefully stop whatever holds 3000\n  portwise stop 3000 --dry-run  Show the plan only\n  portwise run -p 3000 -- npm run dev\n  portwise free-port --near 3000\n  portwise wait 5432 --timeout 30s\n  portwise graph                Which services depend on which\n  portwise stop --cluster shop  Stop a whole stack, dependents first\n  portwise watch                Stream new/closed/conflicting listeners\n\nEXIT CODES: 0 ok · 1 busy/not found/timeout · 2 error · 3 blocked by safety policy · 4 needs elevation"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// When to use colours.
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "auto",
        env = "PORTWISE_COLOR"
    )]
    color: ColorChoice,

    /// Don't query Docker/Podman/OrbStack/Colima.
    #[arg(long, global = true)]
    no_docker: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List ports in use (listening sockets by default).
    #[command(visible_alias = "ls")]
    List(ListArgs),
    /// Show everything about a port: owner, process tree, project, plan.
    Inspect(PortArgs),
    /// Explain in plain English why a port is busy and what to do.
    #[command(visible_alias = "why")]
    Explain(PortArgs),
    /// Gracefully stop whatever holds a port (SIGTERM → SIGKILL), then verify it is free.
    Stop(StopArgs),
    /// Like `stop --force`: kill immediately (SIGKILL / TerminateProcess).
    Kill(StopArgs),
    /// Print a free TCP port.
    FreePort(FreePortArgs),
    /// Wait until a port is accepting connections (or free, with --free).
    Wait(WaitArgs),
    /// Free a port (safely) and run a command on it, with PORT set.
    Run(RunArgs),
    /// Show which services talk to which (dependencies, clusters) as a tree, JSON, DOT or Mermaid.
    #[command(visible_alias = "mesh")]
    Graph(graph::GraphArgs),
    /// Stream port events: new listeners, closed listeners, conflicts.
    Watch(watch::WatchArgs),
    /// Pin a port (favourite): shown first and watched even when free.
    Pin(state::PinArgs),
    /// Remove a pin.
    Unpin(state::PinArgs),
    /// List pinned ports and whether they are in use.
    Pins {
        #[arg(long)]
        json: bool,
    },
    /// Ports portwise stopped recently, with the command that ran there.
    History(state::HistoryArgs),
    /// Stop what holds a port and start the same command again (or re-run it from history).
    Restart(state::RestartArgs),
    /// Open http://localhost:PORT in the browser.
    Open(state::OpenArgs),
    /// Inspect another machine's ports over SSH (agentless, read-only).
    Ssh(remote::SshArgs),
    /// Open the interactive terminal UI.
    Tui,
    /// Run the MCP (Model Context Protocol) server on stdio for AI agents.
    Mcp,
    /// Generate shell completions.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Print the man page (roff).
    Man,
}

#[derive(Args, Debug, Default)]
pub struct ListArgs {
    /// Filter: words, `:3000`, `3000-3999`, `proto:udp`, `pid:123`, `user:me`.
    pub query: Vec<String>,
    /// Include non-listening sockets (ESTABLISHED, TIME_WAIT, …).
    #[arg(short, long)]
    pub all: bool,
    /// TCP only.
    #[arg(long, conflicts_with = "udp")]
    pub tcp: bool,
    /// UDP only.
    #[arg(long)]
    pub udp: bool,
    /// Only likely development servers (frameworks, projects, databases, containers).
    #[arg(short, long)]
    pub dev: bool,
    /// Only processes owned by you.
    #[arg(short, long)]
    pub mine: bool,
    /// Only ports reachable from the network (bound to 0.0.0.0 / ::).
    #[arg(short = 'x', long)]
    pub exposed: bool,
    /// Port range, e.g. 3000-3999.
    #[arg(short, long, value_parser = parse_range)]
    pub range: Option<(u16, u16)>,
    /// Sort order.
    #[arg(short, long, value_enum, default_value = "port")]
    pub sort: SortKey,
    /// Show the full command and user columns.
    #[arg(short, long)]
    pub wide: bool,
    /// Machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum SortKey {
    #[default]
    Port,
    Pid,
    Name,
    Proto,
    Memory,
    Uptime,
}

#[derive(Args, Debug)]
pub struct PortArgs {
    /// Port number (e.g. 3000 or :3000).
    #[arg(value_parser = parse_port)]
    pub port: u16,
    /// Only consider UDP.
    #[arg(long)]
    pub udp: bool,
    /// Machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct StopArgs {
    /// Ports, `pid:<n>` or process names. Bare numbers are ports.
    pub targets: Vec<String>,
    /// Stop a process by PID (repeatable).
    #[arg(long = "pid", value_name = "PID")]
    pub pids: Vec<u32>,
    /// Stop processes by exact name (repeatable).
    #[arg(long = "name", value_name = "NAME")]
    pub names: Vec<String>,
    /// Stop every service in a cluster (see `portwise graph`), dependents first.
    #[arg(long = "cluster", value_name = "CLUSTER")]
    pub clusters: Vec<String>,
    /// Skip SIGTERM: SIGKILL / TerminateProcess immediately.
    #[arg(short, long)]
    pub force: bool,
    /// Don't ask for confirmation.
    #[arg(short, long)]
    pub yes: bool,
    /// Show the plan but don't do anything.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
    /// Grace period before escalating to SIGKILL.
    #[arg(short, long, default_value = "5s", value_parser = humantime::parse_duration)]
    pub timeout: Duration,
    /// Also allow stopping protected processes (system services, editors, terminals).
    #[arg(long)]
    pub allow_protected: bool,
    /// Only stop the socket holder, not its dev-server tree.
    #[arg(long)]
    pub no_tree: bool,
    /// Only consider UDP sockets for port targets.
    #[arg(long)]
    pub udp: bool,
    /// Machine-readable JSON output (implies no prompt; requires --yes unless --dry-run).
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct FreePortArgs {
    /// Return the first free port at or after this one.
    #[arg(long, value_parser = parse_port)]
    pub near: Option<u16>,
    /// Search within a range, e.g. 3000-3999.
    #[arg(long, value_parser = parse_range, conflicts_with = "near")]
    pub range: Option<(u16, u16)>,
    /// How many ports to return.
    #[arg(short = 'c', long, default_value_t = 1)]
    pub count: usize,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct WaitArgs {
    #[arg(value_parser = parse_port)]
    pub port: u16,
    /// Give up after this long (exit code 1).
    #[arg(short, long, default_value = "30s", value_parser = humantime::parse_duration)]
    pub timeout: Duration,
    /// Wait until the port is free instead.
    #[arg(long)]
    pub free: bool,
    /// Polling interval.
    #[arg(long, default_value = "200ms", value_parser = humantime::parse_duration)]
    pub interval: Duration,
    /// Print nothing; just use the exit code.
    #[arg(short, long)]
    pub quiet: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Port the command needs.
    #[arg(short, long, value_parser = parse_port)]
    pub port: u16,
    /// Stop the current owner without asking (still subject to the safety policy).
    #[arg(short, long)]
    pub yes: bool,
    /// Kill the owner immediately instead of gracefully.
    #[arg(short, long)]
    pub force: bool,
    /// If the port can't be freed safely, use the next free port instead.
    #[arg(long)]
    pub fallback: bool,
    /// Grace period before escalating to SIGKILL.
    #[arg(short, long, default_value = "5s", value_parser = humantime::parse_duration)]
    pub timeout: Duration,
    /// Name of the environment variable to set (default PORT).
    #[arg(long, default_value = "PORT")]
    pub env: String,
    /// The command to run.
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
}

pub(crate) fn parse_port(s: &str) -> Result<u16, String> {
    let s = s.trim().trim_start_matches(':');
    match s.parse::<u16>() {
        Ok(0) | Err(_) => Err(format!("`{s}` is not a valid port (1–65535)")),
        Ok(p) => Ok(p),
    }
}

fn parse_range(s: &str) -> Result<(u16, u16), String> {
    portwise_core::parse_range(s).ok_or_else(|| format!("`{s}` is not a range like 3000-3999"))
}

fn main() -> ExitCode {
    // Behave like a well-mannered Unix filter: `portwise list --json | head` must not print
    // "Broken pipe" errors.
    #[cfg(unix)]
    // SAFETY: restoring the default disposition of SIGPIPE before any threads are spawned.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    style::init(cli.color);
    let docker = !cli.no_docker;
    let result = match cli.command {
        None => {
            if std::io::stdout().is_terminal() && std::io::stdin().is_terminal() {
                tui::run(docker)
            } else {
                commands::list(&ListArgs::default(), docker)
            }
        }
        Some(Command::List(a)) => commands::list(&a, docker),
        Some(Command::Inspect(a)) => commands::inspect(&a, docker),
        Some(Command::Explain(a)) => commands::explain(&a, docker),
        Some(Command::Stop(a)) => commands::stop(&a, docker),
        Some(Command::Kill(mut a)) => {
            a.force = true;
            commands::stop(&a, docker)
        }
        Some(Command::FreePort(a)) => commands::free_port(&a, docker),
        Some(Command::Wait(a)) => commands::wait(&a),
        Some(Command::Run(a)) => commands::run(&a, docker),
        Some(Command::Graph(a)) => graph::run(&a, docker),
        Some(Command::Watch(a)) => watch::run(&a, docker),
        Some(Command::Pin(a)) => state::pin(&a, true),
        Some(Command::Unpin(a)) => state::pin(&a, false),
        Some(Command::Pins { json }) => state::pins(json, docker),
        Some(Command::History(a)) => state::history_cmd(&a),
        Some(Command::Restart(a)) => state::restart(&a, docker),
        Some(Command::Open(a)) => state::open(&a),
        Some(Command::Ssh(a)) => remote::run(&a),
        Some(Command::Tui) => tui::run(docker),
        Some(Command::Mcp) => {
            let stdin = std::io::stdin();
            portwise_mcp::serve(stdin.lock(), std::io::stdout().lock())
                .map(|_| exit::OK)
                .map_err(anyhow::Error::from)
        }
        Some(Command::Completions { shell }) => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "portwise",
                &mut std::io::stdout(),
            );
            Ok(exit::OK)
        }
        Some(Command::Man) => {
            let man = clap_mangen::Man::new(Cli::command());
            man.render(&mut std::io::stdout())
                .map(|_| exit::OK)
                .map_err(anyhow::Error::from)
        }
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
            {
                return ExitCode::from(exit::OK);
            }
            eprintln!("{} {}", style::paint("error:", style::S::BoldRed), e);
            for cause in e.chain().skip(1) {
                eprintln!("  {} {cause}", style::dim("caused by:"));
            }
            ExitCode::from(exit::ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn port_parsing() {
        assert_eq!(parse_port(":3000"), Ok(3000));
        assert!(parse_port("0").is_err());
        assert!(parse_port("99999").is_err());
    }
}
