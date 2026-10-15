//! Hook-specific lifecycle projection. Source payloads are never retained.

use super::metadata::{
    id_parts, integer, json_size, mcp_server_from_tool, mcp_server_name, provider_label, safe_name,
    text, validate_span, MAX_DURATION_MS,
};
use crate::{ActivityError, ActivityKind, ActivitySpan, ActivityStatus, Provider, Source};
use serde_json::Value;

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
        mcp_server: if kind == ActivityKind::Tool {
            if provider == Provider::Claude && value.get("mcp_server").is_some() {
                mcp_server_name(
                    value
                        .get("mcp_server")
                        .and_then(|server| server.get("name"))
                        .and_then(Value::as_str),
                )
            } else {
                mcp_server_from_tool(&name)
            }
        } else {
            None
        },
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
