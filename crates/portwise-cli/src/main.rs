//! portwise — see which ports are in use, *why*, and stop the right thing safely.

mod agents;
mod commands;
mod graph;
mod remote;
mod render;
mod shell;
mod stack;
mod state;
mod style;
mod tui;
mod watch;

#[cfg(test)]
mod cli_docs;

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
    after_help = "EXAMPLES:\n  portwise                      Open the interactive TUI\n  portwise list --dev           Only dev servers\n  portwise explain 3000         Why is 3000 busy?\n  portwise stop 3000            Gracefully stop whatever holds 3000\n  portwise stop 3000 --dry-run  Show the plan only\n  portwise run -p 3000 -- npm run dev\n  portwise free-port --near 3000\n  portwise wait 5432 --timeout 30s\n  portwise graph                Which services depend on which\n  portwise agents               Agents & tools: folders, access, ports\n  portwise agents --stop-ports  Stop the ports they started\n  portwise stop --cluster shop  Stop a whole stack, dependents first\n  portwise up                   Start the services in .portwise.toml\n  eval \"$(portwise init zsh)\"   Explain port-in-use errors in your shell\n  portwise watch                Stream new/closed/conflicting listeners\n\nEXIT CODES: 0 ok · 1 busy/not found/timeout · 2 error · 3 blocked by safety policy · 4 needs elevation"
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
    /// Show the AI coding agents running here: their folders, access, ports and connections.
    Agents(agents::AgentsArgs),
    /// Stream port events: new listeners, closed listeners, conflicts.
    Watch(watch::WatchArgs),
    /// Pin a port (favourite): shown first and watched even when free.
    Pin(state::PinArgs),
    /// Remove a pin.
    Unpin(state::UnpinArgs),
    /// List pinned ports and whether they are in use.
    Pins {
        /// Machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Ports portwise stopped recently, with the command that ran there.
    History(state::HistoryArgs),
    /// Stop what holds a port and start the same command again (or re-run it from history).
    Restart(state::RestartArgs),
    /// Open http://localhost:PORT in the browser.
    Open(state::OpenArgs),
    /// Start a project's services from its .portwise.toml, dependencies first.
    Up(stack::UpArgs),
    /// Stop a project's services (dependents first), through the usual safety checks.
    Down(stack::DownArgs),
    /// Show a project's services: running, stopped or held by something else, with HTTP status.
    Status(stack::StatusArgs),
    /// Print a shell hook that explains "port already in use" errors, or write a .portwise.toml.
    Init(InitArgs),
    /// Inspect another machine's ports over SSH (read-only, nothing to install remotely).
    Ssh(remote::SshArgs),
    /// Open the interactive terminal UI.
    Tui,
    /// Run the MCP (Model Context Protocol) server on stdio for AI coding assistants.
    Mcp,
    /// Explain busy ports a failed command wanted (called by the `portwise init` shell hook).
    #[command(hide = true)]
    Hint {
        /// Exit status of the failed command.
        #[arg(long)]
        exit_code: Option<i32>,
        /// The command line that failed.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Print the man page (roff), or write one page per command with --out-dir.
    Man {
        /// Write `portwise.1` and `portwise-<command>.1` into this directory instead of printing.
        #[arg(long, value_name = "DIR")]
        out_dir: Option<std::path::PathBuf>,
    },
}

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Print the integration script for this shell. Without a shell, write a starter
    /// .portwise.toml from the dev servers running under the current directory.
    #[arg(value_enum)]
    pub shell: Option<shell::InitShell>,
    /// Overwrite an existing .portwise.toml.
    #[arg(long)]
    pub force: bool,
    /// Print the project file instead of writing it.
    #[arg(long)]
    pub print: bool,
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
    /// Ask each TCP listener for its HTTP status and page title (a short `GET /`).
    #[arg(long)]
    pub http: bool,
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
    /// Stop every dev server you own (what `portwise list --dev --mine` shows).
    #[arg(long)]
    pub all_dev: bool,
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
    /// Machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct WaitArgs {
    /// Port number (e.g. 3000 or :3000).
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
    /// Machine-readable JSON output.
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

/// Write `portwise.1` plus one `portwise-<command>.1` page per subcommand into `dir`.
fn write_man_pages(dir: &std::path::Path) -> anyhow::Result<u8> {
    std::fs::create_dir_all(dir)?;
    let cmd = Cli::command();
    clap_mangen::generate_to(cmd, dir)?;
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "1") {
            println!("{}", path.display());
        }
    }
    Ok(exit::OK)
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
        Some(Command::Agents(a)) => agents::run(&a, docker),
        Some(Command::Watch(a)) => watch::run(&a, docker),
        Some(Command::Pin(a)) => state::pin(&a, true),
        Some(Command::Unpin(a)) => state::pin(
            &state::PinArgs {
                port: a.port,
                label: None,
            },
            false,
        ),
        Some(Command::Pins { json }) => state::pins(json, docker),
        Some(Command::History(a)) => state::history_cmd(&a),
        Some(Command::Restart(a)) => state::restart(&a, docker),
        Some(Command::Open(a)) => state::open(&a),
        Some(Command::Up(a)) => stack::up(&a, docker),
        Some(Command::Down(a)) => stack::down(&a, docker),
        Some(Command::Status(a)) => stack::status(&a, docker),
        Some(Command::Init(a)) => match a.shell {
            Some(sh) => {
                print!("{}", shell::script(sh));
                Ok(exit::OK)
            }
            None => stack::init_project(a.force, a.print, docker),
        },
        Some(Command::Hint { command, .. }) => shell::hint(&command, docker),
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
        Some(Command::Man { out_dir: None }) => {
            let man = clap_mangen::Man::new(Cli::command());
            man.render(&mut std::io::stdout())
                .map(|_| exit::OK)
                .map_err(anyhow::Error::from)
        }
        Some(Command::Man { out_dir: Some(dir) }) => write_man_pages(&dir),
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
