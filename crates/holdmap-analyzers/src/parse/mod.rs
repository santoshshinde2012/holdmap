//! Output parsers: turn one [`RawOutput`] into findings using only the fields the manifest
//! names. Producer text that isn't mapped (snippets, evidence, explanations) is never read.

mod holdmap_json;
mod json_map;
mod sarif;

pub use holdmap_json::HoldmapJsonParser;
pub use json_map::JsonMapParser;
pub use sarif::SarifParser;

use crate::finding::{Location, RunStatus, Severity};
use crate::manifest::{AnalyzerManifest, OutputFormat};
use crate::runner::RawOutput;
use serde_json::Value;
use thiserror::Error;

/// A finding before identity, subject and text limits are applied.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedFinding {
    /// Rule id.
    pub rule_id: String,
    /// Category.
    pub category: Option<String>,
    /// Severity (`None` when the producer's label is unknown; mapped to medium later).
    pub severity: Option<Severity>,
    /// Title.
    pub title: Option<String>,
    /// Fix hint.
    pub fix: Option<String>,
    /// Location.
    pub location: Location,
    /// The producer's stable fingerprint.
    pub producer_fingerprint: Option<String>,
    /// Reference links.
    pub refs: Vec<String>,
    /// Subject id, when one output covers several subjects.
    pub subject: Option<String>,
}

/// A labelled fact before text limits are applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDetail {
    /// Subject id, when one output covers several subjects.
    pub subject: Option<String>,
    /// Label.
    pub label: String,
    /// Value.
    pub value: String,
}

/// What a parser read from one output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    /// Findings.
    pub findings: Vec<ParsedFinding>,
    /// Facts about the output's subject(s).
    pub details: Vec<ParsedDetail>,
    /// Status the producer reported.
    pub status: Option<RunStatus>,
    /// Coverage the producer reported (0–1).
    pub coverage: Option<f32>,
    /// Plain-language notes.
    pub notes: Vec<String>,
}

/// Why output could not be read.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Not JSON.
    #[error("the report is not valid JSON")]
    NotJson,
    /// JSON of the wrong shape.
    #[error("the report does not have the expected shape: {0}")]
    Shape(&'static str),
}

/// Parses one output format.
pub trait OutputParser: Send + Sync {
    /// Parse `raw` as described by `manifest`.
    fn parse(&self, raw: &RawOutput, manifest: &AnalyzerManifest) -> Result<Parsed, ParseError>;
}

/// The parser for a manifest's declared format (`None` for formats a built-in parses itself).
pub fn for_format(format: OutputFormat) -> Option<Box<dyn OutputParser>> {
    match format {
        OutputFormat::JsonMap => Some(Box::new(JsonMapParser)),
        OutputFormat::Sarif => Some(Box::new(SarifParser)),
        OutputFormat::HoldmapJson => Some(Box::new(HoldmapJsonParser)),
        OutputFormat::Osv => None,
    }
}

pub(crate) fn json(bytes: &[u8]) -> Result<Value, ParseError> {
    serde_json::from_slice(bytes).map_err(|_| ParseError::NotJson)
}

/// Read a dotted path (`a.b.0.c`, with an optional leading `$.`).
pub(crate) fn at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.strip_prefix("$.").unwrap_or(path);
    path.split('.')
        .filter(|p| !p.is_empty())
        .try_fold(value, |v, key| match v {
            Value::Object(map) => map.get(key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => None,
        })
}

/// A scalar as text (strings, numbers, booleans).
pub(crate) fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub(crate) fn status_from(label: &str) -> Option<RunStatus> {
    match label.trim().to_ascii_lowercase().as_str() {
        "complete" | "completed" | "success" | "ok" => Some(RunStatus::Complete),
        "partial" | "incomplete" => Some(RunStatus::Partial),
        "failed" | "error" | "failure" => Some(RunStatus::Failed),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dotted_paths_read_objects_and_arrays() {
        let v = json!({"a": {"b": [{"c": 3}]}});
        assert_eq!(at(&v, "$.a.b.0.c"), Some(&json!(3)));
        assert_eq!(at(&v, "a.x"), None);
        assert_eq!(text(at(&v, "a.b.0.c")).as_deref(), Some("3"));
        assert_eq!(text(Some(&json!({}))), None);
    }
}
