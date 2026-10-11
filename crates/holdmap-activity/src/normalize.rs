use crate::{
    ActivityError, ActivityKind, ActivitySpan, ActivityStatus, Provider, Source, COLLECTOR_LIMITS,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;

const MAX_ID_BYTES: usize = 256;
const MAX_NAME_BYTES: usize = 96;
const MAX_DURATION_MS: u64 = 31 * 24 * 60 * 60 * 1_000;
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
    Ok(format!("hm1_{:x}", digest.finalize()))
}

fn valid_raw_id(raw: &str) -> bool {
    !raw.is_empty() && raw.len() <= MAX_ID_BYTES && !raw.chars().any(unsafe_character)
}

fn unsafe_character(character: char) -> bool {
    character.is_control()
        || matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

fn safe_name(value: Option<&str>, fallback: &str) -> String {
    let Some(value) = value else {
        return fallback.into();
    };
    if matches!(
        value,
        "Tool (name redacted)" | "Agent (name redacted)" | "Activity (name redacted)"
    ) {
        return value.to_owned();
    }
    let lowered = value.to_ascii_lowercase();
    if value.is_empty()
        || value.len() > MAX_NAME_BYTES
        || value
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, ' ' | '_' | '-' | '.' | ':'))
        || lowered.starts_with("sk-")
        || lowered.starts_with("xox")
        || lowered.starts_with("ghp_")
        || lowered.starts_with("gho_")
        || lowered.starts_with("github_pat_")
        || lowered.starts_with("akia")
        || lowered.starts_with("bearer ")
        || (lowered.starts_with("eyj") && value.len() > 24)
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

fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|value| valid_raw_id(value))
}

fn integer(value: &Value) -> Option<u64> {
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

fn boolean(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| match value.as_str()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

fn optional_id(value: Option<&Value>, domain: &str) -> Option<String> {
    pseudonymize_id(domain, text(value?)?).ok()
}

fn id_parts(domain: &str, parts: &[&str]) -> Result<String, ActivityError> {
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
    Ok(format!("hm1_{:x}", digest.finalize()))
}

fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Codex => "codex",
        Provider::Claude => "claude",
        Provider::OpenTelemetry => "opentelemetry",
        Provider::Holdmap => "holdmap",
    }
}

fn hook_outcome(provider: Provider, event: &str, value: &Value) -> ActivityStatus {
    match event {
        "PreToolUse" => ActivityStatus::Requested,
        "PostToolUseFailure" if provider == Provider::Claude => {
            if value.get("is_interrupt").and_then(Value::as_bool) == Some(true) {
                ActivityStatus::Cancelled
            } else {
                ActivityStatus::Error
            }
        }
        "PostToolUse" if provider == Provider::Claude => ActivityStatus::Ok,
        "PostToolUse" => {
            let response = value.get("tool_response").unwrap_or(&Value::Null);
            explicit_outcome(response).unwrap_or(ActivityStatus::Unknown)
        }
        _ => ActivityStatus::Unknown,
    }
}

fn explicit_outcome(response: &Value) -> Option<ActivityStatus> {
    if let Some(success) = response.get("success").and_then(Value::as_bool) {
        return Some(if success {
            ActivityStatus::Ok
        } else {
            ActivityStatus::Error
        });
    }
    if let Some(error) = response.get("isError").and_then(Value::as_bool) {
        return Some(if error {
            ActivityStatus::Error
        } else {
            ActivityStatus::Ok
        });
    }
    if let Some(code) = response
        .get("exit_code")
        .or_else(|| response.get("exitCode"))
        .and_then(Value::as_i64)
    {
        return Some(if code == 0 {
            ActivityStatus::Ok
        } else {
            ActivityStatus::Error
        });
    }
    None
}

/// Projects official hook input into metadata. It never opens transcript paths or copies payloads.
pub fn normalize_hook(
    provider: Provider,
    value: &Value,
    received_at_ms: u64,
) -> Result<Vec<ActivitySpan>, ActivityError> {
    if !matches!(provider, Provider::Codex | Provider::Claude) || !value.is_object() {
        return Err(ActivityError::InvalidMetadata);
    }
    json_size(value)?;
    let event = value
        .get("hook_event_name")
        .and_then(Value::as_str)
        .ok_or(ActivityError::InvalidMetadata)?;
    let session = value
        .get("session_id")
        .and_then(text)
        .ok_or(ActivityError::InvalidMetadata)?;
    let session_id = id_parts("hook-session", &[provider_label(provider), session])?;
    let (id, kind, status, name, started, ended) = match event {
        "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => {
            if event == "PostToolUseFailure" && provider == Provider::Codex {
                return Err(ActivityError::InvalidMetadata);
            }
            let call = value
                .get("tool_use_id")
                .and_then(text)
                .ok_or(ActivityError::InvalidMetadata)?;
            (
                id_parts("hook-tool", &[provider_label(provider), session, call])?,
                ActivityKind::Tool,
                hook_outcome(provider, event, value),
                safe_name(value.get("tool_name").and_then(Value::as_str), "Tool"),
                (event == "PreToolUse").then_some(received_at_ms),
                (event != "PreToolUse").then_some(received_at_ms),
            )
        }
        "SubagentStart" | "SubagentStop" => {
            let agent = value
                .get("agent_id")
                .and_then(text)
                .ok_or(ActivityError::InvalidMetadata)?;
            (
                id_parts("hook-agent", &[provider_label(provider), session, agent])?,
                ActivityKind::Agent,
                if event == "SubagentStart" {
                    ActivityStatus::Running
                } else {
                    ActivityStatus::Unknown
                },
                safe_name(value.get("agent_type").and_then(Value::as_str), "Agent"),
                (event == "SubagentStart").then_some(received_at_ms),
                (event == "SubagentStop").then_some(received_at_ms),
            )
        }
        "SessionStart" | "SessionEnd" => (
            id_parts(
                "hook-session-lifecycle",
                &[provider_label(provider), session],
            )?,
            ActivityKind::Agent,
            if event == "SessionStart" {
                ActivityStatus::Running
            } else {
                ActivityStatus::Unknown
            },
            "Agent session".into(),
            (event == "SessionStart").then_some(received_at_ms),
            (event == "SessionEnd").then_some(received_at_ms),
        ),
        _ => return Err(ActivityError::InvalidMetadata),
    };
    let span = ActivitySpan {
        id,
        trace_id: String::new(),
        parent_id: None,
        session_id: Some(session_id),
        agent_id: value
            .get("agent_id")
            .and_then(text)
            .map(|id| id_parts("hook-agent-id", &[provider_label(provider), session, id]))
            .transpose()?,
        parent_agent_id: value
            .get("parent_agent_id")
            .and_then(text)
            .map(|id| id_parts("hook-agent-id", &[provider_label(provider), session, id]))
            .transpose()?,
        provider,
        source: Source::Hook,
        kind,
        name,
        start_at_ms: started,
        end_at_ms: ended,
        duration_ms: value
            .get("duration_ms")
            .and_then(integer)
            .filter(|duration| *duration <= MAX_DURATION_MS),
        status,
        input_bytes: value
            .get("tool_input")
            .map(json_size)
            .transpose()?
            .map(|size| size as u64),
        output_bytes: value
            .get("tool_response")
            .map(json_size)
            .transpose()?
            .map(|size| size as u64),
        pid: None,
        process_started_at: None,
    };
    validate_span(span).map(|span| vec![span])
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

fn validate_span(mut span: ActivitySpan) -> Result<ActivitySpan, ActivityError> {
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
    Ok(span)
}

type Attributes<'a> = HashMap<&'a str, &'a Value>;

fn attributes(value: Option<&Value>) -> Result<Attributes<'_>, ActivityError> {
    let Some(value) = value else {
        return Ok(HashMap::new());
    };
    let values = value.as_array().ok_or(ActivityError::InvalidMetadata)?;
    if values.len() > 128 {
        return Err(ActivityError::LimitExceeded);
    }
    let mut result = HashMap::new();
    for entry in values {
        let key = entry
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| key.len() <= 128)
            .ok_or(ActivityError::InvalidMetadata)?;
        let value = entry.get("value").ok_or(ActivityError::InvalidMetadata)?;
        if value.as_object().is_none_or(|object| object.len() != 1)
            || !value.as_object().is_some_and(|object| {
                object.keys().all(|field| {
                    matches!(
                        field.as_str(),
                        "stringValue"
                            | "boolValue"
                            | "intValue"
                            | "doubleValue"
                            | "arrayValue"
                            | "kvlistValue"
                            | "bytesValue"
                    )
                })
            })
            || value
                .get("stringValue")
                .is_some_and(|value| !value.is_string())
            || value
                .get("boolValue")
                .is_some_and(|value| !value.is_boolean())
            || value
                .get("intValue")
                .is_some_and(|value| value.as_i64().is_none() && integer(value).is_none())
            || result.insert(key, value).is_some()
        {
            return Err(ActivityError::InvalidMetadata);
        }
    }
    Ok(result)
}

fn attr_text<'a>(attributes: &Attributes<'a>, key: &str) -> Option<&'a str> {
    text(attributes.get(key)?.get("stringValue")?)
}

fn attr_int(attributes: &Attributes<'_>, key: &str) -> Option<u64> {
    let value = attributes.get(key)?;
    integer(value.get("intValue").or_else(|| value.get("stringValue"))?)
}

fn attr_bool(attributes: &Attributes<'_>, key: &str) -> Option<bool> {
    let value = attributes.get(key)?;
    boolean(
        value
            .get("boolValue")
            .or_else(|| value.get("stringValue"))?,
    )
}

fn otlp_provider(resource: &Attributes<'_>, scope: Option<&str>) -> Provider {
    match attr_text(resource, "service.name").or(scope).unwrap_or("") {
        "codex" | "codex_cli" | "codex-cli" | "codex_rs" => Provider::Codex,
        "claude_code" | "claude-code" | "claude-code-cli" => Provider::Claude,
        "holdmap" => Provider::Holdmap,
        _ => Provider::OpenTelemetry,
    }
}

fn wire_id(value: Option<&Value>, length: usize) -> Option<String> {
    let value = value?.as_str()?;
    (value.len() == length
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().any(|byte| byte != b'0'))
    .then(|| value.to_ascii_lowercase())
}

fn trace_identity(value: &Value) -> Option<(String, String, Option<String>)> {
    let trace = wire_id(value.get("traceId"), 32)?;
    let span = wire_id(value.get("spanId"), 16)?;
    let parent = wire_id(value.get("parentSpanId"), 16)
        .and_then(|parent| id_parts("otel-span", &[&trace, &parent]).ok());
    Some((
        pseudonymize_id("otel-trace", &trace).ok()?,
        id_parts("otel-span", &[&trace, &span]).ok()?,
        parent,
    ))
}

fn nano_ms(value: Option<&Value>) -> Option<u64> {
    integer(value?)
        .filter(|time| *time > 0)
        .map(|time| time / 1_000_000)
}

fn trace_span(
    value: &Value,
    resource: &Attributes<'_>,
    provider: Provider,
) -> Result<ActivitySpan, ActivityError> {
    let attributes = attributes(value.get("attributes"))?;
    let raw_name = value.get("name").and_then(Value::as_str).unwrap_or("");
    let operation = attr_text(&attributes, "gen_ai.operation.name");
    let method = attr_text(&attributes, "mcp.method.name");
    let (kind, name) = match raw_name {
        "claude_code.tool.execution" => (ActivityKind::Step, "Tool execution".into()),
        "claude_code.tool.blocked_on_user" => (ActivityKind::Step, "Permission wait".into()),
        "claude_code.hook" | "claude_code.hook.execution" => (ActivityKind::Step, "Hook".into()),
        "claude_code.interaction" => (ActivityKind::Agent, "Agent turn".into()),
        "claude_code.tool" => (
            ActivityKind::Tool,
            safe_name(
                attr_text(&attributes, "tool_name_safe")
                    .or_else(|| attr_text(&attributes, "tool_name")),
                "Tool",
            ),
        ),
        "claude_code.llm_request" => (ActivityKind::Model, "Model request".into()),
        _ => match operation {
            Some("execute_tool") => (
                ActivityKind::Tool,
                safe_name(attr_text(&attributes, "gen_ai.tool.name"), "Tool"),
            ),
            Some("invoke_agent" | "create_agent" | "invoke_workflow" | "plan") => (
                ActivityKind::Agent,
                safe_name(attr_text(&attributes, "gen_ai.agent.name"), "Agent"),
            ),
            Some("chat" | "generate_content" | "text_completion" | "embeddings") => {
                (ActivityKind::Model, "Model request".into())
            }
            _ if method == Some("tools/call") => (
                ActivityKind::Tool,
                safe_name(attr_text(&attributes, "gen_ai.tool.name"), "Tool"),
            ),
            _ => return Err(ActivityError::InvalidMetadata),
        },
    };
    let (trace_id, id, parent_id) = trace_identity(value).ok_or(ActivityError::InvalidMetadata)?;
    let start_at_ms =
        Some(nano_ms(value.get("startTimeUnixNano")).ok_or(ActivityError::InvalidMetadata)?);
    let end_at_ms = nano_ms(value.get("endTimeUnixNano")).ok_or(ActivityError::InvalidMetadata)?;
    let status = match attr_bool(&attributes, "success") {
        Some(true) => ActivityStatus::Ok,
        Some(false) => ActivityStatus::Error,
        None => match value
            .get("status")
            .and_then(|status| status.get("code"))
            .and_then(Value::as_u64)
        {
            Some(1) => ActivityStatus::Ok,
            Some(2) => ActivityStatus::Error,
            _ => ActivityStatus::Unknown,
        },
    };
    validate_span(ActivitySpan {
        id,
        trace_id,
        parent_id,
        session_id: optional_id(
            attributes
                .get("gen_ai.conversation.id")
                .and_then(|value| value.get("stringValue"))
                .or_else(|| {
                    attributes
                        .get("session.id")
                        .and_then(|value| value.get("stringValue"))
                }),
            "otel-session",
        ),
        agent_id: optional_id(
            attributes
                .get("gen_ai.agent.id")
                .and_then(|value| value.get("stringValue"))
                .or_else(|| {
                    attributes
                        .get("agent_id")
                        .and_then(|value| value.get("stringValue"))
                }),
            "otel-agent",
        ),
        parent_agent_id: optional_id(
            attributes
                .get("parent_agent_id")
                .and_then(|value| value.get("stringValue")),
            "otel-agent",
        ),
        provider,
        source: if method.is_some() {
            Source::Mcp
        } else {
            Source::Otlp
        },
        kind,
        name,
        start_at_ms,
        end_at_ms: Some(end_at_ms),
        duration_ms: attr_int(&attributes, "duration_ms")
            .or_else(|| start_at_ms.and_then(|start| end_at_ms.checked_sub(start))),
        status,
        input_bytes: attr_int(&attributes, "tool_input_size_bytes"),
        output_bytes: attr_int(&attributes, "tool_result_size_bytes"),
        pid: attr_int(resource, "process.pid").and_then(|pid| u32::try_from(pid).ok()),
        process_started_at: attr_int(resource, "holdmap.process_started_at"),
    })
}

fn log_span(
    value: &Value,
    resource: &Attributes<'_>,
    provider: Provider,
    received_ms: u64,
) -> Result<ActivitySpan, ActivityError> {
    let attributes = attributes(value.get("attributes"))?;
    let supported = |event: &&str| {
        matches!(
            *event,
            "claude_code.tool_result" | "codex.tool_result" | "tool_result"
        )
    };
    let event = value
        .get("eventName")
        .and_then(Value::as_str)
        .filter(supported)
        // tracing-opentelemetry uses a source-location eventName; Codex supplies event.name.
        .or_else(|| attr_text(&attributes, "event.name").filter(supported))
        .ok_or(ActivityError::InvalidMetadata)?;
    let provider = if event.starts_with("claude_code.") {
        Provider::Claude
    } else if event.starts_with("codex.") {
        Provider::Codex
    } else {
        provider
    };
    let call = attr_text(&attributes, "tool_use_id")
        .or_else(|| attr_text(&attributes, "call_id"))
        .ok_or(ActivityError::InvalidMetadata)?;
    let session = attr_text(&attributes, "conversation.id")
        .or_else(|| attr_text(&attributes, "session.id"))
        .or_else(|| attr_text(&attributes, "session_id"))
        .or_else(|| attr_text(resource, "conversation.id"))
        .or_else(|| attr_text(resource, "session.id"))
        .unwrap_or("");
    let event_time = nano_ms(value.get("timeUnixNano")).or_else(|| {
        let timestamp = attr_text(&attributes, "event.timestamp")?;
        if timestamp.len() > 40 {
            return None;
        }
        humantime::parse_rfc3339(timestamp)
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis()
            .try_into()
            .ok()
    });
    let end = event_time
        .or_else(|| nano_ms(value.get("observedTimeUnixNano")))
        .unwrap_or(received_ms);
    let duration = attr_int(&attributes, "duration_ms");
    let raw_trace = wire_id(value.get("traceId"), 32);
    let trace_id = raw_trace
        .as_deref()
        .map(|trace| pseudonymize_id("otel-trace", trace))
        .transpose()?
        .unwrap_or_default();
    // A log's spanId is its explicit emission context, not a guessed tool execution span.
    let parent_id = raw_trace
        .as_deref()
        .zip(wire_id(value.get("spanId"), 16))
        .map(|(trace, span)| id_parts("otel-span", &[trace, &span]))
        .transpose()?;
    let status = match attr_bool(&attributes, "success") {
        Some(true) => ActivityStatus::Ok,
        Some(false) => ActivityStatus::Error,
        None => ActivityStatus::Unknown,
    };
    validate_span(ActivitySpan {
        id: id_parts("otel-log-tool", &[provider_label(provider), session, call])?,
        trace_id,
        parent_id,
        session_id: (!session.is_empty())
            .then(|| id_parts("hook-session", &[provider_label(provider), session]))
            .transpose()?,
        agent_id: optional_id(
            attributes
                .get("agent_id")
                .and_then(|value| value.get("stringValue")),
            "otel-agent",
        ),
        parent_agent_id: optional_id(
            attributes
                .get("parent_agent_id")
                .and_then(|value| value.get("stringValue")),
            "otel-agent",
        ),
        provider,
        source: Source::Otlp,
        kind: ActivityKind::Tool,
        name: safe_name(
            attr_text(&attributes, "tool_name_safe")
                .or_else(|| attr_text(&attributes, "tool_name")),
            "Tool",
        ),
        start_at_ms: event_time
            .zip(duration)
            .and_then(|(end, duration)| end.checked_sub(duration)),
        end_at_ms: Some(end),
        duration_ms: duration,
        status,
        input_bytes: attr_int(&attributes, "tool_input_size_bytes"),
        output_bytes: attr_int(&attributes, "tool_result_size_bytes"),
        pid: attr_int(resource, "process.pid").and_then(|pid| u32::try_from(pid).ok()),
        process_started_at: attr_int(resource, "holdmap.process_started_at"),
    })
}

pub(crate) fn otlp(value: &Value, traces: bool, received_ms: u64) -> Result<Batch, ActivityError> {
    let groups = value
        .get(if traces {
            "resourceSpans"
        } else {
            "resourceLogs"
        })
        .and_then(Value::as_array)
        .ok_or(ActivityError::InvalidMetadata)?;
    if groups.len() > COLLECTOR_LIMITS.max_records_per_request {
        return Err(ActivityError::LimitExceeded);
    }
    let mut batch = Batch {
        spans: Vec::new(),
        dropped: 0,
    };
    let mut count = 0;
    for group in groups {
        let resource = attributes(
            group
                .get("resource")
                .and_then(|value| value.get("attributes")),
        )?;
        let scopes = group
            .get(if traces { "scopeSpans" } else { "scopeLogs" })
            .and_then(Value::as_array)
            .ok_or(ActivityError::InvalidMetadata)?;
        if scopes.len() > COLLECTOR_LIMITS.max_records_per_request {
            return Err(ActivityError::LimitExceeded);
        }
        for scope in scopes {
            let provider = otlp_provider(
                &resource,
                scope
                    .get("scope")
                    .and_then(|value| value.get("name"))
                    .and_then(Value::as_str),
            );
            let records = scope
                .get(if traces { "spans" } else { "logRecords" })
                .and_then(Value::as_array)
                .ok_or(ActivityError::InvalidMetadata)?;
            for record in records {
                count += 1;
                if count > COLLECTOR_LIMITS.max_records_per_request {
                    return Err(ActivityError::LimitExceeded);
                }
                let result = if traces {
                    trace_span(record, &resource, provider)
                } else {
                    log_span(record, &resource, provider, received_ms)
                };
                match result {
                    Ok(span) => batch.spans.push(span),
                    Err(_) => batch.dropped += 1,
                }
            }
        }
    }
    Ok(batch)
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

fn json_size(value: &Value) -> Result<usize, ActivityError> {
    let mut writer = CountingWriter(0);
    serde_json::to_writer(&mut writer, value).map_err(|_| ActivityError::LimitExceeded)?;
    Ok(writer.0)
}
