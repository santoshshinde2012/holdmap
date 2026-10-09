//! `holdmap ssh HOST …`: look at another machine's ports over SSH.
//!
//! If `holdmap` is installed on the host, arguments are forwarded to it (all safety checks run
//! there, on the machine that owns the processes). Otherwise holdmap reads `ss` + `ps` over
//! SSH and renders the list or graph locally — read-only.

use crate::render;
use crate::style::{self, dim};
use anyhow::{bail, Result};
use holdmap_core::remote::{scan_remote, LocalShell, RemoteRunner, SshRunner};
use holdmap_core::topology::{exporter, GraphExporter, TreeExporter};
use holdmap_core::{Engine, Filter};
use std::io::IsTerminal;

#[derive(clap::Args, Debug)]
pub struct SshArgs {
    /// `[user@]host` (anything `ssh` accepts, including ~/.ssh/config aliases).
    pub host: String,
    /// What to show: `list` (default), `graph`, or any holdmap command line after `--` to run
    /// a remotely installed holdmap.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
    /// JSON output.
    #[arg(long)]
    pub json: bool,
    /// Never use a remote holdmap; always read ss/ps.
    #[arg(long)]
    pub agentless: bool,
}

fn runner(host: &str) -> Box<dyn RemoteRunner> {
    if host == "local" {
        Box::new(LocalShell)
    } else {
        Box::new(SshRunner::new(host))
    }
}

pub fn run(a: &SshArgs) -> Result<u8> {
    let r = runner(&a.host);
    // `holdmap ssh host list --json`: flags after the subcommand land in `command`.
    let json = a.json || a.command.iter().any(|x| x == "--json");
    let sub = a
        .command
        .iter()
        .find(|x| !x.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("list");
    // Forward to a remote holdmap when present (and for any command we don't render locally).
    if !a.agentless && a.host != "local" {
        let probe = r.run("command -v holdmap >/dev/null 2>&1 && echo yes || echo no");
        if probe.is_ok_and(|p| p.trim() == "yes") {
            let mut args: Vec<String> = if a.command.is_empty() {
                vec!["list".into()]
            } else {
                a.command.clone()
            };
            if json && !args.iter().any(|x| x == "--json") {
                args.push("--json".into());
            }
            let quoted: Vec<String> = args
                .iter()
                .map(|x| format!("'{}'", x.replace('\'', "'\\''")))
                .collect();
            // A remote TTY only when we have one: `-t` without a terminal makes ssh warn, and it
            // turns "\n" into "\r\n", which would corrupt piped or --json output.
            let tty = !json && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
            let status = std::process::Command::new("ssh")
                .args(if tty { &["-t"][..] } else { &[] })
                .args(["-o", "ConnectTimeout=10", &a.host, "holdmap"])
                .args(&quoted)
                .status()?;
            return Ok(status.code().map(|c| c.clamp(0, 255) as u8).unwrap_or(2));
        }
    }
    if !matches!(sub, "list" | "ls" | "graph") {
        bail!(
            "`{sub}` needs holdmap installed on {} (agentless mode supports `list` and `graph`, read-only)",
            a.host
        );
    }
    let scan = scan_remote(r.as_ref(), &a.host)?;
    let e = Engine::from_scan(scan);
    if sub == "graph" {
        let mut g = e.topology();
        g.retain_dev();
        if json {
            println!("{}", exporter("json").unwrap().export(&g));
        } else {
            println!(
                "{} {}",
                style::arrow(),
                dim(format!(
                    "{} via {} (agentless, read-only)",
                    a.host,
                    r.describe()
                ))
            );
            print!("{}", TreeExporter::default().export(&g));
        }
        return Ok(crate::exit::OK);
    }
    let filter = Filter {
        listening_only: true,
        ..Default::default()
    };
    let shown = filter.apply(&e.snapshot().entries);
    if json {
        let mut snap = e.snapshot().clone();
        snap.entries = shown.into_iter().cloned().collect();
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(crate::exit::OK);
    }
    println!(
        "{} {}",
        style::arrow(),
        dim(format!(
            "{} via {} (agentless, read-only)",
            a.host,
            r.describe()
        ))
    );
    print!("{}", render::list_table(&shown, false, false));
    println!("\n{}", render::summary_line(e.snapshot(), &shown));
    Ok(crate::exit::OK)
}
