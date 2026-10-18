//! "Needs you" items from approved analyzers: one per agent with open medium-or-worse findings.

use crate::finding::{Finding, Severity, Subject};
use crate::service::{Insights, SeverityCounts};
use crate::{AnalyzerResult, FindingState};
use holdmap_core::agents::AgentsReport;
use holdmap_core::attention::{AttentionItem, RuleId, Target, Urgency};
use std::collections::HashSet;

/// One open finding with its subject.
pub struct AgentFinding<'a> {
    /// The finding.
    pub finding: &'a Finding,
    /// Its subject, when the result lists it.
    pub subject: &'a Subject,
    /// The result it came from.
    pub result: &'a AnalyzerResult,
}

/// Open findings about subjects that belong to `product`, most severe first, then by title.
pub fn agent_findings<'a>(insights: &'a Insights, product: &str) -> Vec<AgentFinding<'a>> {
    let mut out: Vec<AgentFinding<'a>> = Vec::new();
    for result in &insights.results {
        for finding in &result.findings {
            let Some(subject) = result.subjects.iter().find(|s| s.id == finding.subject) else {
                continue;
            };
            if !subject.agents.iter().any(|a| a == product) || finding.state != FindingState::Open {
                continue;
            }
            out.push(AgentFinding {
                finding,
                subject,
                result,
            });
        }
    }
    out.sort_by(|a, b| {
        b.finding
            .severity
            .cmp(&a.finding.severity)
            .then_with(|| a.finding.title.cmp(&b.finding.title))
    });
    out
}

/// "2 high, 1 medium" (at most `max` parts, then "…").
pub fn counts_text(c: &SeverityCounts, max: usize) -> String {
    let parts: Vec<String> = [
        (c.critical, "critical"),
        (c.high, "high"),
        (c.medium, "medium"),
        (c.low, "low"),
        (c.info, "info"),
    ]
    .iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, s)| format!("{n} {s}"))
    .collect();
    let mut text = parts
        .iter()
        .take(max)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if parts.len() > max {
        text.push_str(", …");
    }
    text
}

/// Display name for a bundled analyzer id; other ids are shown as they are.
pub fn analyzer_name(id: &str) -> &str {
    match id {
        "nvidia.skillspector" => "SkillSpector",
        "holdmap.osv" => "OSV lookup",
        other => other,
    }
}

/// One item per agent product with open medium-or-worse findings.
pub fn insight_items(insights: &Insights, report: &AgentsReport) -> Vec<AttentionItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for agent in &report.agents {
        if !seen.insert(agent.product.as_str()) {
            continue;
        }
        let all = agent_findings(insights, &agent.product);
        let Some(top) = all.iter().find(|f| f.finding.severity >= Severity::Medium) else {
            continue;
        };
        let counts = SeverityCounts::of(all.iter().map(|f| f.finding));
        let fix = top
            .finding
            .fix
            .as_deref()
            .filter(|f| !f.is_empty())
            .map(|f| format!(". {f}"))
            .unwrap_or_default();
        items.push(AttentionItem {
            id: format!("analyzer:{}", agent.product),
            rule: RuleId::Analyzer,
            severity: if top.finding.severity >= Severity::High {
                Urgency::High
            } else {
                Urgency::Medium
            },
            title: format!(
                "{}: {} found {} in {}",
                agent.name,
                analyzer_name(&top.finding.analyzer),
                counts_text(&counts, 2),
                top.subject.name
            ),
            detail: format!("{}{fix}", top.finding.title),
            target: Target::Agents {
                agent_id: agent.id.clone(),
            },
            stop: None,
        });
    }
    items
}
