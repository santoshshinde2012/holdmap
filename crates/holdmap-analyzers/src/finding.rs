//! The normalised result every analyzer is mapped into. Text is capped and secret-masked
//! when it is built, so nothing an analyzer prints reaches a surface unfiltered.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Longest finding title, in characters.
pub const MAX_TITLE: usize = 120;
/// Longest fix hint, in characters.
pub const MAX_FIX: usize = 300;
/// Longest detail value, in characters.
pub const MAX_DETAIL: usize = 160;
/// Findings kept per subject; the rest are counted in a note.
pub const MAX_FINDINGS_PER_SUBJECT: usize = 200;
const MAX_REFS: usize = 5;
const MAX_REF_LEN: usize = 300;
const MAX_ID: usize = 80;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Worth knowing, nothing to fix.
    Info,
    /// Minor.
    Low,
    /// Should be looked at.
    Medium,
    /// Should be fixed.
    High,
    /// Fix before using the subject.
    Critical,
}

impl Severity {
    /// Map a producer's label (`HIGH`, `error`, `moderate`…). Unknown labels are `None`.
    pub fn from_label(label: &str) -> Option<Self> {
        match label.trim().to_ascii_lowercase().as_str() {
            "critical" | "crit" => Some(Self::Critical),
            "high" | "error" => Some(Self::High),
            "medium" | "moderate" | "warning" | "warn" => Some(Self::Medium),
            "low" | "note" => Some(Self::Low),
            "info" | "informational" | "none" => Some(Self::Info),
            _ => None,
        }
    }

    /// Map a CVSS-like 0–10 score (SARIF `security-severity`).
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s >= 9.0 => Self::Critical,
            s if s >= 7.0 => Self::High,
            s if s >= 4.0 => Self::Medium,
            s if s > 0.0 => Self::Low,
            _ => Self::Info,
        }
    }

    /// Lowercase label shared by text adapters.
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// What a finding is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    /// A skill folder (`SKILL.md` and its files).
    Skill,
    /// A package an MCP definition launches.
    Package,
    /// An agent as a whole.
    Agent,
}

/// One thing an analyzer looked at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    /// Stable id: the skill's capability id, or `package:<ecosystem>/<name>@<version>`.
    pub id: String,
    /// What it is.
    pub kind: SubjectKind,
    /// Display name.
    pub name: String,
    /// Catalog ids of the agents it belongs to, e.g. `claude-code`.
    #[serde(default)]
    pub agents: Vec<String>,
}

/// Where in the subject a finding points, relative to the subject's folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// Relative file path; absolute or escaping paths are dropped.
    pub file: Option<String>,
    /// 1-based line.
    pub line: Option<u32>,
}

/// Whether the user has dealt with a finding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingState {
    /// Needs a look.
    #[default]
    Open,
    /// Looked at and accepted; stays hidden until it changes.
    Reviewed,
    /// Hidden until a time.
    Snoozed,
}

/// One normalised finding. Never carries matched text, snippets or evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable identity across runs (see [`fingerprint`]).
    pub fingerprint: String,
    /// The analyzer that produced it.
    pub analyzer: String,
    /// The producer's rule id.
    pub rule_id: String,
    /// The producer's category, if any.
    pub category: Option<String>,
    /// How serious it is.
    pub severity: Severity,
    /// Short title, at most [`MAX_TITLE`] characters.
    pub title: String,
    /// The subject's id.
    pub subject: String,
    /// Where it points.
    #[serde(default)]
    pub location: Location,
    /// Fix hint, at most [`MAX_FIX`] characters.
    pub fix: Option<String>,
    /// `https` reference links.
    #[serde(default)]
    pub refs: Vec<String>,
    /// Review state.
    #[serde(default)]
    pub state: FindingState,
}

/// A labelled fact about a subject (a risk score, a recommendation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detail {
    /// The subject's id.
    pub subject: String,
    /// Short label.
    pub label: String,
    /// Value, at most [`MAX_DETAIL`] characters.
    pub value: String,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Every subject was analysed.
    Complete,
    /// Some subjects or files were not analysed; see the notes.
    Partial,
    /// Nothing usable came back.
    Failed,
}

/// The latest result of one analyzer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzerResult {
    /// Analyzer id.
    pub analyzer: String,
    /// Version reported by the analyzer, if known.
    pub analyzer_version: Option<String>,
    /// When the run finished (Unix epoch ms).
    pub run_at_ms: u64,
    /// How it ended.
    pub status: RunStatus,
    /// Share of the input analysed (0–1), when the producer reports it.
    pub coverage: Option<f32>,
    /// What was analysed.
    pub subjects: Vec<Subject>,
    /// Findings, most severe first.
    pub findings: Vec<Finding>,
    /// Labelled facts.
    #[serde(default)]
    pub details: Vec<Detail>,
    /// Short plain-language notes (timeouts, caps, skipped subjects). Never producer output.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Hash of the input this result covers; a different hash means the subjects changed.
    #[serde(default)]
    pub input_signature: Option<String>,
    /// How long the run took, in milliseconds.
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// MCP analyzers: calls per tool.
    #[serde(default)]
    pub tool_calls: Vec<ToolCalls>,
}

/// How often one MCP tool was called in a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCalls {
    /// Tool name.
    pub tool: String,
    /// Calls made.
    pub calls: usize,
    /// Calls that failed.
    pub failed: usize,
}

impl AnalyzerResult {
    /// Open findings at or above `min`.
    pub fn open_at_least(&self, min: Severity) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(move |f| f.state == FindingState::Open && f.severity >= min)
    }
}

/// Stable finding identity: the analyzer, rule and subject, plus the producer's own
/// fingerprint when it has one, else the location. Per-run ids must never be passed here.
pub fn fingerprint(
    analyzer: &str,
    rule_id: &str,
    subject: &str,
    producer: Option<&str>,
    location: &Location,
) -> String {
    let mut hash = Sha256::new();
    for part in [analyzer, rule_id, subject] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    match producer.filter(|p| !p.trim().is_empty()) {
        Some(p) => hash.update(p.trim().as_bytes()),
        None => {
            hash.update(location.file.as_deref().unwrap_or("").as_bytes());
            hash.update([0]);
            hash.update(location.line.unwrap_or(0).to_le_bytes());
        }
    }
    hex(&hash.finalize()[..16])
}

/// Lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Mask secrets, drop control characters, collapse whitespace and cap at `max` characters.
pub fn clean_text(raw: &str, max: usize) -> String {
    let one_line: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let masked = holdmap_core::redact::line(&one_line);
    let printable: String = masked.chars().filter(|c| !c.is_control()).collect();
    truncate(printable.trim(), max)
}

/// A short identifier (rule id, category): printable ASCII-ish, capped.
pub fn clean_id(raw: &str) -> String {
    let kept: String = raw
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/'))
        .collect();
    truncate(&kept, MAX_ID)
}

/// Keep a relative, non-escaping path inside the subject; anything else is dropped.
pub fn clean_relative_path(raw: &str) -> Option<String> {
    let path = raw.trim().replace('\\', "/");
    let path = path.strip_prefix("./").unwrap_or(&path);
    let escapes = path.split('/').any(|part| part == "..");
    let absolute = path.starts_with('/') || path.chars().nth(1) == Some(':');
    if path.is_empty() || escapes || absolute || path.contains("://") {
        return None;
    }
    Some(clean_text(path, 200))
}

/// Keep at most a few `https` links without credentials.
pub fn clean_refs<'a>(raw: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    raw.into_iter()
        .map(str::trim)
        .filter(|r| r.starts_with("https://") && r.len() <= MAX_REF_LEN)
        .filter(|r| !r[8..].split('/').next().unwrap_or("").contains('@'))
        .filter(|r| !r.chars().any(|c| c.is_whitespace() || c.is_control()))
        .map(str::to_string)
        .take(MAX_REFS)
        .collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_maps_producer_labels_and_scores() {
        assert_eq!(Severity::from_label("CRITICAL"), Some(Severity::Critical));
        assert_eq!(Severity::from_label("error"), Some(Severity::High));
        assert_eq!(Severity::from_label("warning"), Some(Severity::Medium));
        assert_eq!(Severity::from_label("note"), Some(Severity::Low));
        assert_eq!(Severity::from_label("weird"), None);
        assert_eq!(Severity::from_score(9.8), Severity::Critical);
        assert_eq!(Severity::from_score(7.0), Severity::High);
        assert_eq!(Severity::from_score(5.5), Severity::Medium);
        assert_eq!(Severity::from_score(0.1), Severity::Low);
        assert!(Severity::Critical > Severity::High);
    }

    #[test]
    fn fingerprint_prefers_the_producer_value_and_ignores_location_then() {
        let a = Location {
            file: Some("SKILL.md".into()),
            line: Some(3),
        };
        let b = Location {
            file: Some("SKILL.md".into()),
            line: Some(9),
        };
        let one = fingerprint("x", "R1", "skill:a", Some("abc"), &a);
        assert_eq!(one, fingerprint("x", "R1", "skill:a", Some("abc"), &b));
        assert_ne!(one, fingerprint("x", "R1", "skill:b", Some("abc"), &a));
        assert_ne!(
            fingerprint("x", "R1", "skill:a", None, &a),
            fingerprint("x", "R1", "skill:a", None, &b)
        );
        assert_eq!(one.len(), 32);
    }

    #[test]
    fn clean_text_masks_secrets_and_caps_length() {
        let text = clean_text("use\n\tAPI_KEY=sk-live-LEAKLEAKLEAKLEAK now", 120);
        assert!(!text.contains("sk-live-LEAK"), "{text}");
        assert!(!text.contains('\n'));
        let long = clean_text(&"a".repeat(500), 10);
        assert_eq!(long.chars().count(), 10);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn paths_must_stay_inside_the_subject() {
        assert_eq!(
            clean_relative_path("./scripts/run.sh").as_deref(),
            Some("scripts/run.sh")
        );
        assert_eq!(clean_relative_path("/etc/passwd"), None);
        assert_eq!(clean_relative_path("../x"), None);
        assert_eq!(clean_relative_path("C:\\x"), None);
        assert_eq!(clean_relative_path("file:///x"), None);
    }

    #[test]
    fn refs_keep_only_plain_https_links() {
        let refs = clean_refs([
            "https://osv.dev/vulnerability/GHSA-1",
            "http://insecure.example",
            "https://user:pw@example.com/x",
            "javascript:alert(1)",
        ]);
        assert_eq!(
            refs,
            vec!["https://osv.dev/vulnerability/GHSA-1".to_string()]
        );
    }

    #[test]
    fn ids_drop_anything_unexpected() {
        assert_eq!(clean_id("SC4 <b>osv</b>"), "SC4bosv/b");
        assert_eq!(clean_id(&"r".repeat(200)).chars().count(), MAX_ID);
    }
}
