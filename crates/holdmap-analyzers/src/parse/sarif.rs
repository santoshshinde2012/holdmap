//! SARIF 2.1.0: the subset holdmap needs (rule, level or security-severity, first location,
//! partial fingerprints, suppressions, execution status). `message` text is never read: it
//! often quotes the matched code.

use super::plain::plain;
use super::{json, text, OutputParser, ParseError, Parsed, ParsedFinding};
use crate::finding::{Location, RunStatus, Severity};
use crate::manifest::AnalyzerManifest;
use crate::runner::RawOutput;
use serde_json::Value;
use std::collections::HashMap;

/// The `sarif` parser.
#[derive(Debug, Default, Clone, Copy)]
pub struct SarifParser;

impl OutputParser for SarifParser {
    fn parse(&self, raw: &RawOutput, _: &AnalyzerManifest) -> Result<Parsed, ParseError> {
        let root = json(&raw.bytes)?;
        let runs = root
            .get("runs")
            .and_then(Value::as_array)
            .ok_or(ParseError::Shape("no runs"))?;
        let mut parsed = Parsed::default();
        let mut failed = false;
        for run in runs {
            let rules = rule_index(run);
            for result in run
                .get("results")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if suppressed(result) {
                    continue;
                }
                let rule_id = text(result.get("ruleId"))
                    .or_else(|| text(result.pointer("/rule/id")))
                    .unwrap_or_else(|| "unknown".into());
                let rule = rules.get(rule_id.as_str()).copied();
                parsed.findings.push(ParsedFinding {
                    severity: Some(severity(result, rule)),
                    title: rule.and_then(title),
                    fix: rule.and_then(fix),
                    category: rule.and_then(|r| {
                        r.pointer("/properties/tags")
                            .and_then(Value::as_array)
                            .and_then(|t| t.first())
                            .and_then(|t| text(Some(t)))
                    }),
                    location: location(result),
                    producer_fingerprint: fingerprint(result),
                    refs: rule
                        .and_then(|r| text(r.get("helpUri")))
                        .into_iter()
                        .collect(),
                    subject: None,
                    rule_id,
                });
            }
            for invocation in run
                .get("invocations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if invocation.get("executionSuccessful") == Some(&Value::Bool(false)) {
                    failed = true;
                }
            }
        }
        parsed.status = Some(if failed {
            RunStatus::Partial
        } else {
            RunStatus::Complete
        });
        Ok(parsed)
    }
}

/// The rule's short description, else a readable form of its name. A bare or generic name
/// (Bandit's `blacklist`) gives way to the slug a help link ends with (`#b404-import-subprocess`).
fn title(rule: &Value) -> Option<String> {
    if let Some(short) = text(rule.pointer("/shortDescription/text")) {
        return Some(plain(&short));
    }
    let slug = text(rule.get("helpUri"))
        .and_then(|u| u.rsplit_once('#').map(|(_, f)| f.to_string()))
        .and_then(|f| {
            let (head, rest) = f.split_once('-')?;
            let is_id = head.chars().any(|c| c.is_ascii_digit())
                && head.chars().all(|c| c.is_ascii_alphanumeric());
            (is_id && !rest.is_empty()).then(|| rest.to_string())
        });
    let name = text(rule.get("name"));
    let raw = match (&name, slug) {
        (Some(n), Some(slug)) if !n.contains(['_', ' ', '-']) => slug,
        (Some(n), _) => n.clone(),
        (None, slug) => slug?,
    };
    Some(sentence(&raw))
}

/// `subprocess_popen_with_shell` or `import-subprocess` as `Subprocess popen with shell`.
fn sentence(raw: &str) -> String {
    if raw.contains(' ') || !raw.contains(['_', '-']) {
        return raw.to_string();
    }
    let words = raw.replace(['_', '-'], " ");
    let mut chars = words.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The rule's help as plain text, unless it only restates the title and description (Trivy's
/// "Secret <title> Severity: … Match: …"), which would show the match as if it were advice.
fn fix(rule: &Value) -> Option<String> {
    let help = plain(&text(rule.pointer("/help/text"))?);
    let restates = |p: &str| {
        text(rule.pointer(p)).is_some_and(|t| {
            let t = plain(&t);
            !t.is_empty() && help.contains(&t)
        })
    };
    if restates("/shortDescription/text") && restates("/fullDescription/text") {
        return None;
    }
    (!help.is_empty()).then_some(help)
}

fn rule_index(run: &Value) -> HashMap<&str, &Value> {
    run.pointer("/tool/driver/rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r.get("id")?.as_str()?, r)))
        .collect()
}

fn suppressed(result: &Value) -> bool {
    result
        .get("suppressions")
        .and_then(Value::as_array)
        .is_some_and(|s| {
            s.iter().any(|s| {
                s.get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("accepted")
                    == "accepted"
            })
        })
}

fn severity(result: &Value, rule: Option<&Value>) -> Severity {
    let score = |v: Option<&Value>| {
        v.and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
    };
    if let Some(s) = score(result.pointer("/properties/security-severity"))
        .or_else(|| score(rule.and_then(|r| r.pointer("/properties/security-severity"))))
    {
        return Severity::from_score(s);
    }
    let level = text(result.get("level"))
        .or_else(|| rule.and_then(|r| text(r.pointer("/defaultConfiguration/level"))))
        .unwrap_or_else(|| "warning".into());
    match level.as_str() {
        "none" => Severity::Info,
        other => Severity::from_label(other).unwrap_or(Severity::Medium),
    }
}

fn location(result: &Value) -> Location {
    let physical = result.pointer("/locations/0/physicalLocation");
    Location {
        file: physical.and_then(|p| text(p.pointer("/artifactLocation/uri"))),
        line: physical
            .and_then(|p| p.pointer("/region/startLine"))
            .and_then(Value::as_u64)
            .and_then(|l| u32::try_from(l).ok()),
    }
}

fn fingerprint(result: &Value) -> Option<String> {
    for key in ["fingerprints", "partialFingerprints"] {
        if let Some(Value::Object(map)) = result.get(key) {
            let mut entries: Vec<_> = map
                .iter()
                .filter_map(|(k, v)| Some((k, v.as_str()?)))
                .collect();
            entries.sort();
            if let Some((k, v)) = entries.first() {
                return Some(format!("{k}:{v}"));
            }
        }
    }
    None
}
