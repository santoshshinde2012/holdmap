//! Optional, payload-free observation of this server's own tool requests.
//!
//! No observer is attached by default. This is not traffic interception, and does not
//! observe calls made to other MCP servers. Byte sizes describe compact JSON messages,
//! not network traffic, prompts or token usage.

use serde::{Deserialize, Serialize};

/// Metadata emitted immediately before one known tool request is dispatched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallStarted {
    /// Generated session-local ordinal, never the client's JSON-RPC request ID.
    pub call_id: u64,
    /// Canonical name from this server's fixed tool catalog.
    pub tool_name: String,
    /// Unix epoch milliseconds when dispatch began.
    pub started_at_ms: u64,
    /// Compact serialized JSON-RPC request size, including its envelope, in bytes.
    /// This excludes whitespace and the stdio newline; arguments are never exposed.
    pub input_bytes: u64,
}

/// Classification of the server's tool response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallOutcome {
    /// A successful tool result.
    Success,
    /// An MCP tool result with `isError: true`.
    ToolError,
    /// A JSON-RPC error, such as unsupported tool-call parameters.
    ProtocolError,
}

/// Metadata emitted after dispatch and compatibility adaptation of one tool response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallCompleted {
    /// The matching generated ordinal from [`ToolCallStarted`].
    pub call_id: u64,
    /// Unix epoch milliseconds when dispatch completed.
    pub finished_at_ms: u64,
    /// Elapsed dispatch time measured by a monotonic clock, in milliseconds.
    pub duration_ms: u64,
    /// Compact serialized JSON-RPC response size, excluding the stdio newline.
    /// In a legacy batch this counts the individual response, not the batch wrapper.
    pub output_bytes: u64,
    /// Successful result, tool error, or protocol error.
    pub outcome: ToolCallOutcome,
    /// JSON-RPC error code; absent for successful and tool-error results.
    pub error_code: Option<i64>,
}

/// A best-effort metadata sink chosen by the caller; the server does not persist events.
///
/// Implementations must handle their own storage failures, return promptly and never write
/// to the protocol output stream. Neither callback receives arguments, results, raw request
/// IDs, client metadata, error text or transport payloads. Unknown tools, invalid envelopes,
/// notifications and non-tool methods are not observed.
pub trait ToolCallObserver {
    /// Receive dispatch metadata; the default implementation discards it.
    fn started(&mut self, _event: &ToolCallStarted) {}
    /// Receive completion metadata; the default implementation discards it.
    fn completed(&mut self, _event: &ToolCallCompleted) {}
}

/// Default observer; no events are captured or persisted.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopToolCallObserver;

impl ToolCallObserver for NoopToolCallObserver {}

pub(super) fn canonical_name(name: &str) -> Option<&'static str> {
    match name {
        "list_ports" => Some("list_ports"),
        "explain_port" => Some("explain_port"),
        "find_free_port" => Some("find_free_port"),
        "wait_for_port" => Some("wait_for_port"),
        "get_topology" => Some("get_topology"),
        "list_agents" => Some("list_agents"),
        "stop_agent_ports" => Some("stop_agent_ports"),
        "plan_cluster_stop" => Some("plan_cluster_stop"),
        "stop_port" => Some("stop_port"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[derive(Default)]
    struct Recorder {
        started: Vec<ToolCallStarted>,
        completed: Vec<ToolCallCompleted>,
        order: Vec<(bool, u64)>,
    }

    impl ToolCallObserver for Recorder {
        fn started(&mut self, event: &ToolCallStarted) {
            self.started.push(event.clone());
            self.order.push((true, event.call_id));
        }
        fn completed(&mut self, event: &ToolCallCompleted) {
            self.completed.push(event.clone());
            self.order.push((false, event.call_id));
        }
    }

    fn observe(messages: &[Value]) -> (Recorder, Vec<Value>, Vec<u8>) {
        let input = messages
            .iter()
            .map(|message| {
                serde_json::to_string_pretty(message)
                    .unwrap()
                    .replace('\n', " ")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut recorder = Recorder::default();
        let mut output = vec![];
        crate::serve_with_observer(input.as_bytes(), &mut output, &mut recorder).unwrap();
        let responses = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        (recorder, responses, output)
    }

    #[test]
    fn observer_names_match_only_the_fixed_catalog() {
        let catalog = crate::catalog::list();
        let names: Vec<_> = catalog
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names.len(), 9);
        for name in names {
            assert_eq!(canonical_name(name), Some(name));
        }
        assert!(canonical_name("list_agents\u{1b}[31m").is_none());
        assert!(canonical_name("private user input").is_none());
    }

    #[test]
    fn observer_keeps_protocol_bytes_and_only_captures_fixed_metadata() {
        let messages = [
            json!({"jsonrpc":"2.0","id":"private-request-id","method":"tools/call","params":{"name":"wait_for_port","arguments":{"port":0,"private-argument":"private-chat"},"_meta":{"private":"private-metadata"}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"find_free_port","task":{"private":"private-task"}}}),
        ];
        let (recorder, responses, output) = observe(&messages);
        let input = messages
            .iter()
            .map(|message| serde_json::to_string(message).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let mut ordinary_output = vec![];
        crate::serve(input.as_bytes(), &mut ordinary_output).unwrap();
        assert_eq!(
            output, ordinary_output,
            "attaching an observer preserves protocol output"
        );
        assert_eq!(
            recorder.order,
            [(true, 1), (false, 1), (true, 2), (false, 2)]
        );
        assert_eq!(recorder.completed[0].outcome, ToolCallOutcome::ToolError);
        assert_eq!(recorder.completed[0].error_code, None);
        assert_eq!(
            recorder.completed[1].outcome,
            ToolCallOutcome::ProtocolError
        );
        assert_eq!(recorder.completed[1].error_code, Some(-32602));
        for (index, message) in messages.iter().enumerate() {
            assert_eq!(
                recorder.started[index].input_bytes,
                serde_json::to_vec(message).unwrap().len() as u64
            );
            assert_eq!(
                recorder.completed[index].output_bytes,
                serde_json::to_vec(&responses[index]).unwrap().len() as u64
            );
            assert!(recorder.started[index].started_at_ms > 0);
            assert!(recorder.completed[index].finished_at_ms > 0);
        }
        let events = format!(
            "{}{}",
            serde_json::to_string(&recorder.started).unwrap(),
            serde_json::to_string(&recorder.completed).unwrap()
        );
        assert!(
            !events.contains("private"),
            "request IDs, arguments, metadata and errors stay out of observer events"
        );
        assert!(events.contains("wait_for_port"));
    }

    #[test]
    fn success_is_measured_and_non_tools_unknowns_and_notifications_are_ignored() {
        let (recorder, responses, _) = observe(&[
            json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"private unknown tool"}}),
            json!({"jsonrpc":"2.0","id":[],"method":"tools/call","params":{"name":"find_free_port"}}),
            json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"find_free_port"}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"find_free_port","arguments":{}}}),
        ]);
        assert_eq!(recorder.started.len(), 1);
        assert_eq!(recorder.started[0].call_id, 1);
        assert_eq!(recorder.started[0].tool_name, "find_free_port");
        assert_eq!(recorder.completed[0].outcome, ToolCallOutcome::Success);
        assert_eq!(recorder.completed[0].error_code, None);
        let response = responses.last().unwrap();
        assert!(
            response["result"]["structuredContent"]["port"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert_eq!(
            recorder.completed[0].output_bytes,
            serde_json::to_vec(response).unwrap().len() as u64
        );
    }

    #[test]
    fn legacy_batch_sizes_match_adapted_responses_and_reinitialize_keeps_unique_ids() {
        let first = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"find_free_port"}});
        let second = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"wait_for_port","arguments":{}}});
        let (recorder, responses, _) = observe(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}),
            json!([first, second]),
            json!({"jsonrpc":"2.0","id":4,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"wait_for_port","arguments":{}}}),
        ]);
        assert_eq!(
            recorder
                .started
                .iter()
                .map(|event| event.call_id)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        let batch = responses[1].as_array().unwrap();
        assert!(batch[0]["result"]["structuredContent"].is_null());
        for (index, response) in batch.iter().enumerate() {
            assert_eq!(
                recorder.completed[index].output_bytes,
                serde_json::to_vec(response).unwrap().len() as u64
            );
        }
        assert_eq!(recorder.completed[0].outcome, ToolCallOutcome::Success);
        assert_eq!(recorder.completed[1].outcome, ToolCallOutcome::ToolError);
    }

    #[test]
    fn oversized_frame_and_batch_never_emit_tool_activity() {
        let mut recorder = Recorder::default();
        let mut input = vec![b' '; crate::MAX_FRAME_BYTES + 1];
        input.push(b'\n');
        let batch = vec![
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"find_free_port"}});
            crate::MAX_BATCH_MESSAGES + 1
        ];
        input.extend(serde_json::to_vec(&batch).unwrap());
        let mut output = vec![];
        crate::serve_with_observer(input.as_slice(), &mut output, &mut recorder).unwrap();
        assert_eq!(std::str::from_utf8(&output).unwrap().lines().count(), 2);
        assert!(recorder.started.is_empty());
        assert!(recorder.completed.is_empty());
    }
}
