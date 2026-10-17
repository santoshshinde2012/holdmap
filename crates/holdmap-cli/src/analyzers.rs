//! `holdmap analyzers` and `holdmap insights`: list opt-in analyzers, show exactly what one
//! would get, approve or revoke it, run it, and read its findings (text, JSON or SARIF).
//! Nothing runs without an approval recorded here or in the desktop app.

use crate::commands::confirm;
use crate::style::{self, bold, dim, paint, S};
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use holdmap_analyzers::service::{AnalyzerView, Preview, ServiceError};
use holdmap_analyzers::FileStore;
use holdmap_analyzers::{
    AnalyzerResult, AnalyzerService, AnalyzerState, Finding, FindingState, Insights, Severity,
    Stores,
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
    },
    /// Turn an analyzer off and delete its results.
    Revoke { id: String },
    /// Run an approved analyzer now (or, with --changed, every approved one whose input changed).
    Run {
        #[arg(required_unless_present = "changed")]
        id: Option<String>,
        #[arg(long, conflicts_with = "id")]
        changed: bool,
    },
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
    /// Exit with status 1 when an open finding at or above this severity exists.
    #[arg(long, value_enum)]
    pub fail_on: Option<MinSeverity>,
}

#[derive(Subcommand, Debug)]
pub enum InsightAction {
    /// Mark a finding reviewed; it stays hidden while it is unchanged.
    Review { fingerprint: String },
    /// Hide a finding for some days.
    Snooze {
        fingerprint: String,
        #[arg(long, default_value_t = 7)]
        days: u64,
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
        AnalyzerAction::Approve { id, yes } => {
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
            let view = svc.approve(id)?;
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
                    style::terminal_text(&render_result(&result, Severity::Info, false))
                );
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

pub fn run_insights(a: &InsightsArgs) -> Result<u8> {
    let svc = service();
    if let Some(action) = &a.action {
        let (fp, state, snooze) = match action {
            InsightAction::Review { fingerprint } => (fingerprint, FindingState::Reviewed, None),
            InsightAction::Snooze { fingerprint, days } => (
                fingerprint,
                FindingState::Snoozed,
                Some(days.saturating_mul(86_400_000)),
            ),
            InsightAction::Reopen { fingerprint } => (fingerprint, FindingState::Open, None),
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
        svc.review(fp, state, snooze)?;
        println!("{} Saved.", style::ok_mark());
        return Ok(crate::exit::OK);
    }
    let insights = svc.insights();
    let min = Severity::from(a.min);
    let format = if a.json {
        InsightFormat::Json
    } else {
        a.format
    };
    match format {
        InsightFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&filtered(&insights, min, a.all))?
        ),
        InsightFormat::Sarif => println!(
            "{}",
            serde_json::to_string_pretty(&sarif(&filtered(&insights, min, a.all)))?
        ),
        InsightFormat::Text => {
            if insights.results.is_empty() {
                println!(
                    "No results yet. Turn an analyzer on with: holdmap analyzers approve <id>"
                );
            }
            for r in &insights.results {
                print!("{}", style::terminal_text(&render_result(r, min, a.all)));
            }
        }
    }
    let failing = a.fail_on.map(Severity::from).is_some_and(|fail| {
        insights
            .results
            .iter()
            .any(|r| r.open_at_least(fail).next().is_some())
    });
    Ok(if failing {
        crate::exit::BUSY
    } else {
        crate::exit::OK
    })
}

fn filtered(insights: &Insights, min: Severity, all: bool) -> Insights {
    let mut out = insights.clone();
    for r in &mut out.results {
        r.findings
            .retain(|f| f.severity >= min && (all || f.state == FindingState::Open));
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
        if a.state == AnalyzerState::NotInstalled {
            if let Some(install) = &a.install {
                out.push_str(&format!("  install: {}\n", dim(install)));
            }
        }
    }
    out.push_str(&dim(
        "Analyzers are off until you approve one: holdmap analyzers show <id>\n",
    ));
    out
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

fn render_result(r: &AnalyzerResult, min: Severity, all: bool) -> String {
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
    let shown: Vec<&Finding> = r
        .findings
        .iter()
        .filter(|f| f.severity >= min && (all || f.state == FindingState::Open))
        .collect();
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
        out.push_str(&format!(
            "  {:<8} {}  {}{}\n",
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
                dim(format!("{:?}", f.state).to_lowercase())
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
                },
            ],
            details: vec![],
            notes: vec![],
            input_signature: None,
        }
    }

    #[test]
    fn text_shows_open_findings_with_location_fix_and_id() {
        style::init(style::ColorChoice::Never);
        let text = render_result(&result(), Severity::Info, false);
        assert!(text.contains("critical Credential file sent over the network  deploy-helper scripts/deploy.sh:14"), "{text}");
        assert!(text.contains("fix: Remove the upload."));
        assert!(
            !text.contains("Missing description"),
            "reviewed findings are hidden by default"
        );
        assert!(render_result(&result(), Severity::Info, true).contains("Missing description"));
        assert!(!render_result(&result(), Severity::High, true).contains("Missing description"));
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
            filtered(&insights, Severity::Info, false).results[0]
                .findings
                .len(),
            1
        );
        assert_eq!(
            filtered(&insights, Severity::Info, true).results[0]
                .findings
                .len(),
            2
        );
    }
}
