//! Owned local integrations exercise the shipped binary; no agent account or payload logs.

use assert_cmd::Command;
use holdmap_activity::client::LocalClient;
use holdmap_activity::{ActivityCollector, ActivityStatus, CollectorCredentials, Source};
use serde_json::{json, Value};

fn command(credentials: Option<&CollectorCredentials>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_holdmap"));
    command
        .env("HOLDMAP_COLOR", "never")
        .env_remove("HOLDMAP_ACTIVITY_ENDPOINT")
        .env_remove("HOLDMAP_ACTIVITY_TOKEN")
        .timeout(std::time::Duration::from_secs(10));
    if let Some(credentials) = credentials {
        command
            .env("HOLDMAP_ACTIVITY_ENDPOINT", &credentials.endpoint)
            .env("HOLDMAP_ACTIVITY_TOKEN", &credentials.token);
    }
    command
}

#[cfg(unix)]
#[test]
fn invalid_unicode_credentials_never_appear_in_errors() {
    use std::os::unix::ffi::OsStringExt;
    let invalid = std::ffi::OsString::from_vec(b"private-credential-fixture\xff".to_vec());
    for field in ["HOLDMAP_ACTIVITY_ENDPOINT", "HOLDMAP_ACTIVITY_TOKEN"] {
        for args in [
            vec!["activity", "snapshot"],
            vec!["activity", "setup", "--provider", "codex"],
        ] {
            let output = command(None)
                .env("HOLDMAP_ACTIVITY_ENDPOINT", "http://127.0.0.1:12345")
                .env("HOLDMAP_ACTIVITY_TOKEN", "a".repeat(64))
                .env(field, &invalid)
                .args(args)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(error.contains(field));
            assert!(!error.contains("private-credential-fixture"));
            assert!(!error.contains("not valid unicode"));
        }
    }
}

fn hook(credentials: &CollectorCredentials, provider: &str, event: Value) {
    let output = command(Some(credentials))
        .args(["activity", "hook", "--provider", provider])
        .write_stdin(serde_json::to_vec(&event).unwrap())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "hooks must never steer the agent");
    assert!(
        output.stderr.is_empty(),
        "valid metadata should reach the recorder"
    );
}

#[test]
fn same_named_parallel_calls_pair_by_id_and_exclude_payloads() {
    let collector = ActivityCollector::new();
    let credentials = collector.start().unwrap();
    let common = json!({
        "session_id":"private-session-fixture", "tool_name":"Bash",
        "cwd":"/private/workspace-fixture", "transcript_path":"/must-not-open/chat-fixture.jsonl",
        "tool_input":{"command":"private-command-fixture", "password":"private-password-fixture"}
    });
    for call in ["private-call-a", "private-call-b"] {
        let mut start = common.clone();
        start["hook_event_name"] = json!("PreToolUse");
        start["tool_use_id"] = json!(call);
        hook(&credentials, "claude", start);
    }
    let mut failed = common.clone();
    failed["hook_event_name"] = json!("PostToolUseFailure");
    failed["tool_use_id"] = json!("private-call-b");
    failed["error"] = json!("private-error-fixture");
    failed["duration_ms"] = json!(42);
    hook(&credentials, "claude", failed);
    let mut completed = common;
    completed["hook_event_name"] = json!("PostToolUse");
    completed["tool_use_id"] = json!("private-call-a");
    completed["tool_response"] = json!({"text":"private-result-fixture"});
    completed["duration_ms"] = json!(84);
    hook(&credentials, "claude", completed);
    let snapshot = LocalClient::from_credentials(&credentials)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(snapshot.spans.len(), 2);
    assert_eq!(snapshot.received, 4);
    assert_eq!(snapshot.dropped, 0);
    assert_eq!(snapshot.spans[0].status, ActivityStatus::Ok);
    assert_eq!(snapshot.spans[1].status, ActivityStatus::Error);
    assert_eq!(snapshot.spans[1].duration_ms, Some(42));
    assert!(snapshot
        .spans
        .iter()
        .all(|span| span.parent_id.is_none() && span.trace_id.is_empty()));
    assert!(snapshot.spans[0].input_bytes.is_some_and(|size| size > 0));
    assert!(snapshot.spans[0].output_bytes.is_some_and(|size| size > 0));
    let serialized = serde_json::to_string(&snapshot).unwrap();
    for secret in [
        "private-session-fixture",
        "private-call-",
        "private-command-fixture",
        "private-password-fixture",
        "private-result-fixture",
        "private-error-fixture",
        "/must-not-open",
        &credentials.token,
    ] {
        assert!(
            !serialized.contains(secret),
            "excluded source content was retained"
        );
    }
    let output = command(Some(&credentials))
        .args(["activity", "snapshot", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let read: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(read["spans"].as_array().unwrap().len(), 2);
    collector.stop().unwrap();
}

#[test]
fn hook_failure_is_advisory_and_never_prints_source_content() {
    let output = command(None)
        .args(["activity", "hook", "--provider", "codex"])
        .write_stdin(json!({"session_id":"private-session", "hook_event_name":"PreToolUse", "tool_use_id":"private-call", "tool_name":"Bash", "tool_input":{"command":"private-command"}}).to_string())
        .output().unwrap();
    assert!(
        output.status.success(),
        "an observer cannot block the agent"
    );
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-"));
}

#[test]
fn codex_nonzero_result_and_missing_finish_remain_truthful() {
    let collector = ActivityCollector::new();
    let credentials = collector.start().unwrap();
    hook(
        &credentials,
        "codex",
        json!({"session_id":"fixture-session", "hook_event_name":"PostToolUse", "tool_use_id":"fixture-failed", "tool_name":"Bash", "tool_response":{"exit_code":1,"output":"private-result"}}),
    );
    hook(
        &credentials,
        "codex",
        json!({"session_id":"fixture-session", "hook_event_name":"PreToolUse", "tool_use_id":"fixture-pending", "tool_name":"Bash", "tool_input":{"command":"private-command"}}),
    );
    let snapshot = collector.snapshot();
    assert_eq!(snapshot.spans[0].status, ActivityStatus::Error);
    assert_eq!(snapshot.spans[1].status, ActivityStatus::Requested);
    collector.stop().unwrap();
    assert_eq!(
        collector.snapshot().spans[1].status,
        ActivityStatus::Unknown
    );
    assert!(collector.snapshot().spans[1].end_at_ms.is_none());
}

#[test]
fn real_mcp_requests_keep_protocol_output_and_record_known_tool_metadata() {
    let collector = ActivityCollector::new();
    let credentials = collector.start().unwrap();
    let request = [
        json!({"jsonrpc":"2.0","id":"private-rpc-success","method":"tools/call","params":{"name":"find_free_port","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":"private-rpc-failure","method":"tools/call","params":{"name":"wait_for_port","arguments":{"port":0,"private":"private-argument-fixture"}}}),
        json!({"jsonrpc":"2.0","id":"private-inspect-id","method":"tools/call","params":{"name":"inspect_port","arguments":["private-argument-fixture"]}}),
        json!({"jsonrpc":"2.0","id":"private-flow-id","method":"tools/call","params":{"name":"get_agent_flow","arguments":"private-argument-fixture"}}),
    ].iter().map(Value::to_string).collect::<Vec<_>>().join("\n") + "\n";
    let output = command(Some(&credentials))
        .args(["mcp", "--record-activity"])
        .write_stdin(request)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 4);
    assert_eq!(responses[0]["id"], "private-rpc-success");
    assert_eq!(responses[1]["id"], "private-rpc-failure");
    assert_eq!(responses[1]["result"]["isError"], true);
    for response in &responses[2..] {
        assert_eq!(response["error"]["code"], -32602);
        assert!(response.get("result").is_none());
    }
    let snapshot = collector.snapshot();
    assert_eq!(snapshot.spans.len(), 4);
    assert_eq!(snapshot.spans[0].name, "find_free_port");
    assert_eq!(snapshot.spans[0].status, ActivityStatus::Ok);
    assert_eq!(snapshot.spans[1].status, ActivityStatus::Error);
    assert_eq!(snapshot.spans[2].name, "inspect_port");
    assert_eq!(snapshot.spans[3].name, "get_agent_flow");
    assert!(snapshot.spans[2..]
        .iter()
        .all(|span| span.status == ActivityStatus::Error));
    assert!(snapshot.spans.iter().all(|span| span.source == Source::Mcp
        && span.mcp_server.is_none()
        && span.duration_ms.is_some()
        && span.output_bytes.is_some_and(|size| size > 0)));
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(!serialized.contains("private-"));
    collector.stop().unwrap();
}

#[test]
fn optional_recording_configuration_cannot_disable_mcp_tools() {
    let output = command(None)
        .args(["mcp", "--record-activity"])
        .write_stdin(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string() + "\n")
        .output()
        .unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["result"]["tools"].as_array().unwrap().len(), 14);
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-"));
}
