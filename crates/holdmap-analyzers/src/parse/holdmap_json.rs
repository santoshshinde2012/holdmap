//! holdmap's own result schema, for analyzers written against holdmap. Everything is still
//! re-validated and re-capped by the service; nothing is trusted as already clean.

use super::{json, OutputParser, ParseError, Parsed, ParsedFinding};
use crate::finding::{Location, RunStatus, Severity};
use crate::manifest::AnalyzerManifest;
use crate::runner::RawOutput;
use serde::Deserialize;

/// The `holdmap-json` parser.
#[derive(Debug, Default, Clone, Copy)]
pub struct HoldmapJsonParser;

#[derive(Deserialize)]
struct Report {
    findings: Vec<Item>,
    #[serde(default)]
    status: Option<RunStatus>,
    #[serde(default)]
    coverage: Option<f32>,
}

#[derive(Deserialize)]
struct Item {
    rule_id: String,
    #[serde(default)]
    category: Option<String>,
    severity: Severity,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    fix: Option<String>,
    #[serde(default)]
    location: Location,
    #[serde(default)]
    fingerprint: Option<String>,
    #[serde(default)]
    refs: Vec<String>,
}

impl OutputParser for HoldmapJsonParser {
    fn parse(&self, raw: &RawOutput, _: &AnalyzerManifest) -> Result<Parsed, ParseError> {
        let value = json(&raw.bytes)?;
        let report: Report = serde_json::from_value(value)
            .map_err(|_| ParseError::Shape("expected {\"findings\": [...]}"))?;
        Ok(Parsed {
            findings: report
                .findings
                .into_iter()
                .map(|i| ParsedFinding {
                    rule_id: i.rule_id,
                    category: i.category,
                    severity: Some(i.severity),
                    title: i.title,
                    fix: i.fix,
                    location: i.location,
                    producer_fingerprint: i.fingerprint,
                    refs: i.refs,
                    subject: None,
                })
                .collect(),
            details: Vec::new(),
            status: report.status,
            coverage: report.coverage.map(|c| c.clamp(0.0, 1.0)),
            notes: Vec::new(),
        })
    }
}
