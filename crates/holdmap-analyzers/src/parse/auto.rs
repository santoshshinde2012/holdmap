//! `auto`: for analyzers whose output format isn't declared (most MCP tools). SARIF and
//! holdmap JSON are recognised by shape; otherwise the manifest's field mapping is applied
//! when it names `items`; otherwise a short excerpt of the text is kept as one detail, so
//! you can see what came back and write a mapping.

use super::{
    json, HoldmapJsonParser, JsonMapParser, OutputParser, ParseError, Parsed, ParsedDetail,
    SarifParser,
};
use crate::manifest::AnalyzerManifest;
use crate::runner::RawOutput;
use serde_json::Value;

/// Characters of unrecognised output kept as a detail.
pub const EXCERPT: usize = 150;

/// The `auto` parser.
#[derive(Debug, Default, Clone, Copy)]
pub struct AutoParser;

impl OutputParser for AutoParser {
    fn parse(&self, raw: &RawOutput, manifest: &AnalyzerManifest) -> Result<Parsed, ParseError> {
        if let Ok(root) = json(&raw.bytes) {
            if root.get("runs").is_some_and(Value::is_array) {
                return SarifParser.parse(raw, manifest);
            }
            if root.get("findings").is_some_and(Value::is_array) {
                if let Ok(p) = HoldmapJsonParser.parse(raw, manifest) {
                    return Ok(p);
                }
            }
            if manifest.output.items.is_some() {
                if let Ok(p) = JsonMapParser.parse(raw, manifest) {
                    return Ok(p);
                }
            }
        }
        Ok(excerpt(raw))
    }
}

fn excerpt(raw: &RawOutput) -> Parsed {
    let text = String::from_utf8_lossy(&raw.bytes);
    let mut value: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() > EXCERPT {
        value = value.chars().take(EXCERPT).collect::<String>() + "…";
    }
    let label = match &raw.tool {
        Some(t) => format!("Output of {t}"),
        None => "Output".to_string(),
    };
    Parsed {
        details: vec![ParsedDetail {
            subject: None,
            label,
            value,
        }],
        ..Parsed::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Subject, SubjectKind};

    fn manifest(output: &str) -> AnalyzerManifest {
        AnalyzerManifest::parse(&format!(
            r#"schema = 1
id = "local.x"
name = "X"
publisher = "unknown"
license = "unknown"
summary = "x"
[runner]
kind = "mcp"
command = "x-mcp"
per_subject = "skill"
[permissions]
spawns_mcp_servers = true
[output]
{output}
"#
        ))
        .unwrap()
    }

    fn raw(bytes: &str) -> RawOutput {
        RawOutput {
            subjects: vec![Subject {
                id: "s".into(),
                kind: SubjectKind::Skill,
                name: "s".into(),
                agents: vec![],
            }],
            exit_code: None,
            bytes: bytes.as_bytes().to_vec(),
            notes: vec![],
            failed: false,
            tool: Some("scan".into()),
        }
    }

    #[test]
    fn sarif_holdmap_json_field_maps_and_text_are_all_read() {
        let m = manifest("format = \"auto\"");
        let sarif = include_str!("../../tests/fixtures/semgrep.sarif");
        assert!(!AutoParser
            .parse(&raw(sarif), &m)
            .unwrap()
            .findings
            .is_empty());
        let hj = r#"{"findings": [{"rule_id": "r1", "severity": "high"}]}"#;
        assert_eq!(
            AutoParser.parse(&raw(hj), &m).unwrap().findings[0].rule_id,
            "r1"
        );
        let mapped = manifest(
            "format = \"auto\"\nitems = \"$.issues[*]\"\nrule_id = \"code\"\nseverity = \"level\"\ntitle = \"msg\"",
        );
        let custom = r#"{"issues": [{"code": "X1", "level": "low", "msg": "Hi"}]}"#;
        let p = AutoParser.parse(&raw(custom), &mapped).unwrap();
        assert_eq!(p.findings[0].rule_id, "X1");
        let text = AutoParser
            .parse(&raw(&"looks fine   \n".repeat(100)), &m)
            .unwrap();
        assert!(text.findings.is_empty());
        assert_eq!(text.details[0].label, "Output of scan");
        assert!(text.details[0].value.starts_with("looks fine looks fine"));
        assert!(text.details[0].value.chars().count() <= EXCERPT + 1);
    }
}
