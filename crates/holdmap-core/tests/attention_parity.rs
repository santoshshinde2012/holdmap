//! The Rust "needs you" rules against the case shared with the desktop's TypeScript mirror
//! (`apps/desktop/src/lib/attention-parity.test.ts`). Analyzer items in the case are checked by
//! `holdmap-analyzers` (`tests/needs_parity.rs`); here they are left out.

use holdmap_core::agents::AgentsReport;
use holdmap_core::attention::{attention, hot_agents, AttentionItem, RuleId};
use holdmap_core::model::PortEntry;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    entries: Vec<PortEntry>,
    report: AgentsReport,
    expected: Vec<AttentionItem>,
}

fn case() -> Case {
    let mut c: Case = serde_json::from_str(include_str!("fixtures/attention_case.json"))
        .expect("shared case parses");
    c.expected.retain(|i| i.rule != RuleId::Analyzer);
    c
}

#[test]
fn matches_the_typescript_rules() {
    let c = case();
    let items = attention(Some(&c.entries), Some(&c.report), vec![]);
    assert_eq!(items, c.expected);
}

#[test]
fn serializes_to_the_same_json() {
    let c = case();
    let items = attention(Some(&c.entries), Some(&c.report), vec![]);
    let raw: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/attention_case.json")).unwrap();
    let expected: Vec<&serde_json::Value> = raw["expected"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["rule"] != "analyzer")
        .collect();
    let got = serde_json::to_value(&items).unwrap();
    assert_eq!(got.as_array().unwrap().iter().collect::<Vec<_>>(), expected);
}

#[test]
fn nothing_without_a_scan_or_report() {
    assert!(attention(None, None, vec![]).is_empty());
}

#[test]
fn port_rules_need_only_the_scan() {
    let c = case();
    let items = attention(Some(&c.entries), None, vec![]);
    assert!(items.iter().all(|i| i.rule == RuleId::ExposedPort));
    assert!(!items.is_empty());
}

#[test]
fn extra_items_sort_with_the_rest() {
    let c = case();
    let mut extra = c.expected[0].clone();
    extra.id = "analyzer:x".into();
    extra.rule = RuleId::Analyzer;
    extra.title = "AAA first".into();
    let items = attention(Some(&c.entries), Some(&c.report), vec![extra]);
    assert_eq!(items[0].id, "analyzer:x");
    assert_eq!(items.len(), c.expected.len() + 1);
}

#[test]
fn hot_agents_are_access_and_settings_items() {
    let c = case();
    let hot = hot_agents(&attention(Some(&c.entries), Some(&c.report), vec![]));
    let mut ids: Vec<_> = hot.into_iter().collect();
    ids.sort();
    assert_eq!(ids, ["agent:1", "agent:2", "agent:51200"]);
}
