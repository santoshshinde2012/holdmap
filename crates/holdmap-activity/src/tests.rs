use crate::{normalize, store::ActivityStore, *};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

fn hook(event: &str, call: &str) -> Value {
    json!({"hook_event_name":event,"session_id":"fixture-session","tool_use_id":call,"tool_name":"Bash"})
}

fn span(id: &str) -> ActivitySpan {
    normalize_hook(Provider::Claude, &hook("PreToolUse", id), 100)
        .unwrap()
        .remove(0)
}

fn attribute(key: &str, value: Value) -> Value {
    json!({"key":key,"value":value})
}

fn trace_record(id: &str, parent: Option<&str>, name: &str) -> Value {
    let mut record = json!({
        "traceId":"0123456789abcdef0123456789abcdef","spanId":id,
        "name":name,"startTimeUnixNano":"1000000000","endTimeUnixNano":"1500000000",
        "attributes":[attribute("gen_ai.operation.name",json!({"stringValue":"execute_tool"})),attribute("gen_ai.tool.name",json!({"stringValue":"fixture_tool"}))]
    });
    if let Some(parent) = parent {
        record["parentSpanId"] = json!(parent);
    }
    record
}

fn traces(records: Vec<Value>) -> Value {
    json!({"resourceSpans":[{"resource":{"attributes":[attribute("service.name",json!({"stringValue":"claude_code"}))]},"scopeSpans":[{"spans":records}]}]})
}

fn logs(records: Vec<Value>) -> Value {
    json!({"resourceLogs":[{"resource":{"attributes":[]},"scopeLogs":[{"scope":{"name":"codex_cli"},"logRecords":records}]}]})
}

fn log_record(success: Option<Value>) -> Value {
    let mut attrs = vec![
        attribute("call_id", json!({"stringValue":"fixture-call"})),
        attribute("session_id", json!({"stringValue":"fixture-session"})),
        attribute("tool_name", json!({"stringValue":"fixture_tool"})),
    ];
    if let Some(success) = success {
        attrs.push(attribute("success", success));
    }
    json!({"eventName":"codex.tool_result","timeUnixNano":"2000000000","attributes":attrs})
}

#[test]
fn hooks_project_only_metadata_and_serialized_sizes() {
    let mut raw = hook("PreToolUse", "private-call-id");
    raw["cwd"] = json!("/private/fixture/project");
    raw["transcript_path"] = json!("/private/fixture/chat.jsonl");
    raw["tool_input"] = json!({"command":"secret-command","url":"https://private.invalid"});
    raw["messages"] = json!(["private prompt"]);
    let result = normalize_hook(Provider::Claude, &raw, 10).unwrap();
    let encoded = serde_json::to_string(&result).unwrap();
    for forbidden in [
        "private-call-id",
        "fixture-session",
        "secret-command",
        "private.invalid",
        "chat.jsonl",
        "private prompt",
    ] {
        assert!(!encoded.contains(forbidden), "retained {forbidden}");
    }
    assert_eq!(
        result[0].input_bytes,
        Some(serde_json::to_vec(&raw["tool_input"]).unwrap().len() as u64)
    );
    assert_eq!(result[0].status, ActivityStatus::Requested);
    assert!(result[0].trace_id.is_empty());
    assert!(result[0].parent_id.is_none());
    assert!(result[0].pid.is_none());
}

#[test]
fn credential_path_control_and_free_text_names_are_redacted() {
    for name in [
        "ghp_secret123",
        "github_pat_secret123",
        "sk-secret",
        "AKIAfixturekey",
        "Bearer token",
        "password secret123",
        "owner@example.com",
        "https://private.invalid",
        "/tmp/private",
        "tool\u{202e}txt",
        "\u{1b}[31mtool",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        let mut raw = hook("PreToolUse", "fixture-call");
        raw["tool_name"] = json!(name);
        let result = normalize_hook(Provider::Claude, &raw, 10).unwrap();
        assert_eq!(
            result[0].name, "Tool (name redacted)",
            "unredacted {name:?}"
        );
    }
    for name in ["Bash", "mcp__fixture__lookup", "fixture.search"] {
        let mut raw = hook("PreToolUse", "fixture-call");
        raw["tool_name"] = json!(name);
        assert_eq!(
            normalize_hook(Provider::Claude, &raw, 10).unwrap()[0].name,
            name
        );
    }
}

#[test]
fn correlation_hashes_are_domain_separated_and_bounded() {
    let a = pseudonymize_id("session", "fixture").unwrap();
    assert_eq!(a, pseudonymize_id("session", "fixture").unwrap());
    assert_ne!(a, pseudonymize_id("tool", "fixture").unwrap());
    assert_eq!(a.len(), 68);
    for raw in [
        String::new(),
        "a".repeat(257),
        "evil\nidentifier".into(),
        "evil\u{202e}identifier".into(),
    ] {
        assert!(pseudonymize_id("session", &raw).is_err());
    }
}

#[test]
fn codex_post_requires_explicit_outcome_and_does_not_assume_bash_success() {
    let mut raw = hook("PostToolUse", "fixture-call");
    for response in [
        json!("Exit code: 1"),
        json!({"output":"failed"}),
        Value::Null,
    ] {
        raw["tool_response"] = response;
        assert_eq!(
            normalize_hook(Provider::Codex, &raw, 50).unwrap()[0].status,
            ActivityStatus::Unknown
        );
    }
    for (response, expected) in [
        (json!({"success":true}), ActivityStatus::Ok),
        (json!({"isError":true}), ActivityStatus::Error),
        (json!({"exit_code":2}), ActivityStatus::Error),
        (json!({"exitCode":0}), ActivityStatus::Ok),
    ] {
        raw["tool_response"] = response;
        assert_eq!(
            normalize_hook(Provider::Codex, &raw, 50).unwrap()[0].status,
            expected
        );
    }
    raw["hook_event_name"] = json!("PostToolUseFailure");
    assert!(normalize_hook(Provider::Codex, &raw, 50).is_err());
}

#[test]
fn claude_explicit_failure_and_interrupt_are_distinct() {
    let mut raw = hook("PostToolUseFailure", "fixture-call");
    raw["error"] = json!("private failure details");
    assert_eq!(
        normalize_hook(Provider::Claude, &raw, 50).unwrap()[0].status,
        ActivityStatus::Error
    );
    raw["is_interrupt"] = json!(true);
    assert_eq!(
        normalize_hook(Provider::Claude, &raw, 50).unwrap()[0].status,
        ActivityStatus::Cancelled
    );
    assert!(
        !serde_json::to_string(&normalize_hook(Provider::Claude, &raw, 50).unwrap())
            .unwrap()
            .contains("private failure")
    );
}

#[test]
fn parallel_calls_pair_by_explicit_id_without_session_parent_fabrication() {
    let mut store = ActivityStore::new();
    for call in ["call-a", "call-b"] {
        store.ingest(
            normalize_hook(Provider::Claude, &hook("PreToolUse", call), 100).unwrap(),
            0,
        );
    }
    store.ingest(
        normalize_hook(Provider::Claude, &hook("PostToolUse", "call-b"), 200).unwrap(),
        0,
    );
    let snapshot = store.snapshot();
    assert_eq!(snapshot.spans.len(), 2);
    assert_eq!(snapshot.spans[0].status, ActivityStatus::Requested);
    assert_eq!(snapshot.spans[1].status, ActivityStatus::Ok);
    assert!(snapshot
        .spans
        .iter()
        .all(|span| span.parent_id.is_none() && span.trace_id.is_empty()));
    assert_eq!(snapshot.received, 3);
}

#[test]
fn late_start_cannot_create_inverted_time_or_replace_finish() {
    let mut store = ActivityStore::new();
    store.ingest(
        normalize_hook(Provider::Claude, &hook("PostToolUse", "fixture-call"), 100).unwrap(),
        0,
    );
    store.ingest(
        normalize_hook(Provider::Claude, &hook("PreToolUse", "fixture-call"), 200).unwrap(),
        0,
    );
    let result = store.snapshot().spans.remove(0);
    assert_eq!(result.status, ActivityStatus::Ok);
    assert_eq!(result.end_at_ms, Some(100));
    assert_eq!(result.start_at_ms, None);
    assert_eq!(result.duration_ms, None);
}

#[test]
fn late_start_keeps_completed_input_sizes_after_permission_changes() {
    let mut store = ActivityStore::new();
    let mut post = hook("PostToolUse", "fixture-call");
    post["tool_input"] = json!({"command":"approved command"});
    let complete = normalize_hook(Provider::Claude, &post, 100).unwrap();
    let completed_bytes = complete[0].input_bytes;
    store.ingest(complete, 0);
    let mut pre = hook("PreToolUse", "fixture-call");
    pre["tool_input"] = json!({"command":"original"});
    store.ingest(normalize_hook(Provider::Claude, &pre, 200).unwrap(), 0);
    assert_eq!(store.snapshot().spans[0].input_bytes, completed_bytes);
}

#[test]
fn first_explicit_outcome_wins_unknown_can_be_enriched() {
    let mut store = ActivityStore::new();
    let mut error = span("fixture-call");
    error.end_at_ms = Some(200);
    error.status = ActivityStatus::Error;
    store.ingest(vec![error.clone()], 0);
    let mut ok = error.clone();
    ok.status = ActivityStatus::Ok;
    ok.end_at_ms = Some(300);
    store.ingest(vec![ok], 0);
    assert_eq!(store.snapshot().spans[0].status, ActivityStatus::Error);
    assert_eq!(store.snapshot().spans[0].end_at_ms, Some(200));
    store.clear();
    error.status = ActivityStatus::Unknown;
    store.ingest(vec![error.clone()], 0);
    error.status = ActivityStatus::Error;
    store.ingest(vec![error], 0);
    assert_eq!(store.snapshot().spans[0].status, ActivityStatus::Error);
}

#[test]
fn conflicting_updates_are_rejected_without_changing_identity() {
    let original = span("fixture-call");
    for change in 0..8 {
        let mut store = ActivityStore::new();
        store.ingest(vec![original.clone()], 0);
        let mut incoming = original.clone();
        match change {
            0 => incoming.provider = Provider::Codex,
            1 => incoming.source = Source::Otlp,
            2 => incoming.kind = ActivityKind::Model,
            3 => incoming.name = "other_tool".into(),
            4 => incoming.session_id = Some(pseudonymize_id("session", "other").unwrap()),
            5 => {
                incoming.trace_id = pseudonymize_id("trace", "other").unwrap();
                let mut first = original.clone();
                first.trace_id = pseudonymize_id("trace", "first").unwrap();
                store.clear();
                store.ingest(vec![first], 0);
            }
            6 => {
                incoming.pid = Some(2);
                let mut first = original.clone();
                first.pid = Some(1);
                store.clear();
                store.ingest(vec![first], 0);
            }
            _ => {
                incoming.parent_id = Some(pseudonymize_id("parent", "other").unwrap());
                let mut first = original.clone();
                first.parent_id = Some(pseudonymize_id("parent", "first").unwrap());
                store.clear();
                store.ingest(vec![first], 0);
            }
        }
        let before = store.snapshot().spans;
        store.ingest(vec![incoming], 0);
        assert_eq!(store.snapshot().spans, before);
        assert_eq!(store.dropped, 1);
        assert_eq!(store.received, 2);
    }
}

#[test]
fn retention_is_bounded_oldest_first_and_clear_resets_counters() {
    let mut store = ActivityStore::new();
    for index in 0..COLLECTOR_LIMITS.max_spans + 3 {
        store.ingest(vec![span(&format!("call-{index}"))], 0);
    }
    let snapshot = store.snapshot();
    assert_eq!(snapshot.spans.len(), COLLECTOR_LIMITS.max_spans);
    assert_eq!(snapshot.spans[0].id, span("call-3").id);
    assert_eq!(snapshot.received, (COLLECTOR_LIMITS.max_spans + 3) as u64);
    assert_eq!(snapshot.dropped, 3);
    store.clear();
    assert_eq!(store.snapshot().received, 0);
    assert_eq!(store.snapshot().dropped, 0);
    assert!(store.snapshot().spans.is_empty());
}

#[test]
fn subagent_hooks_keep_only_explicit_agent_relationships() {
    let raw = json!({"hook_event_name":"SubagentStart","session_id":"parent-session","agent_id":"child-agent","agent_type":"explore","agent_transcript_path":"/private/chat"});
    let start = normalize_hook(Provider::Codex, &raw, 100)
        .unwrap()
        .remove(0);
    assert_eq!(start.kind, ActivityKind::Agent);
    assert_eq!(start.status, ActivityStatus::Running);
    assert!(start.agent_id.is_some());
    assert!(start.parent_id.is_none());
    assert!(start.parent_agent_id.is_none());
    let mut stop = raw;
    stop["hook_event_name"] = json!("SubagentStop");
    let end = normalize_hook(Provider::Codex, &stop, 200)
        .unwrap()
        .remove(0);
    assert_eq!(start.id, end.id);
    assert_eq!(end.status, ActivityStatus::Unknown);
}

#[test]
fn otlp_parents_join_by_trace_and_span_across_providers() {
    let parent = trace_record("0000000000000001", None, "parent");
    let child = trace_record("0000000000000002", Some("0000000000000001"), "child");
    let parent = normalize::otlp(&traces(vec![parent]), true, 0)
        .unwrap()
        .spans
        .remove(0);
    let mut payload = traces(vec![child]);
    payload["resourceSpans"][0]["resource"]["attributes"][0]["value"]["stringValue"] =
        json!("other-provider");
    let child = normalize::otlp(&payload, true, 0).unwrap().spans.remove(0);
    assert_ne!(parent.provider, child.provider);
    assert_eq!(child.parent_id, Some(parent.id));
    assert_eq!(child.trace_id, parent.trace_id);
    assert_eq!(child.duration_ms, Some(500));
}

#[test]
fn claude_tool_wrapper_and_execution_remain_distinct_with_explicit_status() {
    let parent = trace_record("0000000000000001", None, "claude_code.tool");
    let mut child = trace_record(
        "0000000000000002",
        Some("0000000000000001"),
        "claude_code.tool.execution",
    );
    child["status"] = json!({"code":2,"message":"private error text"});
    let batch = normalize::otlp(&traces(vec![parent, child]), true, 0).unwrap();
    assert_eq!(batch.spans[0].kind, ActivityKind::Tool);
    assert_eq!(batch.spans[0].status, ActivityStatus::Unknown);
    assert_eq!(batch.spans[1].kind, ActivityKind::Step);
    assert_eq!(batch.spans[1].status, ActivityStatus::Error);
    assert_eq!(batch.spans[1].parent_id, Some(batch.spans[0].id.clone()));
    assert!(!serde_json::to_string(&batch.spans)
        .unwrap()
        .contains("private error"));
}

#[test]
fn otlp_requires_valid_identifiers_timestamps_and_typed_values() {
    let base = trace_record("0000000000000001", None, "fixture");
    let mut records = vec![base.clone()];
    for field in ["traceId", "spanId", "startTimeUnixNano", "endTimeUnixNano"] {
        let mut bad = base.clone();
        bad[field] = json!("invalid");
        records.push(bad);
    }
    let mut inverted = base.clone();
    inverted["startTimeUnixNano"] = json!("2000000000");
    records.push(inverted);
    let mut duplicate = base.clone();
    duplicate["attributes"]
        .as_array_mut()
        .unwrap()
        .push(attribute(
            "gen_ai.tool.name",
            json!({"stringValue":"duplicate"}),
        ));
    records.push(duplicate);
    let mut mistyped = base.clone();
    mistyped["attributes"]
        .as_array_mut()
        .unwrap()
        .push(attribute("success", json!({"boolValue":"true"})));
    records.push(mistyped);
    let mut ambiguous = base;
    ambiguous["attributes"]
        .as_array_mut()
        .unwrap()
        .push(attribute(
            "success",
            json!({"boolValue":true,"stringValue":"false"}),
        ));
    records.push(ambiguous);
    let batch = normalize::otlp(&traces(records), true, 0).unwrap();
    assert_eq!(batch.spans.len(), 1);
    assert_eq!(batch.dropped, 8);
}

#[test]
fn otlp_logs_keep_explicit_sizes_and_status_and_ignore_bodies() {
    let mut record = log_record(Some(json!({"stringValue":"false"})));
    record["body"] = json!({"stringValue":"private tool result"});
    let attrs = record["attributes"].as_array_mut().unwrap();
    attrs.push(attribute("duration_ms", json!({"intValue":"250"})));
    attrs.push(attribute(
        "tool_input_size_bytes",
        json!({"intValue":"123"}),
    ));
    attrs.push(attribute("tool_result_size_bytes", json!({"intValue":456})));
    attrs.push(attribute(
        "tool_parameters",
        json!({"stringValue":"private command"}),
    ));
    let result = normalize::otlp(&logs(vec![record]), false, 0)
        .unwrap()
        .spans
        .remove(0);
    assert_eq!(result.status, ActivityStatus::Error);
    assert_eq!(result.input_bytes, Some(123));
    assert_eq!(result.output_bytes, Some(456));
    assert_eq!(result.duration_ms, Some(250));
    assert_eq!(result.start_at_ms, Some(1750));
    assert!(!serde_json::to_string(&result).unwrap().contains("private"));
    assert_eq!(
        normalize::otlp(&logs(vec![log_record(None)]), false, 0)
            .unwrap()
            .spans[0]
            .status,
        ActivityStatus::Unknown
    );
}

#[test]
fn codex_tracing_bridge_decimal_strings_and_conversation_id_are_projected() {
    let mut record = log_record(Some(json!({"stringValue":"true"})));
    record.as_object_mut().unwrap().remove("eventName");
    let attrs = record["attributes"].as_array_mut().unwrap();
    attrs.retain(|entry| entry["key"] != "session_id");
    attrs.push(attribute(
        "event.name",
        json!({"stringValue":"codex.tool_result"}),
    ));
    attrs.push(attribute(
        "conversation.id",
        json!({"stringValue":"real-provider-fixture"}),
    ));
    attrs.push(attribute("duration_ms", json!({"stringValue":"37"})));
    let projected = normalize::otlp(&logs(vec![record.clone()]), false, 0)
        .unwrap()
        .spans
        .remove(0);
    assert_eq!(projected.provider, Provider::Codex);
    assert_eq!(projected.status, ActivityStatus::Ok);
    assert_eq!(projected.duration_ms, Some(37));
    assert_eq!(projected.start_at_ms, Some(1963));
    assert!(projected.session_id.is_some());
    assert!(!serde_json::to_string(&projected)
        .unwrap()
        .contains("real-provider-fixture"));
    record["attributes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["key"] == "duration_ms")
        .unwrap()["value"] = json!({"stringValue":"37 milliseconds"});
    assert_eq!(
        normalize::otlp(&logs(vec![record]), false, 0)
            .unwrap()
            .spans[0]
            .duration_ms,
        None
    );
}

#[test]
fn actual_codex_otel_031_wire_fixture_projects_without_sensitive_fields() {
    // Synthetic emission reproduces public openai/codex tool_result.rs/shared.rs at
    // 8c0f7011674145da7e10627769fb221c74fbf7a0 through its OTel 0.31 tracing bridge.
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/codex_tool_result_otlp.json"
    ))
    .unwrap();
    let batch = normalize::otlp(&fixture, false, 500).unwrap();
    assert_eq!(batch.dropped, 0);
    assert_eq!(batch.spans.len(), 1);
    let span = &batch.spans[0];
    assert_eq!(span.provider, Provider::Codex);
    assert_eq!(span.name, "exec_command");
    assert_eq!(span.status, ActivityStatus::Ok);
    assert_eq!(span.duration_ms, Some(1234));
    assert!(span.session_id.is_some());
    let expected = humantime::parse_rfc3339("2026-10-11T12:00:00.123Z")
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert_eq!(span.end_at_ms, Some(expected));
    assert_eq!(span.start_at_ms, Some(expected - 1234));
    let encoded = serde_json::to_string(span).unwrap();
    for forbidden in [
        "synthetic-private",
        "fixture-machine",
        "019a0000",
        "src/main.rs",
        "fixture-agent",
    ] {
        assert!(!encoded.contains(forbidden));
    }
    let mut observed_only = fixture;
    let record = &mut observed_only["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0];
    record["attributes"]
        .as_array_mut()
        .unwrap()
        .retain(|entry| entry["key"] != "event.timestamp");
    let projected = normalize::otlp(&observed_only, false, 500)
        .unwrap()
        .spans
        .remove(0);
    assert_eq!(projected.end_at_ms, Some(1791708555628));
    assert_eq!(projected.start_at_ms, None);
}

#[test]
fn normalized_route_rejects_payload_fields_invalid_relationships_and_sizes() {
    let base = serde_json::to_value(span("fixture-call")).unwrap();
    let mut records = vec![base.clone()];
    let mut unknown = base.clone();
    unknown["arguments"] = json!("private");
    records.push(unknown);
    let mut raw = base.clone();
    raw["id"] = json!("raw-id");
    records.push(raw);
    let mut parent = base.clone();
    parent["parent_id"] = json!(pseudonymize_id("parent", "fixture").unwrap());
    records.push(parent);
    let mut terminal = base.clone();
    terminal["status"] = json!("ok");
    records.push(terminal);
    let mut handoff = base.clone();
    handoff["kind"] = json!("handoff");
    records.push(handoff);
    let mut duration = base;
    duration["duration_ms"] = json!(u64::MAX);
    records.push(duration);
    let batch = normalize::normalized(&json!(records)).unwrap();
    assert_eq!(batch.spans.len(), 1);
    assert_eq!(batch.dropped, 6);
    assert!(normalize::normalized(&json!(vec![json!({}); 257])).is_err());
}

fn address(credentials: &CollectorCredentials) -> SocketAddr {
    credentials
        .endpoint
        .strip_prefix("http://")
        .unwrap()
        .parse()
        .unwrap()
}

fn request(
    credentials: &CollectorCredentials,
    method: &str,
    path: &str,
    headers: &str,
    body: &str,
) -> String {
    format!("{method} {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}",address(credentials),credentials.token,body.len())
}

fn exchange(credentials: &CollectorCredentials, wire: &str) -> (u16, Value, String) {
    let mut stream =
        TcpStream::connect_timeout(&address(credentials), Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(wire.as_bytes()).unwrap();
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let response = String::from_utf8(response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").expect("HTTP response");
    let code = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, serde_json::from_str(body).unwrap(), response)
}

#[test]
fn real_loopback_client_roundtrip_and_credentials_never_in_projections() {
    let collector = ActivityCollector::new();
    assert!(!collector.status().active);
    let credentials = collector.start().unwrap();
    assert_eq!(credentials.token.len(), 64);
    assert!(address(&credentials).ip().is_loopback());
    assert!(collector.start().is_err());
    let client = client::LocalClient::from_credentials(&credentials).unwrap();
    client.record(&[span("fixture-call")]).unwrap();
    let snapshot = client.snapshot().unwrap();
    assert_eq!(snapshot.received, 1);
    assert_eq!(snapshot.spans.len(), 1);
    for encoded in [
        serde_json::to_string(&collector.status()).unwrap(),
        serde_json::to_string(&snapshot).unwrap(),
        format!("{credentials:?}"),
    ] {
        assert!(!encoded.contains(&credentials.token));
    }
    collector.stop().unwrap();
    assert!(!collector.status().active);
    assert_eq!(
        collector.snapshot().spans[0].status,
        ActivityStatus::Unknown
    );
    assert!(collector.snapshot().spans[0].end_at_ms.is_none());
    assert!(
        TcpStream::connect_timeout(&address(&credentials), Duration::from_millis(100)).is_err()
    );
    collector.clear().unwrap();
    assert_eq!(collector.snapshot().received, 0);
    let next = collector.start().unwrap();
    assert_ne!(credentials.token, next.token);
}

#[test]
fn http_hook_routes_preserve_provider_specific_lifecycle_rules() {
    let collector = ActivityCollector::new();
    let credentials = collector.start().unwrap();
    let start = hook("PreToolUse", "fixture-call").to_string();
    for route in [protocol::Route::CodexHook, protocol::Route::ClaudeHook] {
        let (code, body, _) = exchange(
            &credentials,
            &request(&credentials, "POST", route.path(), "", &start),
        );
        assert_eq!(code, 200);
        assert_eq!(body, json!({"accepted":1,"dropped":0}));
    }
    let failure = hook("PostToolUseFailure", "fixture-call").to_string();
    let (code, body, _) = exchange(
        &credentials,
        &request(
            &credentials,
            "POST",
            protocol::Route::CodexHook.path(),
            "",
            &failure,
        ),
    );
    assert_eq!(code, 400);
    assert_eq!(
        body,
        json!({"code":3,"message":"Invalid activity metadata"})
    );
    assert_eq!(
        exchange(
            &credentials,
            &request(
                &credentials,
                "POST",
                protocol::Route::ClaudeHook.path(),
                "",
                &failure,
            ),
        )
        .0,
        200
    );
    let snapshot = collector.snapshot();
    assert_eq!(snapshot.received, 3);
    assert_eq!(snapshot.spans.len(), 2);
    assert_eq!(snapshot.spans[0].provider, Provider::Codex);
    assert_eq!(snapshot.spans[0].status, ActivityStatus::Requested);
    assert_eq!(snapshot.spans[1].provider, Provider::Claude);
    assert_eq!(snapshot.spans[1].status, ActivityStatus::Error);
    assert!(snapshot
        .spans
        .iter()
        .all(|span| span.source == Source::Hook));
    assert_ne!(snapshot.spans[0].id, snapshot.spans[1].id);
}

#[test]
fn http_rejects_unauthorized_browser_remote_and_ambiguous_requests() {
    let collector = ActivityCollector::new();
    let credentials = collector.start().unwrap();
    let valid = request(&credentials, "POST", "/v1/snapshot", "", "{}");
    let cases = vec![
        (valid.replace(&credentials.token, &"0".repeat(64)), 401),
        (
            valid.replace(&address(&credentials).to_string(), "attacker.invalid"),
            403,
        ),
        (
            request(
                &credentials,
                "POST",
                "/v1/snapshot",
                "Origin: http://localhost\r\n",
                "{}",
            ),
            403,
        ),
        (
            request(
                &credentials,
                "POST",
                "/v1/snapshot",
                "Access-Control-Request-Method: POST\r\n",
                "{}",
            ),
            403,
        ),
        (request(&credentials, "GET", "/v1/snapshot", "", "{}"), 405),
        (
            request(&credentials, "POST", "/v1/snapshot?token=private", "", "{}"),
            404,
        ),
        (
            request(
                &credentials,
                "POST",
                "/v1/snapshot",
                "Content-Encoding: gzip\r\n",
                "{}",
            ),
            415,
        ),
        (
            request(
                &credentials,
                "POST",
                "/v1/snapshot",
                "Transfer-Encoding: chunked\r\n",
                "{}",
            ),
            400,
        ),
        (
            request(
                &credentials,
                "POST",
                "/v1/snapshot",
                "Expect: 100-continue\r\n",
                "{}",
            ),
            400,
        ),
        (
            request(
                &credentials,
                "POST",
                "/v1/snapshot",
                "Content-Length: 2\r\n",
                "{}",
            ),
            400,
        ),
        (valid.replace("application/json", "text/plain"), 415),
        (valid.replace("HTTP/1.1", "HTTP/1.0"), 400),
    ];
    for (wire, expected) in cases {
        let (code, body, response) = exchange(&credentials, &wire);
        assert_eq!(code, expected, "request {wire:?}");
        assert!(!body.to_string().contains(&credentials.token));
        assert!(!response.contains("Access-Control-Allow"));
    }
    assert_eq!(collector.snapshot().received, 0);
}

#[test]
fn http_enforces_header_body_record_and_snapshot_limits() {
    let collector = ActivityCollector::new();
    let credentials = collector.start().unwrap();
    let mut excessive = request(
        &credentials,
        "POST",
        "/v1/activity",
        &format!("X-Fixture: {}\r\n", "x".repeat(9000)),
        "[]",
    );
    excessive.truncate(8192);
    assert_eq!(exchange(&credentials, &excessive).0, 431);
    let mut oversized = request(&credentials, "POST", "/v1/activity", "", "[]");
    oversized = oversized.replace("Content-Length: 2", "Content-Length: 262145");
    assert_eq!(exchange(&credentials, &oversized).0, 413);
    assert_eq!(
        exchange(
            &credentials,
            &request(
                &credentials,
                "POST",
                "/v1/activity",
                "",
                &json!(vec![json!({}); 257]).to_string()
            )
        )
        .0,
        413
    );
    assert_eq!(
        exchange(
            &credentials,
            &request(
                &credentials,
                "POST",
                "/v1/snapshot",
                "",
                "{\"private\":true}"
            )
        )
        .0,
        400
    );
    assert_eq!(
        exchange(
            &credentials,
            &request(&credentials, "POST", "/v1/activity", "", "not-json")
        )
        .0,
        400
    );
    assert_eq!(collector.snapshot().received, 0);
}

#[test]
fn http_partial_success_reports_rejected_records_and_keeps_no_payloads() {
    let collector = ActivityCollector::new();
    let credentials = collector.start().unwrap();
    let payload = traces(vec![
        trace_record("0000000000000001", None, "fixture"),
        json!({"name":"private contents"}),
    ]);
    let (code, body, _) = exchange(
        &credentials,
        &request(&credentials, "POST", "/v1/traces", "", &payload.to_string()),
    );
    assert_eq!(code, 200);
    assert_eq!(body["partialSuccess"]["rejectedSpans"], "1");
    assert_eq!(collector.snapshot().received, 1);
    assert_eq!(collector.snapshot().dropped, 1);
    assert!(!serde_json::to_string(&collector.snapshot())
        .unwrap()
        .contains("private contents"));
    let payload = logs(vec![
        log_record(None),
        json!({"body":{"stringValue":"private chat"}}),
    ]);
    let (_, body, _) = exchange(
        &credentials,
        &request(&credentials, "POST", "/v1/logs", "", &payload.to_string()),
    );
    assert_eq!(body["partialSuccess"]["rejectedLogRecords"], "1");
}

#[test]
fn http_acknowledges_conflicts_as_rejected_but_retention_as_accepted() {
    let collector = ActivityCollector::new();
    let credentials = collector.start().unwrap();
    let original = span("fixture-call");
    let (_, ack, _) = exchange(
        &credentials,
        &request(
            &credentials,
            "POST",
            "/v1/activity",
            "",
            &json!([original.clone()]).to_string(),
        ),
    );
    assert_eq!(ack, json!({"accepted":1,"dropped":0}));
    let mut conflicting = original;
    conflicting.name = "different_tool".into();
    let (_, ack, _) = exchange(
        &credentials,
        &request(
            &credentials,
            "POST",
            "/v1/activity",
            "",
            &json!([conflicting]).to_string(),
        ),
    );
    assert_eq!(ack, json!({"accepted":0,"dropped":1}));
    assert_eq!(collector.snapshot().received, 2);
    assert_eq!(collector.snapshot().dropped, 1);
    collector.clear().unwrap();
    for batch in 0..4 {
        let records: Vec<_> = (0..256)
            .map(|index| span(&format!("retained-{}", batch * 256 + index)))
            .collect();
        let (_, ack, _) = exchange(
            &credentials,
            &request(
                &credentials,
                "POST",
                "/v1/activity",
                "",
                &json!(records).to_string(),
            ),
        );
        assert_eq!(ack, json!({"accepted":256,"dropped":0}));
    }
    assert_eq!(collector.snapshot().spans.len(), 1000);
    assert_eq!(collector.snapshot().dropped, 24);
}

#[test]
fn collector_drop_closes_owned_listener_and_bounded_workers_stop_slow_clients() {
    let credentials;
    {
        let collector = ActivityCollector::new();
        credentials = collector.start().unwrap();
        let mut streams = Vec::new();
        for _ in 0..COLLECTOR_LIMITS.max_workers + COLLECTOR_LIMITS.max_pending_connections + 4 {
            if let Ok(mut stream) =
                TcpStream::connect_timeout(&address(&credentials), Duration::from_millis(100))
            {
                let _ = stream.write_all(b"POST /v1/activity HTTP/1.1\r\n");
                streams.push(stream);
            }
        }
        let started = Instant::now();
        collector.stop().unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(!collector.status().active);
        drop(streams);
    }
    assert!(
        TcpStream::connect_timeout(&address(&credentials), Duration::from_millis(100)).is_err()
    );
    let collector = ActivityCollector::new();
    let next = collector.start().unwrap();
    drop(collector);
    assert!(TcpStream::connect_timeout(&address(&next), Duration::from_millis(100)).is_err());
}

#[test]
fn delayed_partial_requests_work_but_trickle_cannot_extend_absolute_deadline() {
    let collector = ActivityCollector::new();
    let credentials = collector.start().unwrap();
    let mut delayed = TcpStream::connect(address(&credentials)).unwrap();
    delayed
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let wire = request(&credentials, "POST", "/v1/snapshot", "", "{}");
    let split = wire.len() / 2;
    delayed.write_all(&wire.as_bytes()[..split]).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    delayed.write_all(&wire.as_bytes()[split..]).unwrap();
    let mut response = String::new();
    delayed.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    let mut slow = TcpStream::connect(address(&credentials)).unwrap();
    slow.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let wire = request(&credentials, "POST", "/v1/snapshot", "", "{}");
    let prefix = wire.strip_suffix("{}").unwrap();
    let started = Instant::now();
    slow.write_all(prefix.as_bytes()).unwrap();
    std::thread::sleep(Duration::from_millis(600));
    slow.write_all(b"{").unwrap();
    let mut bytes = Vec::new();
    let _ = slow.read_to_end(&mut bytes);
    assert!(started.elapsed() < Duration::from_millis(1600));
    assert!(collector.snapshot().spans.is_empty());
}

#[test]
fn oversized_and_unrecognized_hook_inputs_fail_with_fixed_safe_errors() {
    let mut raw = hook("PreToolUse", "fixture-call");
    raw["tool_input"] = json!("x".repeat(COLLECTOR_LIMITS.max_body_bytes));
    assert_eq!(
        normalize_hook(Provider::Claude, &raw, 0)
            .unwrap_err()
            .to_string(),
        "Activity metadata exceeds collector limits"
    );
    raw = json!({"hook_event_name":"private-event","session_id":"fixture"});
    assert_eq!(
        normalize_hook(Provider::Claude, &raw, 0)
            .unwrap_err()
            .to_string(),
        "Invalid activity metadata"
    );
    assert!(normalize_hook(Provider::Holdmap, &hook("PreToolUse", "fixture"), 0).is_err());
}
