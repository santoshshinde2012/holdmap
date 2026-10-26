//! `list_skill_trust` and `list_risks`: read-only views of what holdmap already stored. Neither
//! reads a skill folder nor runs an analyzer; folders and file contents never appear.

use holdmap_analyzers::risk::{explain, risks, RiskBand, RiskScope};
use holdmap_analyzers::trust::{SkillTrustView, TrustReport};
use holdmap_analyzers::Insights;
use serde_json::{json, Value};

/// `list_skill_trust`: each skill's trust from the last scan the user ran in holdmap.
pub(super) fn skills(report: &TrustReport, args: &Value) -> (String, Value) {
    let alerts_only = args["alerts_only"].as_bool().unwrap_or(false);
    let rows: Vec<Value> = report
        .skills
        .iter()
        .filter(|s| !alerts_only || s.state.alerts())
        .map(row)
        .collect();
    let text = if report.skills.is_empty() {
        "No skill trust yet. The user scans skills in holdmap (Security → Skills) or with \
         `holdmap analyzers skills`."
            .to_string()
    } else {
        let mut lines = vec![format!(
            "{} changed since trusted, {} not trusted yet.",
            report.changed, report.untrusted
        )];
        lines.extend(
            report
                .skills
                .iter()
                .filter(|s| !alerts_only || s.state.alerts())
                .map(|s| {
                    format!(
                        "{}: {} ({} file(s))",
                        s.subject.name,
                        s.state.label(),
                        s.file_count
                    )
                }),
        );
        lines.join("\n")
    };
    (
        text,
        json!({ "skills": rows, "changed": report.changed, "untrusted": report.untrusted, "scanned_ms": report.scanned_ms }),
    )
}

fn row(s: &SkillTrustView) -> Value {
    json!({
        "id": s.subject.id, "name": s.subject.name, "agents": s.subject.agents, "scope": s.subject.scope,
        "state": s.state, "sha256": s.sha256, "files": s.file_count, "bytes": s.total_bytes,
        "partial": s.partial, "first_seen_ms": s.first_seen_ms, "seen_ms": s.seen_ms,
        "changed_since_last_scan": s.changed_since_last_scan, "trusted_ms": s.trusted_ms,
        "reason": s.reason, "changes": s.changes,
    })
}

/// `list_risks`: banded risk per agent and per skill folder from stored results.
pub(super) fn list_risks(
    insights: &Insights,
    trust: &TrustReport,
    args: &Value,
) -> (String, Value) {
    let scope = match args["by"].as_str() {
        Some("agent") => Some(RiskScope::Agent),
        Some("folder") => Some(RiskScope::Folder),
        _ => None,
    };
    let min = match args["min_band"].as_str() {
        Some("critical") => RiskBand::Critical,
        Some("high") => RiskBand::High,
        Some("elevated") => RiskBand::Elevated,
        _ => RiskBand::Low,
    };
    let mut list: Vec<_> = risks(&insights.results, Some(trust))
        .into_iter()
        .filter(|e| scope.is_none_or(|s| e.scope == s) && e.band >= min)
        .collect();
    // MCP never shows folder paths: a folder row is named by the skill(s) scanned there.
    for e in list.iter_mut().filter(|e| e.scope == RiskScope::Folder) {
        e.key = folder_label(insights, &e.key);
    }
    let text = if list.is_empty() {
        "No matching risks.".to_string()
    } else {
        list.iter()
            .map(|e| format!("{} {:?} {}: {}", e.band.label(), e.scope, e.key, explain(e)))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let rows: Vec<Value> = list
        .iter()
        .map(|e| {
            let mut v = serde_json::to_value(e).unwrap_or(Value::Null);
            v["why"] = json!(explain(e));
            v
        })
        .collect();
    (
        format!("{text}\nPoints: critical 40, high 15, medium 5, low 1 per open finding, 10 per skill changed since trusted; any open critical makes the band Critical."),
        json!({ "risks": rows }),
    )
}

/// "skill folder: a, b" for the skills scanned in `dir`; never the path itself.
fn folder_label(insights: &Insights, dir: &str) -> String {
    let mut names: Vec<&str> = insights
        .results
        .iter()
        .flat_map(|r| &r.subjects)
        .filter(|s| s.dir.as_deref() == Some(dir))
        .map(|s| s.name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    if names.is_empty() {
        "skill folder".to_string()
    } else {
        format!("skill folder: {}", names.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stores_say_how_to_start_and_read_nothing() {
        let (text, data) = skills(&TrustReport::default(), &json!({}));
        assert!(text.contains("holdmap analyzers skills"));
        assert_eq!(data["skills"], json!([]));
        let (text, data) = list_risks(
            &Insights {
                results: vec![],
                open: Default::default(),
            },
            &TrustReport::default(),
            &json!({"by": "agent"}),
        );
        assert!(text.starts_with("No matching risks."));
        assert_eq!(data["risks"], json!([]));
    }

    #[test]
    fn folder_risks_name_the_skill_never_the_path() {
        let result: holdmap_analyzers::AnalyzerResult = serde_json::from_value(json!({
            "analyzer": "nvidia.skillspector", "run_at_ms": 2, "status": "complete",
            "subjects": [{"id": "s", "kind": "skill", "name": "release-check", "agents": ["claude-code"],
                          "dir": "/Users/someone/code/app/.claude/skills/release-check"}],
            "findings": [{"fingerprint": "f", "analyzer": "nvidia.skillspector", "rule_id": "r",
                          "severity": "high", "title": "t", "subject": "s", "location": {}}],
        }))
        .unwrap();
        let insights = Insights {
            results: vec![result],
            open: Default::default(),
        };
        let (text, data) = list_risks(&insights, &TrustReport::default(), &json!({"by": "folder"}));
        let all = format!("{text}{data}");
        assert!(!all.contains("/Users/someone"), "{all}");
        assert_eq!(data["risks"][0]["key"], "skill folder: release-check");
    }
}
