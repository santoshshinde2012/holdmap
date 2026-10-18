//! `list_needs`: the "needs you" list, the same rules as `holdmap needs` and the desktop Home.

use holdmap_core::attention::{AttentionItem, Urgency};
use serde_json::{json, Value};

/// Text and structured content for a list of items.
pub(super) fn result(items: &[AttentionItem]) -> (String, Value) {
    let text = if items.is_empty() {
        "Nothing needs attention.".to_string()
    } else {
        items
            .iter()
            .map(|i| {
                let level = match i.severity {
                    Urgency::High => "high",
                    Urgency::Medium => "medium",
                };
                format!("{level}: {}. {}", i.title, i.detail)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    (text, json!({ "items": items }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::attention::{RuleId, Stop, Target};

    #[test]
    fn lists_items_with_their_stop() {
        let item = AttentionItem {
            id: "exposed-port:e".into(),
            rule: RuleId::ExposedPort,
            severity: Urgency::Medium,
            title: "Vite on :5173 is open to the network".into(),
            detail: "Dev servers usually only need localhost.".into(),
            target: Target::List {
                entry_id: "e".into(),
            },
            stop: Some(Stop::Entry {
                entry_id: "e".into(),
            }),
        };
        let (text, data) = result(&[item]);
        assert!(text.starts_with("medium: Vite on :5173"));
        assert_eq!(data["items"][0]["rule"], "exposed-port");
        assert_eq!(data["items"][0]["stop"]["kind"], "entry");
        assert_eq!(result(&[]).0, "Nothing needs attention.");
    }
}
