//! `holdmap analyzers` and `holdmap insights`: list opt-in analyzers, show exactly what one
//! would get, approve or revoke it, run it, and read its findings (text, JSON or SARIF).
//! Nothing runs without an approval recorded here or in the desktop app.

use crate::commands::confirm;
use crate::style::{self, bold, dim, paint, S};
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use holdmap_analyzers::catalog::{CatalogDiff, SignedCatalog};
use holdmap_analyzers::finding::RunRecord;
use holdmap_analyzers::secrets::Secret;
use holdmap_analyzers::service::{AnalyzerView, Preview, ServiceError};
use holdmap_analyzers::FileStore;
use holdmap_analyzers::{
    AnalyzerResult, AnalyzerService, AnalyzerState, Finding, FindingState, Insights, SandboxLevel,
    Severity, Stores,
};
use holdmap_core::agents::AgentsReport;
use holdmap_core::util::printable;
use holdmap_core::{Engine, ScanOptions};
use serde_json::{json, Value};

#[derive(Args, Debug)]
pub struct AnalyzersArgs {
    #[command(subcommand)]
    pub action: Option<AnalyzerAction>,
    /// Machine-readable JSON output.
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum AnalyzerAction {
    /// List analyzers and whether each is on (the default).
    List,
    /// Show what an analyzer would get: folders or packages, command, network and environment.
    Show { id: String },
    /// Review an analyzer's permissions and turn it on.
    Approve {
        id: String,
        /// Don't ask; approve what `holdmap analyzers show ID` prints.
        #[arg(long)]
        yes: bool,
        /// Allow an MCP analyzer on another machine (it gets metadata only).
        #[arg(long)]
        allow_remote: bool,
    },
    /// Turn an analyzer off and delete its results.
    Revoke { id: String },
    /// Store or delete an API key an analyzer declares, in the OS keychain.
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Register your own analyzer from a manifest file, a preset (--preset), or any MCP server
    /// with --mcp (from a command, a URL or one of your agents' servers). It stays off until you
    /// approve it.
    Add {
        #[arg(required_unless_present_any = ["mcp", "preset"])]
        file: Option<std::path::PathBuf>,
        /// A ready-made manifest for a well-known local analyzer (see `holdmap analyzers presets`).
        #[arg(long, conflicts_with_all = ["file", "mcp"])]
        preset: Option<String>,
        #[command(flatten)]
        mcp: crate::analyzers_mcp::McpAddArgs,
    },
    /// Ready-made manifests for well-known local analyzers; add one with `add --preset ID`.
    Presets,
    /// Your agents' configured MCP servers that can be added as analyzers.
    Servers,
    /// An MCP analyzer's tools: descriptions, warnings, suggested inputs and pin. Lists them
    /// from the server (after you confirm) the first time or with --refresh.
    Tools {
        id: String,
        /// Ask the server again.
        #[arg(long)]
        refresh: bool,
        /// Don't ask before starting or contacting the server.
        #[arg(long)]
        yes: bool,
    },
    /// Choose an MCP analyzer's tools and map their inputs and output.
    Map(crate::analyzers_mcp::MapArgs),
    /// Remove an analyzer you registered, with its approval, results and stored keys.
    Remove { id: String },
    /// The signed analyzer catalog: show it, get a newer one, review the diff, accept or reject.
    Catalog {
        #[command(subcommand)]
        action: Option<CatalogAction>,
    },
    /// Allow (on) or stop (off) automatic runs, when items change, for an approved analyzer
    /// that uses the network. Analyzers without network access run automatically once on.
    Auto {
        id: String,
        #[arg(value_enum)]
        setting: Toggle,
    },
    /// Run an approved analyzer now (or, with --changed, every approved one whose input changed).
    Run {
        #[arg(required_unless_present = "changed")]
        id: Option<String>,
        #[arg(long, conflicts_with = "id")]
        changed: bool,
    },
    /// An analyzer's past runs, newest first: status, subjects, findings, new and fixed.
    History { id: String },
}

#[derive(Subcommand, Debug)]
pub enum CatalogAction {
    /// Which catalog is in use and whether an update waits for review (the default).
    Status,
    /// Download the newest signed catalog from the holdmap repository and stage it for review.
    Fetch,
    /// Stage a catalog file (its signature is read from FILE.minisig) for review.
    Import { file: std::path::PathBuf },
    /// Apply the staged update. Analyzers whose manifest changed ask for approval again.
    Accept,
    /// Discard the staged update.
    Reject,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    On,
    Off,
}

#[derive(Subcommand, Debug)]
pub enum KeyAction {
    /// Store a key: paste it when asked (not echoed), or pipe it on standard input.
    Set { id: String, name: String },
    /// Delete a stored key.
    Delete { id: String, name: String },
}

#[derive(Args, Debug)]
pub struct InsightsArgs {
    #[command(subcommand)]
    pub action: Option<InsightAction>,
    /// Output format.
    #[arg(long, value_enum, default_value = "text", global = true)]
    pub format: InsightFormat,
    /// Shorthand for --format json.
    #[arg(long, global = true)]
    pub json: bool,
    /// Only findings at or above this severity.
    #[arg(long, value_enum, default_value = "info")]
    pub min: MinSeverity,
    /// Include reviewed and snoozed findings.
    #[arg(long)]
    pub all: bool,
    /// Only findings new since each analyzer's previous run (with --fail-on: fail on new
    /// findings only, so existing ones act as a baseline).
    #[arg(long)]
    pub new: bool,
    /// Exit with status 1 when an open finding at or above this severity exists (among the
    /// shown findings when --new is set).
    #[arg(long, value_enum)]
    pub fail_on: Option<MinSeverity>,
}

#[derive(Subcommand, Debug)]
pub enum InsightAction {
    /// Mark a finding reviewed; it stays hidden while it is unchanged.
    Review {
        fingerprint: String,
        /// Why it is acceptable (kept with the review, at most 200 characters).
        #[arg(long)]
        reason: Option<String>,
    },
    /// Hide a finding for some days.
    Snooze {
        fingerprint: String,
        #[arg(long, default_value_t = 7)]
        days: u64,
        /// Why it can wait (kept with the snooze, at most 200 characters).
        #[arg(long)]
        reason: Option<String>,
    },
    /// Show a reviewed or snoozed finding again.
    Reopen { fingerprint: String },
}

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum InsightFormat {
    Text,
    Json,
    Sarif,
}

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum MinSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl From<MinSeverity> for Severity {
    fn from(m: MinSeverity) -> Self {
        match m {
            MinSeverity::Info => Severity::Info,
            MinSeverity::Low => Severity::Low,
            MinSeverity::Medium => Severity::Medium,
            MinSeverity::High => Severity::High,
            MinSeverity::Critical => Severity::Critical,
        }
    }
}

pub(crate) fn service() -> AnalyzerService {
    AnalyzerService::new(Stores::of(FileStore::open_default()))
}

fn inventory(docker: bool) -> Result<AgentsReport> {
    let e = Engine::new(&ScanOptions {
        all_states: false,
        docker,
    })
    .context("failed to scan processes")?;
    Ok(e.agents_with_capabilities())
}

pub fn run_analyzers(a: &AnalyzersArgs, docker: bool) -> Result<u8> {
    let svc = service();
    match a.action.as_ref().unwrap_or(&AnalyzerAction::List) {
        AnalyzerAction::List => {
            let list = svc.list();
            if a.json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                print!("{}", style::terminal_text(&render_list(&list)));
            }
        }
        AnalyzerAction::Show { id } => {
            let preview = svc.preview(id, &inventory(docker)?)?;
            if a.json {
                println!("{}", serde_json::to_string_pretty(&preview)?);
            } else {
                print!("{}", style::terminal_text(&render_preview(&preview)));
            }
        }
        AnalyzerAction::Approve {
            id,
            yes,
            allow_remote,
        } => {
            let preview = svc.preview(id, &inventory(docker)?)?;
            if preview.analyzer.state == AnalyzerState::NotInstalled {
                bail!(
                    "{} is not installed. {}",
                    preview.analyzer.name,
                    preview
                        .analyzer
                        .install
                        .as_deref()
                        .map(|i| format!("Install it with: {i}"))
                        .unwrap_or_default()
                );
            }
            if !a.json {
                eprint!("{}", style::terminal_text(&render_preview(&preview)));
            }
            if !yes
                && !confirm(&format!(
                    "Turn on {} with these permissions?",
                    preview.analyzer.name
                ))?
            {
                eprintln!("Not approved; nothing changed.");
                return Ok(crate::exit::BUSY);
            }
            let view = match svc.approve_with(id, *allow_remote) {
                Ok(v) => v,
                Err(e @ (ServiceError::Run(_) | ServiceError::Manifest(_))) => {
                    eprintln!("{} {e}", style::err_mark());
                    return Ok(crate::exit::BLOCKED);
                }
                Err(e) => return Err(e.into()),
            };
            if a.json {
                println!("{}", serde_json::to_string_pretty(&view)?);
            } else {
                println!(
                    "{} {} is on. Run it with: holdmap analyzers run {}",
                    style::ok_mark(),
                    view.name,
                    view.id
                );
            }
        }
        AnalyzerAction::Revoke { id } => {
            let existed = svc.revoke(id)?;
            if a.json {
                println!("{}", json!({ "revoked": existed }));
            } else if existed {
                println!("{} Turned off and its results deleted.", style::ok_mark());
            } else {
                println!("It was not on.");
            }
        }
        AnalyzerAction::Key { action } => return run_key(&svc, action, a.json),
        AnalyzerAction::Presets => {
            let presets = svc.presets();
            if a.json {
                println!("{}", serde_json::to_string_pretty(&presets)?);
            } else {
                for p in presets {
                    let net = if p.network.is_empty() {
                        "no network".to_string()
                    } else {
                        format!("contacts {}", p.network.join(", "))
                    };
                    let found = if p.installed {
                        "installed".to_string()
                    } else {
                        format!(
                            "not installed · install: {}",
                            p.install.as_deref().unwrap_or("see the project page")
                        )
                    };
                    println!(
                        "{}  {}  {}{}\n  {}\n  {} · {} · {}\n  {}\n",
                        p.name,
                        p.id,
                        p.publisher,
                        if p.added { "  (added)" } else { "" },
                        p.summary,
                        p.license,
                        net,
                        p.homepage,
                        found
                    );
                }
                println!("Add one with: holdmap analyzers add --preset ID (it stays off until you approve it)");
            }
        }
        AnalyzerAction::Add {
            preset: Some(id), ..
        } => return registered(svc.add_preset(id), a.json),
        AnalyzerAction::Add {
            file: None, mcp, ..
        } => return crate::analyzers_mcp::add(&svc, mcp, || inventory(docker), a.json),
        AnalyzerAction::Servers => {
            return crate::analyzers_mcp::servers(&svc, &inventory(docker)?, a.json)
        }
        AnalyzerAction::Tools { id, refresh, yes } => {
            return crate::analyzers_mcp::tools(&svc, id, *refresh, *yes, a.json)
        }
        AnalyzerAction::Map(m) => return crate::analyzers_mcp::map(&svc, m, a.json),
        AnalyzerAction::Add {
            file: Some(file), ..
        } => {
            let text = std::fs::read_to_string(file)
                .with_context(|| format!("couldn't read {}", file.display()))?;
            return registered(svc.add_manifest(&text), a.json);
        }
        AnalyzerAction::Remove { id } => match svc.remove_manifest(id) {
            Ok(existed) => {
                if a.json {
                    println!("{}", json!({ "removed": existed }));
                } else if existed {
                    println!("{} Removed {id}.", style::ok_mark());
                } else {
                    println!("{id} was not registered.");
                }
            }
            Err(ServiceError::Manifest(e)) => {
                eprintln!("{} {e}", style::err_mark());
                return Ok(crate::exit::BLOCKED);
            }
            Err(e) => return Err(e.into()),
        },
        AnalyzerAction::Catalog { action } => {
            return run_catalog(
                &svc,
                action.as_ref().unwrap_or(&CatalogAction::Status),
                a.json,
            )
        }
        AnalyzerAction::Auto { id, setting } => {
            let view = match svc.set_background(id, *setting == Toggle::On) {
                Ok(v) => v,
                Err(ServiceError::Run(e)) => {
                    eprintln!("{} {e}", style::err_mark());
                    return Ok(crate::exit::BLOCKED);
                }
                Err(e) => return Err(e.into()),
            };
            if a.json {
                println!("{}", serde_json::to_string_pretty(&view)?);
            } else if !view.permissions.network.is_empty() && *setting == Toggle::On {
                println!(
                    "{} {} will also run when the analysed items change, using the network.",
                    style::ok_mark(),
                    view.name
                );
            } else if view.background {
                println!(
                    "{} {} doesn't use the network; it always runs when items change.",
                    style::ok_mark(),
                    view.name
                );
            } else {
                println!(
                    "{} {} runs only when you run it.",
                    style::ok_mark(),
                    view.name
                );
            }
        }
        AnalyzerAction::Run { id: Some(id), .. } => {
            let result = match svc.run(id, &inventory(docker)?) {
                Ok(r) => r,
                Err(ServiceError::Run(e)) => {
                    eprintln!("{} {e}", style::err_mark());
                    return Ok(crate::exit::BLOCKED);
                }
                Err(e) => return Err(e.into()),
            };
            if a.json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print!(
                    "{}",
                    style::terminal_text(&render_result(
                        &result,
                        &View {
                            min: Severity::Info,
                            all: false,
                            new_only: false
                        }
                    ))
                );
            }
        }
        AnalyzerAction::History { id } => {
            let runs = svc.history(id)?;
            if a.json {
                println!("{}", serde_json::to_string_pretty(&runs)?);
            } else {
                print!("{}", style::terminal_text(&render_history(&runs)));
            }
        }
        AnalyzerAction::Run { id: None, .. } => {
            let ran = svc.run_changed(&inventory(docker)?);
            if a.json {
                println!("{}", json!({ "ran": ran }));
            } else if ran.is_empty() {
                println!("Nothing changed since the last runs (or no analyzer is on).");
            } else {
                println!("{} Ran {}.", style::ok_mark(), ran.join(", "));
            }
        }
    }
    Ok(crate::exit::OK)
}

fn run_catalog(svc: &AnalyzerService, action: &CatalogAction, json: bool) -> Result<u8> {
    let staged = |r: Result<CatalogDiff, ServiceError>| -> Result<u8> {
        match r {
            Ok(diff) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&diff)?);
                } else {
                    print!("{}", style::terminal_text(&render_diff(&diff)));
                    println!("Apply it with: holdmap analyzers catalog accept");
                }
                Ok(crate::exit::OK)
            }
            Err(ServiceError::Catalog(e)) => {
                eprintln!("{} {e}", style::err_mark());
                Ok(crate::exit::BLOCKED)
            }
            Err(e) => Err(e.into()),
        }
    };
    match action {
        CatalogAction::Status => {
            let status = svc.catalog_status();
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!(
                    "Catalog version {} ({}), {}.",
                    status.version,
                    status.published,
                    if status.accepted {
                        "an update you accepted"
                    } else {
                        "built into holdmap"
                    }
                );
                if let Some(diff) = &status.pending {
                    print!("{}", style::terminal_text(&render_diff(diff)));
                    println!("Apply it with: holdmap analyzers catalog accept (or reject)");
                }
            }
            Ok(crate::exit::OK)
        }
        CatalogAction::Fetch => staged(svc.fetch_catalog()),
        CatalogAction::Import { file } => {
            let mut sig = file.clone().into_os_string();
            sig.push(".minisig");
            let offered = SignedCatalog {
                json: std::fs::read_to_string(file)
                    .with_context(|| format!("couldn't read {}", file.display()))?,
                signature: std::fs::read_to_string(&sig).with_context(|| {
                    format!("couldn't read the signature {}", sig.to_string_lossy())
                })?,
            };
            staged(svc.stage_catalog(&offered))
        }
        CatalogAction::Accept => match svc.accept_catalog() {
            Ok(status) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&status)?);
                } else {
                    println!(
                        "{} Catalog version {} is in use. Analyzers that changed ask for approval again.",
                        style::ok_mark(),
                        status.version
                    );
                }
                Ok(crate::exit::OK)
            }
            Err(ServiceError::Catalog(e)) => {
                eprintln!("{} {e}", style::err_mark());
                Ok(crate::exit::BLOCKED)
            }
            Err(e) => Err(e.into()),
        },
        CatalogAction::Reject => {
            svc.reject_catalog()?;
            if json {
                println!("{}", json!({ "rejected": true }));
            } else {
                println!("Discarded the staged catalog update.");
            }
            Ok(crate::exit::OK)
        }
    }
}

fn render_diff(d: &CatalogDiff) -> String {
    let mut out = format!(
        "{} Signed catalog update: version {} → {} ({})\n",
        style::warn_mark(),
        d.from_version,
        d.to_version,
        d.published
    );
    if d.is_empty() {
        out.push_str("  No analyzer changes.\n");
    }
    for a in &d.added {
        out.push_str(&format!("  + {a}\n"));
    }
    for r in &d.removed {
        out.push_str(&format!("  - {r}\n"));
    }
    for c in &d.changed {
        out.push_str(&format!("  ~ {}\n", bold(&c.id)));
        for f in &c.fields {
            out.push_str(&format!(
                "      {}: {} → {}\n",
                f.field,
                if f.before.is_empty() {
                    dim("(none)")
                } else {
                    f.before.clone()
                },
                if f.after.is_empty() {
                    dim("(none)")
                } else {
                    f.after.clone()
                }
            ));
        }
    }
    out
}

/// Report a newly registered analyzer, or why its manifest was refused.
fn registered(added: Result<AnalyzerView, ServiceError>, json: bool) -> Result<u8> {
    let view = match added {
        Ok(v) => v,
        Err(ServiceError::Manifest(e)) => {
            eprintln!("{} {e}", style::err_mark());
            return Ok(crate::exit::BLOCKED);
        }
        Err(e) => return Err(e.into()),
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else if view.state == AnalyzerState::Approved {
        // Re-adding the exact manifest that was approved keeps the approval pin.
        println!(
            "{} {} ({}) is already registered and approved; its manifest is unchanged.",
            style::ok_mark(),
            view.name,
            view.id
        );
    } else {
        println!(
            "{} Registered {} ({}). It is off: review it with `holdmap analyzers show {}`, then `holdmap analyzers approve {}`.",
            style::ok_mark(),
            view.name,
            view.id,
            view.id,
            view.id
        );
    }
    Ok(crate::exit::OK)
}

fn run_key(svc: &AnalyzerService, action: &KeyAction, json: bool) -> Result<u8> {
    match action {
        KeyAction::Set { id, name } => {
            let view = svc.get(id)?;
            if !view.secrets.iter().any(|s| &s.name == name) {
                bail!(
                    "{} doesn't declare a key called {name:?}. It declares: {}",
                    view.name,
                    if view.secrets.is_empty() {
                        "none".to_string()
                    } else {
                        view.secrets
                            .iter()
                            .map(|s| s.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                );
            }
            let value = read_secret(&format!("{name} for {}: ", view.name))?;
            svc.set_secret(id, name, &value)?;
            if json {
                println!("{}", json!({ "stored": true, "name": name }));
            } else {
                println!(
                    "{} Stored {name} in your keychain. Only {} gets it, when it runs.",
                    style::ok_mark(),
                    view.name
                );
            }
        }
        KeyAction::Delete { id, name } => {
            let existed = svc.delete_secret(id, name)?;
            if json {
                println!("{}", json!({ "deleted": existed, "name": name }));
            } else if existed {
                println!("{} Deleted {name} from your keychain.", style::ok_mark());
            } else {
                println!("{name} was not stored.");
            }
        }
    }
    Ok(crate::exit::OK)
}

/// Read one line without echoing it on a terminal, or from a pipe.
fn read_secret(prompt: &str) -> Result<Secret> {
    use std::io::{BufRead, IsTerminal};
    let line = if std::io::stdin().is_terminal() {
        rpassword::prompt_password(prompt).context("couldn't read the key")?
    } else {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .context("couldn't read the key from standard input")?;
        line
    };
    Ok(Secret::new(line.trim_end_matches(['\r', '\n'])))
}

pub fn run_insights(a: &InsightsArgs) -> Result<u8> {
    let svc = service();
    if let Some(action) = &a.action {
        let (fp, state, snooze, reason) = match action {
            InsightAction::Review {
                fingerprint,
                reason,
            } => (fingerprint, FindingState::Reviewed, None, reason),
            InsightAction::Snooze {
                fingerprint,
                days,
                reason,
            } => (
                fingerprint,
                FindingState::Snoozed,
                Some(days.saturating_mul(86_400_000)),
                reason,
            ),
            InsightAction::Reopen { fingerprint } => (fingerprint, FindingState::Open, None, &None),
        };
        if !svc
            .insights()
            .results
            .iter()
            .flat_map(|r| &r.findings)
            .any(|f| &f.fingerprint == fp)
        {
            bail!("no current finding has fingerprint {fp:?}");
        }
        svc.review_with_reason(fp, state, snooze, reason.as_deref())?;
        println!("{} Saved.", style::ok_mark());
        return Ok(crate::exit::OK);
    }
    let insights = svc.insights();
    let view = View {
        min: Severity::from(a.min),
        all: a.all,
        new_only: a.new,
    };
    let format = if a.json {
        InsightFormat::Json
    } else {
        a.format
    };
    match format {
        InsightFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&filtered(&insights, &view))?
        ),
        InsightFormat::Sarif => println!(
            "{}",
            serde_json::to_string_pretty(&sarif(&filtered(&insights, &view)))?
        ),
        InsightFormat::Text => {
            if insights.results.is_empty() {
                println!(
                    "No results yet. Turn an analyzer on with: holdmap analyzers approve <id>"
                );
            }
            for r in &insights.results {
                print!("{}", style::terminal_text(&render_result(r, &view)));
            }
        }
    }
    let failing = a.fail_on.map(Severity::from).is_some_and(|fail| {
        insights.results.iter().any(|r| {
            r.open_at_least(fail)
                .any(|f| !view.new_only || is_new(r, &f.fingerprint))
        })
    });
    Ok(if failing {
        crate::exit::BUSY
    } else {
        crate::exit::OK
    })
}

/// Which findings a listing shows.
struct View {
    min: Severity,
    all: bool,
    new_only: bool,
}

impl View {
    fn shows(&self, r: &AnalyzerResult, f: &Finding) -> bool {
        f.severity >= self.min
            && (self.all || f.state == FindingState::Open)
            && (!self.new_only || is_new(r, &f.fingerprint))
    }
}

/// New since the analyzer's previous run. A first run has nothing to compare with, so every
/// finding counts as new.
fn is_new(r: &AnalyzerResult, fingerprint: &str) -> bool {
    r.changes.as_ref().is_none_or(|c| c.is_new(fingerprint))
}

fn filtered(insights: &Insights, view: &View) -> Insights {
    let mut out = insights.clone();
    for r in &mut out.results {
        let keep: Vec<Finding> = r
            .findings
            .iter()
            .filter(|f| view.shows(r, f))
            .cloned()
            .collect();
        r.findings = keep;
    }
    out
}

fn state_label(state: AnalyzerState) -> String {
    match state {
        AnalyzerState::Approved => paint("on", S::Green),
        AnalyzerState::NotApproved => dim("off"),
        AnalyzerState::NotInstalled => dim("not installed"),
        AnalyzerState::Changed => paint("changed: approve again", S::Yellow),
    }
}

fn sandbox_label(level: SandboxLevel) -> &'static str {
    match level {
        SandboxLevel::Basic => "basic (no OS sandbox)",
        SandboxLevel::MacosSeatbelt => "macOS sandbox-exec (best effort)",
        SandboxLevel::LinuxBubblewrap => "Linux bubblewrap",
        SandboxLevel::LinuxLandlock => "Linux Landlock",
        SandboxLevel::WindowsJob => "Windows Job Object",
    }
}

fn render_list(list: &[AnalyzerView]) -> String {
    let mut out = String::new();
    for a in list {
        out.push_str(&format!(
            "{}  {}  {}\n",
            bold(&a.name),
            dim(&a.id),
            state_label(a.state)
        ));
        out.push_str(&format!("  {}\n", a.summary));
        for change in &a.changes {
            out.push_str(&format!("  {} {change}\n", style::warn_mark()));
        }
        if let Some(run) = &a.last_run {
            out.push_str(&format!(
                "  last run: {} subject(s), {} open finding(s), {}\n",
                run.subjects,
                run.open.total(),
                dim(humantime::format_rfc3339_seconds(
                    std::time::UNIX_EPOCH + std::time::Duration::from_millis(run.run_at_ms)
                )
                .to_string())
            ));
        }
        if a.state == AnalyzerState::Approved {
            out.push_str(&format!(
                "  runs: {}\n",
                if a.background {
                    "on demand and when items change".to_string()
                } else {
                    dim(format!(
                        "on demand only (uses the network; holdmap analyzers auto {} on)",
                        a.id
                    ))
                }
            ));
            out.push_str(&format!("  sandbox: {}\n", dim(sandbox_label(a.sandbox))));
        }
        for key in &a.secrets {
            out.push_str(&format!(
                "  key {}: {}\n",
                key.name,
                if key.stored {
                    "stored in your keychain".to_string()
                } else {
                    dim(format!(
                        "not stored (holdmap analyzers key set {} {})",
                        a.id, key.name
                    ))
                }
            ));
        }
        if a.state == AnalyzerState::NotInstalled {
            if let Some(install) = &a.install {
                out.push_str(&format!("  install: {}\n", dim(install)));
            }
        }
    }
    out.push_str(&dim(list_footer(list)));
    out
}

/// The hint under the list: the "all off" line only while nothing is approved.
fn list_footer(list: &[AnalyzerView]) -> &'static str {
    if list.iter().any(|a| a.state == AnalyzerState::Approved) {
        "Each analyzer stays off until you approve it: holdmap analyzers show <id>\n"
    } else {
        "Analyzers are off until you approve one: holdmap analyzers show <id>\n"
    }
}

fn render_preview(p: &Preview) -> String {
    let a = &p.analyzer;
    let mut out = format!(
        "{} by {} ({})\n{}\n\n",
        bold(&a.name),
        a.publisher,
        a.license,
        a.summary
    );
    for n in &p.notices {
        let mark = if n.starts_with("Warning:") {
            style::warn_mark()
        } else {
            dim("i")
        };
        out.push_str(&format!("{mark} {n}\n"));
    }
    if !p.notices.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("{}\n", bold("It gets")));
    if p.subjects.is_empty() {
        out.push_str("  nothing right now (no matching items found)\n");
    }
    for s in &p.subjects {
        out.push_str(&format!(
            "  {}  {}\n",
            printable(&s.subject.name),
            dim(printable(&s.input))
        ));
    }
    for s in &p.skipped {
        out.push_str(&format!("  {} {}\n", dim("skipped:"), printable(s)));
    }
    if let Some(cmd) = &p.command {
        out.push_str(&format!("{}\n  {}\n", bold("Command"), cmd.join(" ")));
        if let Some(bin) = &a.binary {
            out.push_str(&format!("  program: {}\n", dim(bin)));
        }
    }
    out.push_str(&format!("{}\n", bold("Network")));
    if a.permissions.network.is_empty() {
        out.push_str("  none declared\n");
    } else {
        out.push_str(&format!(
            "  {} (allowed, not limited to these hosts)\n",
            a.permissions.network.join(", ")
        ));
        if let Some(reason) = &a.permissions.network_reason {
            out.push_str(&format!("  {}\n", dim(reason)));
        }
        for s in &a.permissions.sends {
            out.push_str(&format!("  sends: {s}\n"));
        }
    }
    for body in &p.payload {
        out.push_str(&format!("{}\n  {body}\n", bold("Exact request")));
    }
    if !p.projection.is_empty() {
        out.push_str(&format!("{}\n", bold("Fields it gets")));
        for f in &p.projection {
            out.push_str(&format!("  {}\n", printable(f)));
        }
    }
    if !p.tool_calls.is_empty() {
        out.push_str(&format!("{}\n", bold("Exact tool calls")));
        for c in &p.tool_calls {
            out.push_str(&format!(
                "  {}  {}  {}\n",
                bold(printable(&c.tool)),
                dim(printable(&c.subject)),
                printable(&c.arguments.to_string())
            ));
            for problem in &c.problems {
                out.push_str(&format!(
                    "    {} {}\n",
                    style::warn_mark(),
                    printable(problem)
                ));
            }
        }
    }
    if !p.env.is_empty() {
        out.push_str(&format!(
            "{}\n  {} (set by holdmap; none of your variables or secrets are passed)\n",
            bold("Environment"),
            p.env.join(", ")
        ));
    }
    out.push_str(&format!(
        "{}\n  {}\n{}\n  {}\n",
        bold("Sandbox"),
        p.sandbox,
        bold("When"),
        p.schedule
    ));
    out
}

fn sev_paint(s: Severity) -> String {
    let style = match s {
        Severity::Critical | Severity::High => S::BoldRed,
        Severity::Medium => S::Yellow,
        _ => S::Dim,
    };
    paint(s.label(), style)
}

fn render_history(runs: &[RunRecord]) -> String {
    if runs.is_empty() {
        return "No runs yet.\n".into();
    }
    let mut out = String::new();
    for r in runs {
        let total: usize = r.counts.values().sum();
        let change = match (r.new, r.fixed) {
            (Some(n), Some(f)) => format!(", {n} new, {f} fixed"),
            _ => String::new(),
        };
        out.push_str(&format!(
            "  {}  {:<8} {} subject(s), {total} finding(s){change}{}\n",
            dim(holdmap_core::util::local_datetime(r.run_at_ms / 1000)),
            format!("{:?}", r.status).to_lowercase(),
            r.subjects,
            r.duration_ms
                .map(|d| dim(format!("  {:.1}s", d as f64 / 1000.0)))
                .unwrap_or_default()
        ));
    }
    out
}

fn render_result(r: &AnalyzerResult, view: &View) -> String {
    let mut out = format!(
        "{}  {:?}{}  {} subject(s)\n",
        bold(&r.analyzer),
        r.status,
        r.coverage
            .map(|c| format!(", {:.0}% covered", c * 100.0))
            .unwrap_or_default(),
        r.subjects.len()
    )
    .to_lowercase();
    if let Some(c) = &r.changes {
        out.push_str(&format!(
            "  {}\n",
            dim(format!(
                "since the previous run: {} new, {} fixed",
                c.new_total, c.fixed_total
            ))
        ));
    }
    let shown: Vec<&Finding> = r.findings.iter().filter(|f| view.shows(r, f)).collect();
    if shown.is_empty() {
        out.push_str("  no open findings\n");
    }
    for f in shown {
        let subject = r
            .subjects
            .iter()
            .find(|s| s.id == f.subject)
            .map_or(f.subject.as_str(), |s| s.name.as_str());
        let at = match (&f.location.file, f.location.line) {
            (Some(file), Some(line)) => format!(" {file}:{line}"),
            (Some(file), None) => format!(" {file}"),
            _ => String::new(),
        };
        let new = if r.changes.as_ref().is_some_and(|c| c.is_new(&f.fingerprint)) {
            format!("{} ", paint("new", S::Yellow))
        } else {
            String::new()
        };
        out.push_str(&format!(
            "  {:<8} {new}{}  {}{}\n",
            sev_paint(f.severity),
            f.title,
            dim(subject),
            dim(at)
        ));
        if let Some(fix) = &f.fix {
            out.push_str(&format!("           fix: {fix}\n"));
        }
        if f.state != FindingState::Open {
            out.push_str(&format!(
                "           {}\n",
                dim(match &f.note {
                    Some(note) => format!("{:?}: {note}", f.state).to_lowercase(),
                    None => format!("{:?}", f.state).to_lowercase(),
                })
            ));
        }
        out.push_str(&format!(
            "           {}\n",
            dim(format!("id {}", f.fingerprint))
        ));
    }
    for d in &r.details {
        let subject = r
            .subjects
            .iter()
            .find(|s| s.id == d.subject)
            .map_or("", |s| s.name.as_str());
        out.push_str(&format!("  {} {}: {}\n", dim(subject), d.label, d.value));
    }
    for n in &r.notes {
        out.push_str(&format!("  {} {n}\n", dim("note:")));
    }
    out
}

/// SARIF 2.1.0 with one run per analyzer; only normalised fields are included.
pub fn sarif(insights: &Insights) -> Value {
    let runs: Vec<Value> = insights
        .results
        .iter()
        .map(|r| {
            let mut rules: Vec<&str> = r.findings.iter().map(|f| f.rule_id.as_str()).collect();
            rules.sort();
            rules.dedup();
            json!({
                "tool": {"driver": {
                    "name": r.analyzer,
                    "version": r.analyzer_version,
                    "informationUri": "https://github.com/santoshshinde2012/holdmap",
                    "rules": rules.iter().map(|id| json!({"id": id})).collect::<Vec<_>>(),
                }},
                "results": r.findings.iter().map(|f| {
                    let mut loc = json!({"physicalLocation": {"artifactLocation": {"uri": f.location.file.clone().unwrap_or_else(|| ".".into())}}});
                    if let Some(line) = f.location.line {
                        loc["physicalLocation"]["region"] = json!({"startLine": line});
                    }
                    json!({
                        "ruleId": f.rule_id,
                        "level": match f.severity { Severity::Critical | Severity::High => "error", Severity::Medium => "warning", _ => "note" },
                        "message": {"text": f.title},
                        "locations": [loc],
                        "partialFingerprints": {"holdmap/v1": f.fingerprint},
                        "properties": {"severity": f.severity.label(), "subject": f.subject, "state": f.state, "fix": f.fix},
                    })
                }).collect::<Vec<_>>(),
                "invocations": [{"executionSuccessful": r.status != holdmap_analyzers::RunStatus::Failed}],
            })
        })
        .collect();
    json!({"version": "2.1.0", "$schema": "https://json.schemastore.org/sarif-2.1.0.json", "runs": runs})
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_analyzers::{Location, RunStatus, Subject, SubjectKind};

    fn result() -> AnalyzerResult {
        AnalyzerResult {
            analyzer: "nvidia.skillspector".into(),
            analyzer_version: Some("2.13.0".into()),
            run_at_ms: 0,
            status: RunStatus::Complete,
            coverage: Some(1.0),
            subjects: vec![Subject {
                id: "skill:a".into(),
                kind: SubjectKind::Skill,
                name: "deploy-helper".into(),
                agents: vec![],
                dir: None,
                scope: None,
            }],
            findings: vec![
                Finding {
                    fingerprint: "f1".into(),
                    analyzer: "nvidia.skillspector".into(),
                    rule_id: "E2".into(),
                    category: None,
                    severity: Severity::Critical,
                    title: "Credential file sent over the network".into(),
                    subject: "skill:a".into(),
                    location: Location {
                        file: Some("scripts/deploy.sh".into()),
                        line: Some(14),
                    },
                    fix: Some("Remove the upload.".into()),
                    refs: vec![],
                    state: FindingState::Open,
                    note: None,
                },
                Finding {
                    fingerprint: "f2".into(),
                    analyzer: "nvidia.skillspector".into(),
                    rule_id: "Q1".into(),
                    category: None,
                    severity: Severity::Low,
                    title: "Missing description".into(),
                    subject: "skill:a".into(),
                    location: Location::default(),
                    fix: None,
                    refs: vec![],
                    state: FindingState::Reviewed,
                    note: None,
                },
            ],
            details: vec![],
            notes: vec![],
            input_signature: None,

            duration_ms: None,

            tool_calls: Vec::new(),

            changes: None,
        }
    }

    #[test]
    fn text_shows_open_findings_with_location_fix_and_id() {
        style::init(style::ColorChoice::Never);
        let text = render_result(
            &result(),
            &View {
                min: Severity::Info,
                all: false,
                new_only: false,
            },
        );
        assert!(text.contains("critical Credential file sent over the network  deploy-helper scripts/deploy.sh:14"), "{text}");
        assert!(text.contains("fix: Remove the upload."));
        assert!(
            !text.contains("Missing description"),
            "reviewed findings are hidden by default"
        );
        assert!(render_result(
            &result(),
            &View {
                min: Severity::Info,
                all: true,
                new_only: false
            }
        )
        .contains("Missing description"));
        assert!(!render_result(
            &result(),
            &View {
                min: Severity::High,
                all: true,
                new_only: false
            }
        )
        .contains("Missing description"));
    }

    #[test]
    fn sarif_output_is_valid_shape_and_carries_only_normalised_fields() {
        let insights = Insights {
            results: vec![result()],
            open: Default::default(),
        };
        let s = sarif(&insights);
        assert_eq!(s["version"], "2.1.0");
        let r = &s["runs"][0]["results"][0];
        assert_eq!(r["level"], "error");
        assert_eq!(
            r["locations"][0]["physicalLocation"]["region"]["startLine"],
            14
        );
        assert_eq!(r["partialFingerprints"]["holdmap/v1"], "f1");
        assert_eq!(
            s["runs"][0]["tool"]["driver"]["rules"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn filtering_keeps_reviewed_only_with_all() {
        let insights = Insights {
            results: vec![result()],
            open: Default::default(),
        };
        assert_eq!(
            filtered(
                &insights,
                &View {
                    min: Severity::Info,
                    all: false,
                    new_only: false
                }
            )
            .results[0]
                .findings
                .len(),
            1
        );
        assert_eq!(
            filtered(
                &insights,
                &View {
                    min: Severity::Info,
                    all: true,
                    new_only: false
                }
            )
            .results[0]
                .findings
                .len(),
            2
        );
    }

    #[test]
    fn new_only_shows_findings_new_since_the_previous_run() {
        let mut r = result();
        let first = r.findings[0].fingerprint.clone();
        r.changes = Some(holdmap_analyzers::RunChanges {
            new: vec![first.clone()],
            new_total: 1,
            fixed: vec![],
            fixed_total: 2,
            since_ms: 1,
        });
        let view = View {
            min: Severity::Info,
            all: true,
            new_only: true,
        };
        let shown: Vec<&Finding> = r.findings.iter().filter(|f| view.shows(&r, f)).collect();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].fingerprint, first);
        let text = render_result(&r, &view);
        assert!(text.contains("1 new, 2 fixed"), "{text}");
        // A first run has nothing to compare with: everything is new.
        r.changes = None;
        assert!(r.findings.iter().all(|f| view.shows(&r, f)));
    }

    #[test]
    fn history_lists_new_and_fixed_when_known() {
        let mut rec = RunRecord::of(&result());
        assert!(!render_history(std::slice::from_ref(&rec)).contains("new"));
        rec.new = Some(3);
        rec.fixed = Some(1);
        assert!(render_history(&[rec]).contains("3 new, 1 fixed"));
        assert_eq!(render_history(&[]), "No runs yet.\n");
    }
}
