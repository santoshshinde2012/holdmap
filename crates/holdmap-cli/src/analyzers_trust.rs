//! `holdmap analyzers skills|trust|untrust|risks|schedule`: skill trust, banded risk and the
//! scheduled-scan plan, plus the CSV and Markdown tables `holdmap insights` also uses.
//! File contents never appear: only names, sizes, hashes and times.

use crate::style::{self, bold, dim, paint, S};
use anyhow::Result;
use clap::ValueEnum;
use holdmap_analyzers::risk::{explain, risks, RiskEntry, RiskScope};
use holdmap_analyzers::schedule::{SchedulePlan, ScheduleStore};
use holdmap_analyzers::trust::{SkillTrust, TrustReport, TrustState};
use holdmap_analyzers::{FileStore, Insights, RiskBand};
use holdmap_core::agents::AgentsReport;
use holdmap_core::util::local_datetime;
use std::sync::Arc;

/// Table output for lists.
#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum TableFormat {
    Text,
    Json,
    Csv,
    Markdown,
}

/// Which risks to list.
#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum RiskBy {
    Agent,
    Folder,
    All,
}

pub(crate) fn trust() -> SkillTrust {
    SkillTrust::new(Arc::new(FileStore::open_default()))
}

/// A CSV table. Cells that a spreadsheet would run as a formula are prefixed with `'`.
pub fn csv(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cell = |c: &str| {
        let c = if c.starts_with(['=', '+', '-', '@', '\t', '\r']) {
            format!("'{c}")
        } else {
            c.to_string()
        };
        if c.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", c.replace('"', "\"\""))
        } else {
            c
        }
    };
    let mut out = String::new();
    for row in std::iter::once(headers.iter().map(|h| h.to_string()).collect::<Vec<_>>())
        .chain(rows.iter().cloned())
    {
        out.push_str(&row.iter().map(|c| cell(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

/// A Markdown table; pipes and line breaks in cells are escaped.
pub fn markdown(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cell = |c: &str| c.replace('|', "\\|").replace(['\n', '\r'], " ");
    let mut out = format!(
        "| {} |\n|{}\n",
        headers.join(" | "),
        " --- |".repeat(headers.len())
    );
    for row in rows {
        out.push_str(&format!(
            "| {} |\n",
            row.iter().map(|c| cell(c)).collect::<Vec<_>>().join(" | ")
        ));
    }
    out
}

fn table(format: TableFormat, headers: &[&str], rows: &[Vec<String>]) -> Option<String> {
    match format {
        TableFormat::Csv => Some(csv(headers, rows)),
        TableFormat::Markdown => Some(markdown(headers, rows)),
        _ => None,
    }
}

fn state_text(s: TrustState) -> String {
    match s {
        TrustState::Changed => paint(s.label(), S::Yellow),
        TrustState::New => paint(s.label(), S::Yellow),
        TrustState::Unreadable => dim(s.label()),
        TrustState::Trusted => paint(s.label(), S::Green),
    }
}

fn when(ms: u64) -> String {
    local_datetime(ms / 1000)
}

/// Scan (unless `no_scan`) and list every skill's trust.
pub fn run_skills(report: Option<&AgentsReport>, format: TableFormat) -> Result<u8> {
    let t = trust();
    let rep = match report {
        Some(r) => t.scan(r)?,
        None => t.report(),
    };
    let headers = [
        "skill",
        "state",
        "agents",
        "folder",
        "files",
        "bytes",
        "sha256",
        "first_seen",
        "trusted",
        "reason",
        "added",
        "removed",
        "modified",
    ];
    let rows: Vec<Vec<String>> = rep
        .skills
        .iter()
        .map(|s| {
            let c = s.changes.clone().unwrap_or_default();
            vec![
                s.subject.name.clone(),
                format!("{:?}", s.state).to_lowercase(),
                s.subject.agents.join(" "),
                s.subject.dir.clone().unwrap_or_default(),
                s.file_count.to_string(),
                s.total_bytes.to_string(),
                s.sha256.clone().unwrap_or_default(),
                when(s.first_seen_ms),
                s.trusted_ms.map(when).unwrap_or_default(),
                s.reason.clone().unwrap_or_default(),
                c.added_total.to_string(),
                c.removed_total.to_string(),
                c.modified_total.to_string(),
            ]
        })
        .collect();
    if format == TableFormat::Json {
        println!("{}", serde_json::to_string_pretty(&rep)?);
    } else if let Some(t) = table(format, &headers, &rows) {
        print!("{t}");
    } else {
        print!("{}", style::terminal_text(&render_skills(&rep)));
    }
    Ok(crate::exit::OK)
}

pub(crate) fn render_skills(rep: &TrustReport) -> String {
    if rep.skills.is_empty() {
        return "No skills found.\n".into();
    }
    let mut out = format!(
        "{}  {} changed since trusted, {} not trusted yet\n",
        bold("Skills"),
        rep.changed,
        rep.untrusted
    );
    for s in &rep.skills {
        out.push_str(&format!(
            "  {:<28} {}{}  {} file(s){}\n",
            s.subject.name,
            state_text(s.state),
            if s.changed_since_last_scan {
                paint("  changed since last scan", S::Yellow)
            } else {
                String::new()
            },
            s.file_count,
            if s.partial {
                dim("  (too large to fingerprint fully)")
            } else {
                String::new()
            }
        ));
        out.push_str(&format!(
            "    {}\n",
            dim(format!(
                "{} · first seen {}{}",
                s.subject.id,
                when(s.first_seen_ms),
                s.trusted_ms
                    .map(|t| format!(" · trusted {}", when(t)))
                    .unwrap_or_default()
            ))
        ));
        if let Some(r) = &s.reason {
            out.push_str(&format!("    {}\n", dim(format!("Reason: {r}"))));
        }
        if let Some(c) = &s.changes {
            out.push_str(&format!(
                "    {} added, {} removed, {} modified (names and sizes only):\n",
                c.added_total, c.removed_total, c.modified_total
            ));
            for (sign, list) in [("+", &c.added), ("-", &c.removed), ("~", &c.modified)] {
                for f in list {
                    let size = match (f.size_before, f.size_after) {
                        (Some(b), Some(a)) => format!("{b} → {a} bytes"),
                        (None, Some(a)) => format!("{a} bytes"),
                        (Some(b), None) => format!("{b} bytes"),
                        _ => String::new(),
                    };
                    out.push_str(&format!("      {sign} {} {}\n", f.path, dim(size)));
                }
            }
        }
        if let Some(e) = &s.error {
            out.push_str(&format!("    {}\n", dim(e)));
        }
    }
    if rep.alerts().next().is_some() {
        out.push_str(&dim("Trust a version you've checked with: holdmap analyzers trust SKILL_ID --reason \"…\"\n"));
    }
    out
}

/// Trust (or forget trusting) the version the last scan saw.
pub fn run_trust(skill: &str, reason: Option<&str>, forget: bool, json: bool) -> Result<u8> {
    let t = trust();
    let view = if forget {
        t.untrust(skill)?
    } else {
        t.trust(skill, reason)?
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else {
        println!(
            "{} {}: {}",
            style::ok_mark(),
            view.subject.name,
            view.state.label()
        );
    }
    Ok(crate::exit::OK)
}

fn band_text(b: RiskBand) -> String {
    match b {
        RiskBand::Critical => paint(b.label(), S::BoldRed),
        RiskBand::High => paint(b.label(), S::BoldRed),
        RiskBand::Elevated => paint(b.label(), S::Yellow),
        RiskBand::Low | RiskBand::None => dim(b.label()),
    }
}

/// Risks from the latest results and the last trust scan (no folder is read).
pub fn risk_entries(insights: &Insights, by: RiskBy) -> Vec<RiskEntry> {
    let trust = trust().report();
    risks(&insights.results, Some(&trust))
        .into_iter()
        .filter(|e| match by {
            RiskBy::All => true,
            RiskBy::Agent => e.scope == RiskScope::Agent,
            RiskBy::Folder => e.scope == RiskScope::Folder,
        })
        .collect()
}

pub fn run_risks(insights: &Insights, by: RiskBy, format: TableFormat) -> Result<u8> {
    let list = risk_entries(insights, by);
    let headers = [
        "scope",
        "key",
        "band",
        "points",
        "critical",
        "high",
        "medium",
        "low",
        "changed_skills",
        "why",
    ];
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|e| {
            vec![
                format!("{:?}", e.scope).to_lowercase(),
                e.key.clone(),
                e.band.label().into(),
                e.points.to_string(),
                e.open.critical.to_string(),
                e.open.high.to_string(),
                e.open.medium.to_string(),
                e.open.low.to_string(),
                e.changed_skills.to_string(),
                explain(e),
            ]
        })
        .collect();
    if format == TableFormat::Json {
        println!("{}", serde_json::to_string_pretty(&list)?);
    } else if let Some(t) = table(format, &headers, &rows) {
        print!("{t}");
    } else if list.is_empty() {
        println!("No risks: nothing open and no skill changed since you trusted it.");
    } else {
        let mut out = format!("{}\n", bold("Top risks"));
        for e in &list {
            out.push_str(&format!(
                "  {:<9} {:<7} {}  {}\n",
                band_text(e.band),
                format!("{:?}", e.scope).to_lowercase(),
                e.key,
                dim(explain(e))
            ));
            for c in &e.top {
                out.push_str(&format!(
                    "      {} {}\n",
                    dim(format!("{:?}", c.severity).to_lowercase()),
                    c.title
                ));
            }
        }
        out.push_str(&dim("Bands come from open findings (critical 40, high 15, medium 5, low 1) and skills changed since trusted (10). See docs/analyzers.md.\n"));
        print!("{}", style::terminal_text(&out));
    }
    Ok(crate::exit::OK)
}

/// Show the schedule and what it would run. Turning it on happens in the app, where it runs.
pub fn run_schedule(views: &[holdmap_analyzers::AnalyzerView], json: bool) -> Result<u8> {
    let s = FileStore::open_default().schedule();
    let plan = SchedulePlan::new(views, s.every_hours, &s.mcp_opt_in, s.skip_on_battery);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({ "schedule": s, "plan": plan, "next_ms": s.next_ms() })
            )?
        );
        return Ok(crate::exit::OK);
    }
    let mut out = format!(
        "{}  {}\n",
        bold("Scheduled scans"),
        if s.enabled {
            paint("on", S::Green)
        } else {
            dim("off")
        }
    );
    if s.enabled {
        out.push_str(&format!(
            "  Every {} h: {}\n",
            s.every_hours,
            s.analyzers.join(", ")
        ));
        if let Some(n) = s.next_ms() {
            out.push_str(&format!("  Next: {}\n", when(n)));
        }
    }
    out.push_str(&format!("  {}\n", plan.runs_when));
    for a in &plan.analyzers {
        match &a.blocked {
            None => out.push_str(&format!("  {} {}\n", style::ok_mark(), a.name)),
            Some(why) => out.push_str(&format!("  {} {}  {}\n", dim("·"), a.name, dim(why))),
        }
    }
    out.push_str(&dim(
        "Turn scheduled scans on or off in the desktop app: Settings → Analyzers.\n",
    ));
    print!("{}", style::terminal_text(&out));
    Ok(crate::exit::OK)
}

/// The trust line `holdmap insights` prints, from the last trust scan.
pub fn trust_alert_line() -> Option<String> {
    let rep = trust().report();
    let n = rep.changed + rep.untrusted;
    (n > 0).then(|| {
        format!(
            "{} {} skill(s) changed since you trusted them, {} not trusted yet. See: holdmap analyzers skills\n",
            paint("!", S::Yellow),
            rep.changed,
            rep.untrusted
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_and_defuses_formulas() {
        let out = csv(&["a", "b"], &[vec!["=cmd()".into(), "x,\"y\"".into()]]);
        assert_eq!(out, "a,b\n'=cmd(),\"x,\"\"y\"\"\"\n");
    }

    #[test]
    fn markdown_escapes_pipes_and_newlines() {
        let out = markdown(&["a"], &[vec!["x|y\nz".into()]]);
        assert_eq!(out, "| a |\n| --- |\n| x\\|y z |\n");
    }
}
