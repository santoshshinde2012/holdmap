//! IPC adapter for the "needs you" list. The rules live in holdmap-core (`attention`) and
//! holdmap-analyzers (`needs`), shared with `holdmap needs` and the MCP `list_needs` tool. The
//! front end passes the scan, report and insights it is showing, so the list always matches
//! the screen; nothing is read or probed here.

use holdmap_analyzers::Insights;
use holdmap_core::agents::AgentsReport;
use holdmap_core::attention::{attention, AttentionItem};
use holdmap_core::model::PortEntry;

/// The sorted "needs you" items for what the window shows.
#[tauri::command]
pub fn needs(
    entries: Option<Vec<PortEntry>>,
    report: Option<AgentsReport>,
    insights: Option<Insights>,
) -> Vec<AttentionItem> {
    items(entries.as_deref(), report.as_ref(), insights.as_ref())
}

fn items(
    entries: Option<&[PortEntry]>,
    report: Option<&AgentsReport>,
    insights: Option<&Insights>,
) -> Vec<AttentionItem> {
    let extra = match (insights, report) {
        (Some(i), Some(r)) => holdmap_analyzers::needs::insight_items(i, r),
        _ => Vec::new(),
    };
    attention(entries, report, extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_without_data() {
        assert!(items(None, None, None).is_empty());
    }

    #[test]
    fn matches_the_shared_case() {
        #[derive(serde::Deserialize)]
        struct Case {
            entries: Vec<PortEntry>,
            report: AgentsReport,
            insights: Insights,
            expected: Vec<AttentionItem>,
        }
        let c: Case = serde_json::from_str(include_str!(
            "../../../../crates/holdmap-core/tests/fixtures/attention_case.json"
        ))
        .unwrap();
        assert_eq!(
            items(Some(&c.entries), Some(&c.report), Some(&c.insights)),
            c.expected
        );
    }
}
