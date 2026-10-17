//! Analyzer "needs you" items against the case shared with the desktop's TypeScript mirror.

use holdmap_analyzers::needs::{counts_text, insight_items};
use holdmap_analyzers::service::{Insights, SeverityCounts};
use holdmap_core::agents::AgentsReport;
use holdmap_core::attention::{attention, AttentionItem, RuleId};
use holdmap_core::model::PortEntry;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    entries: Vec<PortEntry>,
    report: AgentsReport,
    insights: Insights,
    expected: Vec<AttentionItem>,
}

fn case() -> Case {
    serde_json::from_str(include_str!(
        "../../holdmap-core/tests/fixtures/attention_case.json"
    ))
    .expect("shared case parses")
}

#[test]
fn full_list_matches_the_typescript_rules() {
    let c = case();
    let extra = insight_items(&c.insights, &c.report);
    assert!(extra.iter().all(|i| i.rule == RuleId::Analyzer));
    assert!(!extra.is_empty());
    assert_eq!(
        attention(Some(&c.entries), Some(&c.report), extra),
        c.expected
    );
}

#[test]
fn reviewed_findings_raise_nothing() {
    let mut c = case();
    for r in &mut c.insights.results {
        for f in &mut r.findings {
            f.state = holdmap_analyzers::FindingState::Reviewed;
        }
    }
    assert!(insight_items(&c.insights, &c.report).is_empty());
}

#[test]
fn one_item_per_product() {
    let mut c = case();
    let twin = c.report.agents[0].clone();
    c.report.agents.push(twin);
    let items = insight_items(&c.insights, &c.report);
    let mut ids: Vec<_> = items.iter().map(|i| i.id.clone()).collect();
    ids.dedup();
    assert_eq!(ids.len(), items.len());
}

#[test]
fn counts_text_caps_parts() {
    let c = SeverityCounts {
        critical: 1,
        high: 2,
        medium: 3,
        low: 0,
        info: 0,
    };
    assert_eq!(counts_text(&c, 2), "1 critical, 2 high, …");
    assert_eq!(counts_text(&c, 3), "1 critical, 2 high, 3 medium");
    assert_eq!(counts_text(&SeverityCounts::default(), 2), "");
}
