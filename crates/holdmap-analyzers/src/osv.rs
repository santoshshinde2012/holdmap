//! Built-in OSV lookup: pinned `(ecosystem, name, version)` triples go to OSV.dev's batch API,
//! then the details of matched advisories. The request bodies are built once and shown on the
//! permissions sheet byte for byte, so the preview is exactly what is sent.

use crate::finding::Severity;
use crate::input::{AnalyzerInput, PackageCoord};
use crate::manifest::AnalyzerManifest;
use crate::parse::{json, OutputParser, ParseError, Parsed, ParsedDetail, ParsedFinding};
use crate::runner::{AnalyzerRunner, RawOutput, RunContext, RunError};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io;
use std::time::Duration;

/// OSV's API root.
pub const API: &str = "https://api.osv.dev/v1";
/// Queries per batch request.
pub const BATCH: usize = 100;
/// Advisory details fetched per run.
pub const MAX_DETAILS: usize = 50;

/// The HTTPS seam (fake in tests).
pub trait OsvTransport: Send + Sync {
    /// POST a JSON body.
    fn post(&self, url: &str, body: &[u8]) -> io::Result<Vec<u8>>;
    /// GET a URL.
    fn get(&self, url: &str) -> io::Result<Vec<u8>>;
}

/// Exact request bodies for the batch queries, in order.
pub fn request_bodies(input: &AnalyzerInput) -> Vec<String> {
    let coords: Vec<&PackageCoord> = input
        .subjects
        .iter()
        .filter_map(|s| s.package.as_ref())
        .collect();
    coords
        .chunks(BATCH)
        .map(|chunk| {
            let queries: Vec<Value> = chunk
                .iter()
                .map(|c| json!({"package": {"name": c.name, "ecosystem": c.ecosystem}, "version": c.version}))
                .collect();
            json!({ "queries": queries }).to_string()
        })
        .collect()
}

/// Runs the OSV lookup.
pub struct OsvRunner {
    transport: Option<Box<dyn OsvTransport>>,
}

impl OsvRunner {
    /// A runner over `transport` (`None` when this build has no HTTPS client).
    pub fn new(transport: Option<Box<dyn OsvTransport>>) -> Self {
        Self { transport }
    }

    /// The default HTTPS transport, when compiled in.
    pub fn with_default_transport() -> Self {
        #[cfg(feature = "osv-http")]
        return Self::new(Some(Box::new(UreqTransport::new(Duration::from_secs(20)))));
        #[cfg(not(feature = "osv-http"))]
        Self::new(None)
    }
}

impl AnalyzerRunner for OsvRunner {
    fn run(
        &self,
        _: &AnalyzerManifest,
        input: &AnalyzerInput,
        _: &RunContext,
    ) -> Result<Vec<RawOutput>, RunError> {
        let transport = self.transport.as_deref().ok_or_else(|| {
            RunError::Unavailable("This build of holdmap can't reach OSV.dev.".into())
        })?;
        let coords: Vec<&PackageCoord> = input
            .subjects
            .iter()
            .filter_map(|s| s.package.as_ref())
            .collect();
        if coords.is_empty() {
            return Err(RunError::NoSubjects(
                "no pinned npm or PyPI packages were found".into(),
            ));
        }
        let mut notes = Vec::new();
        let mut matches: Vec<(String, Vec<String>)> = Vec::new();
        let mut failed = false;
        for (body, chunk) in request_bodies(input).iter().zip(coords.chunks(BATCH)) {
            let Some(results) = transport
                .post(&format!("{API}/querybatch"), body.as_bytes())
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|v| v.get("results").and_then(Value::as_array).cloned())
            else {
                failed = true;
                notes.push("OSV.dev could not be reached or answered unexpectedly.".into());
                break;
            };
            for (coord, result) in chunk.iter().zip(results) {
                let ids: Vec<String> = result
                    .get("vulns")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.get("id")?.as_str().map(String::from))
                    .filter(|id| safe_id(id))
                    .collect();
                if result.get("next_page_token").is_some() {
                    notes.push(format!(
                        "{} has more advisories than one page; only the first page was read.",
                        coord.name
                    ));
                }
                matches.push((coord.subject_id(), ids));
            }
        }
        let mut vulns = serde_json::Map::new();
        let unique: Vec<&String> = {
            let mut seen: Vec<&String> = matches.iter().flat_map(|(_, ids)| ids).collect();
            seen.sort();
            seen.dedup();
            seen
        };
        if unique.len() > MAX_DETAILS {
            notes.push(format!(
                "Details were fetched for the first {MAX_DETAILS} of {} advisories.",
                unique.len()
            ));
        }
        for id in unique.into_iter().take(MAX_DETAILS) {
            if let Some(v) = transport
                .get(&format!("{API}/vulns/{id}"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            {
                vulns.insert(id.clone(), reduce(&v));
            }
        }
        let packages: BTreeMap<String, &PackageCoord> =
            coords.iter().map(|c| (c.subject_id(), *c)).collect();
        let bytes = json!({
            "matches": matches.iter().map(|(s, ids)| json!({"subject": s, "package": packages.get(s), "ids": ids})).collect::<Vec<_>>(),
            "vulns": vulns,
        })
        .to_string()
        .into_bytes();
        Ok(vec![RawOutput {
            subjects: input.subjects.iter().map(|s| s.subject.clone()).collect(),
            exit_code: None,
            bytes,
            notes,
            failed,
        }])
    }
}

/// Keep only the advisory fields holdmap shows.
fn reduce(v: &Value) -> Value {
    let label = v
        .pointer("/database_specific/severity")
        .and_then(Value::as_str);
    let fixed: Vec<Value> = v
        .get("affected")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|a| {
            let events = a
                .get("ranges")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .flat_map(|r| {
                    r.get("events")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                })
                .filter_map(|e| e.get("fixed").and_then(Value::as_str).map(String::from))
                .collect::<Vec<_>>();
            json!({
                "name": a.pointer("/package/name"),
                "ecosystem": a.pointer("/package/ecosystem"),
                "fixed": events,
            })
        })
        .collect();
    json!({ "summary": v.get("summary"), "severity": label, "fixed": fixed })
}

fn safe_id(id: &str) -> bool {
    id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

/// Parses the reduced OSV output.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsvParser;

impl OutputParser for OsvParser {
    fn parse(&self, raw: &RawOutput, _: &AnalyzerManifest) -> Result<Parsed, ParseError> {
        let root = json(&raw.bytes)?;
        let matches = root
            .get("matches")
            .and_then(Value::as_array)
            .ok_or(ParseError::Shape("no matches"))?;
        let vulns = root.get("vulns").cloned().unwrap_or_default();
        let mut parsed = Parsed::default();
        for m in matches {
            let Some(subject) = m.get("subject").and_then(Value::as_str) else {
                continue;
            };
            let name = m
                .pointer("/package/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let ecosystem = m
                .pointer("/package/ecosystem")
                .and_then(Value::as_str)
                .unwrap_or("");
            let ids = m
                .get("ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            parsed.details.push(ParsedDetail {
                subject: Some(subject.to_string()),
                label: "Known advisories".into(),
                value: if ids.is_empty() {
                    "none".into()
                } else {
                    ids.len().to_string()
                },
            });
            for id in ids.iter().filter_map(Value::as_str) {
                let v = vulns.get(id);
                let label = v.and_then(|v| v.get("severity")).and_then(Value::as_str);
                let severity = label.and_then(Severity::from_label);
                let fixed: Vec<String> = v
                    .and_then(|v| v.get("fixed"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|a| {
                        a.get("name").and_then(Value::as_str) == Some(name)
                            && a.get("ecosystem").and_then(Value::as_str) == Some(ecosystem)
                    })
                    .flat_map(|a| {
                        a.get("fixed")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default()
                    })
                    .filter_map(|f| f.as_str().map(String::from))
                    .collect();
                parsed.findings.push(ParsedFinding {
                    rule_id: id.to_string(),
                    category: Some(
                        if severity.is_some() {
                            "vulnerability"
                        } else {
                            "vulnerability (severity unknown)"
                        }
                        .into(),
                    ),
                    severity,
                    title: v
                        .and_then(|v| v.get("summary"))
                        .and_then(Value::as_str)
                        .map(String::from)
                        .or_else(|| Some(format!("Known vulnerability {id}"))),
                    fix: fixed.last().map(|f| format!("Pin {name} to {f} or later.")),
                    location: Default::default(),
                    producer_fingerprint: Some(id.to_string()),
                    refs: vec![format!("https://osv.dev/vulnerability/{id}")],
                    subject: Some(subject.to_string()),
                });
            }
        }
        Ok(parsed)
    }
}

/// HTTPS via `ureq` with bundled Mozilla roots; no cookies, no redirects to other hosts.
#[cfg(feature = "osv-http")]
pub struct UreqTransport {
    agent: ureq::Agent,
}

#[cfg(feature = "osv-http")]
impl UreqTransport {
    /// A transport with an overall per-request timeout.
    pub fn new(timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .max_redirects(0)
            .https_only(true)
            .user_agent(concat!("holdmap/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { agent }
    }
}

#[cfg(feature = "osv-http")]
impl OsvTransport for UreqTransport {
    fn post(&self, url: &str, body: &[u8]) -> io::Result<Vec<u8>> {
        self.agent
            .post(url)
            .header("content-type", "application/json")
            .send(body)
            .map_err(io::Error::other)?
            .body_mut()
            .with_config()
            .limit(8 * 1024 * 1024)
            .read_to_vec()
            .map_err(io::Error::other)
    }

    fn get(&self, url: &str) -> io::Result<Vec<u8>> {
        self.agent
            .get(url)
            .call()
            .map_err(io::Error::other)?
            .body_mut()
            .with_config()
            .limit(2 * 1024 * 1024)
            .read_to_vec()
            .map_err(io::Error::other)
    }
}
