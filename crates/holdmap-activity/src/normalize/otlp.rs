//! OTLP wire parsing and source-specific trace/log projection into shared metadata.

use super::metadata::{
    boolean, id_parts, integer, mcp_server_from_tool, optional_id, provider_label, pseudonymize_id,
    safe_name, text, validate_span, Batch,
};
use crate::{
    ActivityError, ActivityKind, ActivitySpan, ActivityStatus, Provider, Source, COLLECTOR_LIMITS,
};
use serde_json::Value;
use std::collections::HashMap;

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
    // The exported span status is its final outcome. A provider-specific attribute can
    // fill an Unset status, but must not override explicit OTLP success or failure.
    let status = match value
        .get("status")
        .and_then(|status| status.get("code"))
        .and_then(Value::as_u64)
    {
        Some(1) => ActivityStatus::Ok,
        Some(2) => ActivityStatus::Error,
        _ => match attr_bool(&attributes, "success") {
            Some(true) => ActivityStatus::Ok,
            Some(false) => ActivityStatus::Error,
            None => ActivityStatus::Unknown,
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
        mcp_server: (kind == ActivityKind::Tool)
            .then(|| mcp_server_from_tool(&name))
            .flatten(),
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
            "claude_code.tool_result"
                | "codex.tool_result"
                | "tool_result"
                | "claude_code.skill_activated"
                | "skill_activated"
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
    let skill = matches!(event, "claude_code.skill_activated" | "skill_activated");
    if skill && provider != Provider::Claude {
        return Err(ActivityError::InvalidMetadata);
    }
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
    let id = if skill {
        // The documented per-process event counter plus source time/session
        // identifies this activation without retaining a prompt or skill path.
        let sequence =
            attr_int(&attributes, "event.sequence").ok_or(ActivityError::InvalidMetadata)?;
        let time = event_time.ok_or(ActivityError::InvalidMetadata)?;
        if session.is_empty() {
            return Err(ActivityError::InvalidMetadata);
        }
        id_parts(
            "otel-log-skill",
            &[
                provider_label(provider),
                session,
                &sequence.to_string(),
                &time.to_string(),
                &attr_int(resource, "process.pid")
                    .unwrap_or_default()
                    .to_string(),
            ],
        )?
    } else {
        let call = attr_text(&attributes, "tool_use_id")
            .or_else(|| attr_text(&attributes, "call_id"))
            .ok_or(ActivityError::InvalidMetadata)?;
        id_parts("otel-log-tool", &[provider_label(provider), session, call])?
    };
    let end = event_time
        .or_else(|| nano_ms(value.get("observedTimeUnixNano")))
        .unwrap_or(received_ms);
    let duration = (!skill)
        .then(|| attr_int(&attributes, "duration_ms"))
        .flatten();
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
    let status = match (!skill)
        .then(|| attr_bool(&attributes, "success"))
        .flatten()
    {
        Some(true) => ActivityStatus::Ok,
        Some(false) => ActivityStatus::Error,
        None => ActivityStatus::Unknown,
    };
    let name = if skill {
        safe_name(
            attr_text(&attributes, "skill.name")
                .filter(|name| !matches!(*name, "custom_skill" | "third-party")),
            "Skill",
        )
    } else {
        safe_name(
            attr_text(&attributes, "tool_name_safe")
                .or_else(|| attr_text(&attributes, "tool_name")),
            "Tool",
        )
    };
    validate_span(ActivitySpan {
        id,
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
        kind: if skill {
            ActivityKind::Skill
        } else {
            ActivityKind::Tool
        },
        mcp_server: (!skill).then(|| mcp_server_from_tool(&name)).flatten(),
        name,
        start_at_ms: event_time
            .zip(duration)
            .and_then(|(end, duration)| end.checked_sub(duration)),
        end_at_ms: Some(end),
        duration_ms: duration,
        status,
        input_bytes: (!skill)
            .then(|| attr_int(&attributes, "tool_input_size_bytes"))
            .flatten(),
        output_bytes: (!skill)
            .then(|| attr_int(&attributes, "tool_result_size_bytes"))
            .flatten(),
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
