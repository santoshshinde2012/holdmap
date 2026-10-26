//! Risk per agent and per folder: a banded label from open findings and changed skills.
//!
//! The formula is deliberately simple, deterministic and documented (docs/analyzers.md):
//!
//! | Contributor | Points |
//! | --- | --- |
//! | Open critical finding | 40 |
//! | Open high finding | 15 |
//! | Open medium finding | 5 |
//! | Open low finding | 1 |
//! | Info finding | 0 |
//! | Skill changed since you trusted it | 10 |
//!
//! Reviewed and snoozed findings don't count. A finding counts once per agent or folder even
//! when several analyzers report it. Bands: **Critical** (any open critical, or 40+ points),
//! **High** (25–39), **Elevated** (10–24), **Low** (1–9), **None** (0). The band is what's
//! shown; points are kept only so lists sort and exports stay explainable. Exposure (an
//! agent's open ports) is not part of the score: holdmap can't confirm a skill uses them.

use crate::finding::{AnalyzerResult, FindingState, Severity, Subject};
use crate::service::SeverityCounts;
use crate::trust::{TrustReport, TrustState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Points per open finding of a severity.
pub fn severity_points(s: Severity) -> u32 {
    match s {
        Severity::Critical => 40,
        Severity::High => 15,
        Severity::Medium => 5,
        Severity::Low => 1,
        Severity::Info => 0,
    }
}

/// Points per skill changed since you trusted it.
pub const CHANGED_SKILL_POINTS: u32 = 10;

/// Most contributing findings listed per entry.
pub const MAX_CONTRIBUTORS: usize = 5;

/// A risk band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskBand {
    /// Nothing open.
    None,
    /// 1–9 points.
    Low,
    /// 10–24 points.
    Elevated,
    /// 25–39 points.
    High,
    /// Any open critical, or 40+ points.
    Critical,
}

impl RiskBand {
    /// The band for `points`, raised to Critical by any open critical finding.
    pub fn of(points: u32, any_critical: bool) -> Self {
        match points {
            _ if any_critical => Self::Critical,
            40.. => Self::Critical,
            25..=39 => Self::High,
            10..=24 => Self::Elevated,
            1..=9 => Self::Low,
            0 => Self::None,
        }
    }
    /// Plain label.
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Low => "Low",
            Self::Elevated => "Elevated",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }
}

/// What a risk entry is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskScope {
    /// An agent (catalog id).
    Agent,
    /// A skill folder.
    Folder,
}

/// A finding that adds to a risk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskContributor {
    /// Finding fingerprint.
    pub fingerprint: String,
    /// Severity.
    pub severity: Severity,
    /// Title.
    pub title: String,
    /// Subject id.
    pub subject: String,
}

/// One agent's or folder's risk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskEntry {
    /// Agent or folder.
    pub scope: RiskScope,
    /// Agent id or folder path.
    pub key: String,
    /// Band.
    pub band: RiskBand,
    /// Points behind the band (see the module docs).
    pub points: u32,
    /// Open findings by severity.
    pub open: SeverityCounts,
    /// Skills changed since you trusted them.
    pub changed_skills: usize,
    /// The most severe contributing findings (first [`MAX_CONTRIBUTORS`]).
    pub top: Vec<RiskContributor>,
}

#[derive(Default)]
struct Acc<'a> {
    findings: BTreeMap<&'a str, &'a crate::finding::Finding>,
    changed: BTreeSet<&'a str>,
}

/// Risks for every agent and folder with anything to report, riskiest first (ties by key).
pub fn risks(results: &[AnalyzerResult], trust: Option<&TrustReport>) -> Vec<RiskEntry> {
    let mut by: BTreeMap<(RiskScope, String), Acc> = BTreeMap::new();
    let mut subjects: BTreeMap<&str, &Subject> = BTreeMap::new();
    for r in results {
        for s in &r.subjects {
            subjects.entry(s.id.as_str()).or_insert(s);
        }
    }
    let keys = |s: &Subject| {
        let mut k: Vec<(RiskScope, String)> = s
            .agents
            .iter()
            .map(|a| (RiskScope::Agent, a.clone()))
            .collect();
        if let Some(d) = &s.dir {
            k.push((RiskScope::Folder, d.clone()));
        }
        k
    };
    for f in results.iter().flat_map(|r| &r.findings) {
        if f.state != FindingState::Open || f.severity == Severity::Info {
            continue;
        }
        let Some(s) = subjects.get(f.subject.as_str()) else {
            continue;
        };
        for k in keys(s) {
            by.entry(k)
                .or_default()
                .findings
                .insert(f.fingerprint.as_str(), f);
        }
    }
    for s in trust.into_iter().flat_map(|t| &t.skills) {
        if s.state == TrustState::Changed {
            for k in keys(&s.subject) {
                by.entry(k)
                    .or_default()
                    .changed
                    .insert(s.subject.id.as_str());
            }
        }
    }
    let mut out: Vec<RiskEntry> = by
        .into_iter()
        .map(|((scope, key), acc)| {
            let mut found: Vec<_> = acc.findings.into_values().collect();
            found.sort_by(|a, b| {
                b.severity
                    .cmp(&a.severity)
                    .then_with(|| a.fingerprint.cmp(&b.fingerprint))
            });
            let points = found
                .iter()
                .map(|f| severity_points(f.severity))
                .sum::<u32>()
                + CHANGED_SKILL_POINTS * acc.changed.len() as u32;
            let any_critical = found.iter().any(|f| f.severity == Severity::Critical);
            RiskEntry {
                scope,
                key,
                band: RiskBand::of(points, any_critical),
                points,
                open: SeverityCounts::of(found.iter().copied()),
                changed_skills: acc.changed.len(),
                top: found
                    .iter()
                    .take(MAX_CONTRIBUTORS)
                    .map(|f| RiskContributor {
                        fingerprint: f.fingerprint.clone(),
                        severity: f.severity,
                        title: f.title.clone(),
                        subject: f.subject.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.band
            .cmp(&a.band)
            .then(b.points.cmp(&a.points))
            .then(a.scope.cmp(&b.scope))
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

/// A one-line "why" for an entry, e.g. `1 high, 2 medium, 1 changed skill`.
pub fn explain(e: &RiskEntry) -> String {
    let mut parts = Vec::new();
    for (n, label) in [
        (e.open.critical, "critical"),
        (e.open.high, "high"),
        (e.open.medium, "medium"),
        (e.open.low, "low"),
    ] {
        if n > 0 {
            parts.push(format!("{n} {label}"));
        }
    }
    if e.changed_skills > 0 {
        let s = if e.changed_skills == 1 { "" } else { "s" };
        parts.push(format!("{} changed skill{s}", e.changed_skills));
    }
    if parts.is_empty() {
        "nothing open".into()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Finding, Location, RunStatus, SubjectKind};

    fn f(fp: &str, severity: Severity, subject: &str) -> Finding {
        Finding {
            fingerprint: fp.into(),
            analyzer: "a.b".into(),
            rule_id: "r".into(),
            category: None,
            severity,
            title: format!("t {fp}"),
            subject: subject.into(),
            location: Location::default(),
            fix: None,
            refs: vec![],
            state: FindingState::Open,
            note: None,
            until_ms: None,
            new: false,
        }
    }

    fn subject(id: &str, agents: &[&str], dir: &str) -> Subject {
        Subject {
            id: id.into(),
            kind: SubjectKind::Skill,
            name: id.into(),
            agents: agents.iter().map(|a| a.to_string()).collect(),
            dir: Some(dir.into()),
            scope: Some("user".into()),
        }
    }

    fn result(subjects: Vec<Subject>, findings: Vec<Finding>) -> AnalyzerResult {
        AnalyzerResult {
            analyzer: "a.b".into(),
            analyzer_version: None,
            run_at_ms: 1,
            status: RunStatus::Complete,
            coverage: None,
            subjects,
            findings,
            details: vec![],
            notes: vec![],
            input_signature: None,
            duration_ms: None,
            tool_calls: vec![],
            changes: None,
        }
    }

    #[test]
    fn bands_follow_the_documented_thresholds() {
        assert_eq!(RiskBand::of(0, false), RiskBand::None);
        assert_eq!(RiskBand::of(9, false), RiskBand::Low);
        assert_eq!(RiskBand::of(10, false), RiskBand::Elevated);
        assert_eq!(RiskBand::of(24, false), RiskBand::Elevated);
        assert_eq!(RiskBand::of(25, false), RiskBand::High);
        assert_eq!(RiskBand::of(40, false), RiskBand::Critical);
        assert_eq!(RiskBand::of(1, true), RiskBand::Critical);
    }

    #[test]
    fn risk_adds_open_findings_per_agent_and_folder_and_skips_reviewed_ones() {
        let s1 = subject("s1", &["claude-code", "codex"], "/p/a");
        let s2 = subject("s2", &["codex"], "/p/b");
        let mut reviewed = f("r", Severity::Critical, "s1");
        reviewed.state = FindingState::Reviewed;
        let rs = vec![
            result(
                vec![s1.clone(), s2],
                vec![
                    f("h", Severity::High, "s1"),
                    f("m", Severity::Medium, "s2"),
                    reviewed,
                ],
            ),
            // The same fingerprint from a second result counts once.
            result(
                vec![s1],
                vec![f("h", Severity::High, "s1"), f("i", Severity::Info, "s1")],
            ),
        ];
        let out = risks(&rs, None);
        let get = |scope, key: &str| {
            out.iter()
                .find(|e| e.scope == scope && e.key == key)
                .unwrap()
        };
        let codex = get(RiskScope::Agent, "codex");
        assert_eq!((codex.points, codex.band), (20, RiskBand::Elevated));
        assert_eq!(codex.top[0].fingerprint, "h");
        assert_eq!(get(RiskScope::Agent, "claude-code").points, 15);
        assert_eq!(get(RiskScope::Folder, "/p/b").band, RiskBand::Low);
        assert_eq!(out[0].key, "codex", "riskiest first");
        assert_eq!(explain(codex), "1 high, 1 medium");
    }

    #[test]
    fn a_skill_changed_since_trusted_adds_points() {
        use crate::trust::{SkillTrustView, TrustReport};
        let s = subject("s1", &["claude-code"], "/p/a");
        let view = SkillTrustView {
            subject: s,
            state: TrustState::Changed,
            sha256: None,
            file_count: 0,
            total_bytes: 0,
            modified_ms: None,
            partial: false,
            first_seen_ms: 0,
            seen_ms: 0,
            changed_since_last_scan: true,
            trusted_ms: Some(0),
            reason: None,
            changes: None,
            error: None,
        };
        let t = TrustReport {
            skills: vec![view],
            changed: 1,
            untrusted: 0,
            scanned_ms: Some(0),
        };
        let out = risks(&[], Some(&t));
        assert_eq!(out.len(), 2);
        assert!(out
            .iter()
            .all(|e| e.points == 10 && e.band == RiskBand::Elevated && e.changed_skills == 1));
        assert_eq!(explain(&out[0]), "1 changed skill");
    }

    /// `cargo test -p holdmap-analyzers --release --lib risk -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark"]
    fn bench_scoring_50k_findings() {
        let subjects: Vec<Subject> = (0..500)
            .map(|i| {
                subject(
                    &format!("s{i}"),
                    &["claude-code", if i % 2 == 0 { "codex" } else { "cursor" }],
                    &format!("/p/{i}"),
                )
            })
            .collect();
        let sev = [
            Severity::Critical,
            Severity::High,
            Severity::Medium,
            Severity::Low,
            Severity::Info,
        ];
        let findings: Vec<Finding> = (0..50_000)
            .map(|i| f(&format!("fp{i}"), sev[i % 5], &format!("s{}", i % 500)))
            .collect();
        let rs = vec![result(subjects, findings)];
        let start = std::time::Instant::now();
        let out = risks(&rs, None);
        println!(
            "risk: 50k findings, 500 skills -> {} entries in {:?}",
            out.len(),
            start.elapsed()
        );
    }
}
