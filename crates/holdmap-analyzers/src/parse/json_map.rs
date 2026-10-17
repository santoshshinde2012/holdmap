//! Arbitrary JSON mapped by the manifest's field paths (SkillSpector uses this).

use super::{
    at, json, status_from, text, OutputParser, ParseError, Parsed, ParsedDetail, ParsedFinding,
};
use crate::finding::{Location, Severity};
use crate::manifest::AnalyzerManifest;
use crate::runner::RawOutput;
use serde_json::Value;

/// The `json-map` parser.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonMapParser;

impl OutputParser for JsonMapParser {
    fn parse(&self, raw: &RawOutput, manifest: &AnalyzerManifest) -> Result<Parsed, ParseError> {
        let spec = &manifest.output;
        let root = json(&raw.bytes)?;
        let items_path = spec.items.as_deref().unwrap_or("").trim_end_matches("[*]");
        let items = match at(&root, items_path) {
            Some(Value::Array(items)) => items,
            _ => return Err(ParseError::Shape("the finding list is missing")),
        };
        let field =
            |item: &Value, path: &Option<String>| path.as_deref().and_then(|p| text(at(item, p)));
        let findings = items
            .iter()
            .filter(|item| item.is_object())
            .map(|item| ParsedFinding {
                rule_id: field(item, &spec.rule_id).unwrap_or_else(|| "unknown".into()),
                category: field(item, &spec.category),
                severity: field(item, &spec.severity).and_then(|s| Severity::from_label(&s)),
                title: field(item, &spec.title),
                fix: field(item, &spec.fix),
                location: Location {
                    file: field(item, &spec.file),
                    line: field(item, &spec.line).and_then(|l| l.parse().ok()),
                },
                producer_fingerprint: field(item, &spec.fingerprint),
                refs: spec
                    .refs
                    .as_deref()
                    .map(|p| refs(at(item, p)))
                    .unwrap_or_default(),
                subject: None,
            })
            .collect();
        let details = spec
            .details
            .iter()
            .filter_map(|d| {
                let value = text(at(&root, &d.path))?.replace('_', " ");
                Some(ParsedDetail {
                    subject: None,
                    label: d.label.clone(),
                    value: format!("{value}{}", d.suffix.as_deref().unwrap_or("")),
                })
            })
            .collect();
        Ok(Parsed {
            findings,
            details,
            status: spec
                .status
                .as_deref()
                .and_then(|p| text(at(&root, p)))
                .and_then(|s| status_from(&s)),
            coverage: spec
                .coverage_percent
                .as_deref()
                .and_then(|p| at(&root, p))
                .and_then(Value::as_f64)
                .map(|c| (c / 100.0).clamp(0.0, 1.0) as f32),
            notes: Vec::new(),
        })
    }
}

fn refs(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}
