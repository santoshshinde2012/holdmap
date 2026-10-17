//! Shared metadata invariants, sanitization, correlation identities and bounded projection.

use crate::{
    ActivityError, ActivityKind, ActivitySpan, ActivityStatus, Provider, Source, COLLECTOR_LIMITS,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Write;

const MAX_ID_BYTES: usize = 256;
const MAX_NAME_BYTES: usize = 96;
pub(super) const MAX_DURATION_MS: u64 = 31 * 24 * 60 * 60 * 1_000;
const MAX_TIMESTAMP_MS: u64 = 253_402_300_799_999;

/// Domain-separated, deterministic correlation pseudonym. No raw identifier is retained.
/// This permits correlation; it is not a claim that low-entropy identifiers are anonymized.
pub fn pseudonymize_id(namespace: &str, raw: &str) -> Result<String, ActivityError> {
    if namespace.len() > 64 || !valid_raw_id(raw) {
        return Err(ActivityError::InvalidMetadata);
    }
    let mut digest = Sha256::new();
    digest.update(b"holdmap-activity-v1\0");
    digest.update(namespace.as_bytes());
    digest.update([0]);
    digest.update(raw.as_bytes());
    Ok(format!("hm1_{}", hex::encode(digest.finalize())))
}

fn valid_raw_id(raw: &str) -> bool {
    !raw.is_empty() && raw.len() <= MAX_ID_BYTES && !raw.chars().any(unsafe_character)
}

fn unsafe_character(character: char) -> bool {
    character.is_control()
        || matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

fn name_separator(byte: u8) -> bool {
    matches!(byte, b' ' | b'_' | b'-' | b'.' | b':')
}

fn credential_prefix(value: &str) -> bool {
    ["sk-", "xox", "ghp_", "gho_", "github_pat_", "akia", "bearer "]
        .iter()
        .any(|prefix| value.starts_with(prefix))
        || (value.starts_with("eyj") && value.len() > 24)
        // Recognize the common 20-character STS access-key ID shape so ordinary
        // labels such as AsiaHelper or asia_search remain useful source names.
        || (value.starts_with("asia")
            && value
                .as_bytes()
                .get(..20)
                .is_some_and(|key| key.iter().all(u8::is_ascii_alphanumeric))
            && value.as_bytes().get(20).is_none_or(|byte| name_separator(*byte)))
}

fn has_credential_component(value: &str) -> bool {
    // MCP and namespaced tool labels prepend server names to a tool identifier.
    // Prefix checks must apply after separators too, before any label is retained.
    value.char_indices().any(|(index, _)| {
        (index == 0 || name_separator(value.as_bytes()[index - 1]))
            && credential_prefix(&value[index..])
    })
}

pub(super) fn safe_name(value: Option<&str>, fallback: &str) -> String {
    let Some(value) = value else {
        return fallback.into();
    };
    if matches!(
        value,
        "Tool (name redacted)"
            | "Agent (name redacted)"
            | "Activity (name redacted)"
            | "Skill (name redacted)"
    ) {
        return value.to_owned();
    }
    let lowered = value.to_ascii_lowercase();
    if value.is_empty()
        || value.len() > MAX_NAME_BYTES
        || value
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, ' ' | '_' | '-' | '.' | ':'))
        || has_credential_component(&lowered)
        || (value.contains(' ')
            && !matches!(
                value,
                "Tool execution"
                    | "Permission wait"
                    | "Agent turn"
                    | "Agent session"
                    | "Model request"
                    | "Tool (name redacted)"
                    | "Agent (name redacted)"
                    | "Activity (name redacted)"
            ))
        || value
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|word| word.len() > 32)
    {
        return format!("{fallback} (name redacted)");
    }
    value.to_owned()
}

/// Retain an exact identifier only when the shared label policy leaves it unchanged.
pub(super) fn mcp_server_name(value: Option<&str>) -> Option<String> {
    let value = value?;
    (!matches!(value, "custom" | "mcp_tool" | "mcp_other")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
        && value.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && safe_name(Some(value), "Tool") == value)
        .then(|| value.to_owned())
}

/// Claude's documented tool label convention is not a universal MCP wire format.
/// An extra double separator makes the server/tool split ambiguous, so omit it.
pub(super) fn mcp_server_from_tool(value: &str) -> Option<String> {
    if safe_name(Some(value), "Tool") != value {
        return None;
    }
    let (server, tool) = value.strip_prefix("mcp__")?.split_once("__")?;
    if server.is_empty() || tool.is_empty() || tool.starts_with('_') || tool.contains("__") {
        return None;
    }
    mcp_server_name(Some(server))
}

pub(super) fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|value| valid_raw_id(value))
}

pub(super) fn integer(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_str()
            .filter(|value| {
                !value.is_empty() && value.len() <= 20 && value.bytes().all(|b| b.is_ascii_digit())
            })?
            .parse()
            .ok()
    })
}

pub(super) fn boolean(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| match value.as_str()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

pub(super) fn optional_id(value: Option<&Value>, domain: &str) -> Option<String> {
    pseudonymize_id(domain, text(value?)?).ok()
}

pub(super) fn id_parts(domain: &str, parts: &[&str]) -> Result<String, ActivityError> {
    // Length framing avoids collisions between tuples containing separators.
    let joined: String = parts
        .iter()
        .map(|part| format!("{}:{part}", part.len()))
        .collect();
    let mut digest = Sha256::new();
    digest.update(b"holdmap-activity-v1\0");
    digest.update(domain.as_bytes());
    digest.update([0]);
    digest.update(joined.as_bytes());
    Ok(format!("hm1_{}", hex::encode(digest.finalize())))
}

pub(super) fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Codex => "codex",
        Provider::Claude => "claude",
        Provider::OpenTelemetry => "opentelemetry",
        Provider::Holdmap => "holdmap",
    }
}

pub(crate) struct Batch {
    pub spans: Vec<ActivitySpan>,
    pub dropped: u64,
}

pub(crate) fn normalized(value: &Value) -> Result<Batch, ActivityError> {
    let records = value.as_array().ok_or(ActivityError::InvalidMetadata)?;
    if records.len() > COLLECTOR_LIMITS.max_records_per_request {
        return Err(ActivityError::LimitExceeded);
    }
    let mut batch = Batch {
        spans: Vec::new(),
        dropped: 0,
    };
    for record in records {
        match serde_json::from_value(record.clone())
            .map_err(|_| ActivityError::InvalidMetadata)
            .and_then(validate_span)
        {
            Ok(span) => batch.spans.push(span),
            Err(_) => batch.dropped += 1,
        }
    }
    Ok(batch)
}

fn hashed_id(id: &str) -> bool {
    id.len() == 68
        && id.starts_with("hm1_")
        && id[4..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub(super) fn validate_span(mut span: ActivitySpan) -> Result<ActivitySpan, ActivityError> {
    if !hashed_id(&span.id)
        || (!span.trace_id.is_empty() && !hashed_id(&span.trace_id))
        || [
            &span.parent_id,
            &span.session_id,
            &span.agent_id,
            &span.parent_agent_id,
        ]
        .iter()
        .any(|id| id.as_deref().is_some_and(|id| !hashed_id(id)))
        || span.parent_id.as_ref() == Some(&span.id)
        || (span.parent_id.is_some() && span.trace_id.is_empty())
        || (span.source == Source::Hook
            && !matches!(span.provider, Provider::Codex | Provider::Claude))
        || (span.kind == ActivityKind::Handoff
            && (span.agent_id.is_none() || span.parent_agent_id.is_none()))
        || (span.mcp_server.is_some() && span.kind != ActivityKind::Tool)
        || (span.kind == ActivityKind::Skill
            && (span.status != ActivityStatus::Unknown
                || span.start_at_ms.is_some()
                || span.end_at_ms.is_none()
                || span.duration_ms.is_some()
                || span.input_bytes.is_some()
                || span.output_bytes.is_some()))
        || span.start_at_ms.is_some_and(|time| time > MAX_TIMESTAMP_MS)
        || span.end_at_ms.is_some_and(|time| time > MAX_TIMESTAMP_MS)
        || span
            .duration_ms
            .is_some_and(|duration| duration > MAX_DURATION_MS)
        || matches!((span.start_at_ms, span.end_at_ms), (Some(start), Some(end)) if start > end)
        || span.pid == Some(0)
        || span
            .process_started_at
            .is_some_and(|time| time > MAX_TIMESTAMP_MS)
    {
        return Err(ActivityError::InvalidMetadata);
    }
    if span.end_at_ms.is_none()
        && matches!(
            span.status,
            ActivityStatus::Ok | ActivityStatus::Error | ActivityStatus::Cancelled
        )
    {
        return Err(ActivityError::InvalidMetadata);
    }
    if span.end_at_ms.is_some()
        && matches!(
            span.status,
            ActivityStatus::Requested | ActivityStatus::Running
        )
    {
        return Err(ActivityError::InvalidMetadata);
    }
    span.name = safe_name(Some(&span.name), "Activity");
    span.mcp_server = mcp_server_name(span.mcp_server.as_deref());
    Ok(span)
}

struct CountingWriter(usize);

impl Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|size| *size <= COLLECTOR_LIMITS.max_body_bytes)
            .ok_or_else(|| std::io::Error::other("limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn json_size(value: &Value) -> Result<usize, ActivityError> {
    let mut writer = CountingWriter(0);
    serde_json::to_writer(&mut writer, value).map_err(|_| ActivityError::LimitExceeded)?;
    Ok(writer.0)
}
