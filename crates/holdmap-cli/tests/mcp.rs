//! Exercise the shipped CLI's MCP stdio interface as a real client, without stopping owners.

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
const SESSION_TIMEOUT: Duration = Duration::from_secs(30);
const STDERR_LIMIT: usize = 8192;

struct FixtureServer {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<usize>>,
}

impl FixtureServer {
    fn finish(mut self) -> usize {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("owned HTTP fixture finishes")
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Drain both pipes while requests run; closing stdin finishes the server gracefully.
/// The cleanup guard reaps only the test's own child, including on assertion failures.
struct McpClient {
    child: Child,
    input: Option<ChildStdin>,
    replies: mpsc::Receiver<Result<Value, String>>,
    readers: Vec<JoinHandle<()>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    deadline: Instant,
    next_id: u64,
    _home: tempfile::TempDir,
}

impl McpClient {
    fn start(version: &str) -> Self {
        let home = tempfile::tempdir().expect("temporary Holdmap state directory");
        let mut child = Command::new(env!("CARGO_BIN_EXE_holdmap"))
            .arg("mcp")
            .env("HOLDMAP_HOME", home.path())
            .env("HOLDMAP_COLOR", "never")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the built Holdmap CLI");
        let input = child.stdin.take().expect("MCP stdin pipe");
        let stdout = child.stdout.take().expect("MCP stdout pipe");
        let mut stderr_pipe = child.stderr.take().expect("MCP stderr pipe");
        let (sender, replies) = mpsc::channel();
        let output_reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let reply = line
                    .map_err(|error| format!("read MCP stdout: {error}"))
                    .and_then(|line| {
                        serde_json::from_str(&line)
                            .map_err(|error| format!("MCP stdout is not JSON-RPC: {error}"))
                    });
                if sender.send(reply).is_err() {
                    break;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&stderr);
        let error_reader = thread::spawn(move || {
            let mut buffer = [0_u8; 1024];
            while let Ok(size) = stderr_pipe.read(&mut buffer) {
                if size == 0 {
                    break;
                }
                let mut output = captured.lock().expect("stderr capture lock");
                let keep = size.min(STDERR_LIMIT.saturating_sub(output.len()));
                output.extend_from_slice(&buffer[..keep]);
            }
        });
        let mut client = Self {
            child,
            input: Some(input),
            replies,
            readers: vec![output_reader, error_reader],
            stderr,
            deadline: Instant::now() + SESSION_TIMEOUT,
            next_id: 1,
            _home: home,
        };
        let initialize = client.request(
            "initialize",
            json!({
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": {"name": "holdmap-integration-test", "version": "1"}
            }),
        );
        assert_eq!(initialize["result"]["protocolVersion"], version);
        assert_eq!(initialize["result"]["serverInfo"]["name"], "holdmap");
        let capabilities = &initialize["result"]["capabilities"];
        assert_eq!(capabilities["tools"]["listChanged"], false);
        assert_eq!(capabilities["resources"]["subscribe"], false);
        assert_eq!(capabilities["prompts"]["listChanged"], false);
        assert!(initialize["result"]["instructions"]
            .as_str()
            .expect("client instructions")
            .contains("list_agents"));
        client.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        client
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.exchange(json!({"method": method, "params": params}))
    }

    fn exchange(&mut self, mut message: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        message["jsonrpc"] = json!("2.0");
        message["id"] = json!(id);
        self.send(&message);
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        let response = self
            .replies
            .recv_timeout(remaining.min(REPLY_TIMEOUT))
            .unwrap_or_else(|error| panic!("MCP response {id}: {error}; stderr: {}", self.stderr()))
            .unwrap_or_else(|error| panic!("{error}; stderr: {}", self.stderr()));
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(
            response["id"], id,
            "MCP replies correlate to their requests"
        );
        assert_ne!(
            response.get("result").is_some(),
            response.get("error").is_some(),
            "MCP reply contains exactly one of result and error"
        );
        response
    }

    fn send(&mut self, message: &Value) {
        // Every fixture request is smaller than 1 KiB; the concurrent stdout reader also
        // prevents response backpressure while this small write completes.
        let input = self.input.as_mut().expect("open MCP stdin");
        serde_json::to_writer(&mut *input, message).expect("write MCP request");
        input.write_all(b"\n").expect("terminate MCP request");
        input.flush().expect("flush MCP request");
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.stderr.lock().expect("stderr capture lock")).into_owned()
    }

    fn finish(mut self) {
        drop(self.input.take());
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("check MCP child exit") {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "MCP did not exit after stdin closed"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.success(),
            "MCP exit {status}; stderr: {}",
            self.stderr()
        );
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        drop(self.input.take());
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

fn names(items: &Value) -> BTreeSet<&str> {
    items
        .as_array()
        .expect("discovered items array")
        .iter()
        .map(|item| item["name"].as_str().expect("discovered item name"))
        .collect()
}

fn tool_text(response: &Value) -> &str {
    response["result"]["content"][0]["text"]
        .as_str()
        .expect("human-readable tool result")
}

fn assert_tool_json_parity(response: &Value) {
    let text = response["result"]["content"][1]["text"]
        .as_str()
        .expect("backwards-compatible JSON text");
    let decoded: Value = serde_json::from_str(text).expect("tool text contains valid JSON");
    // Both representations cross the same JSON decoder. Reserializing its f64 values
    // can change CPU metrics by one ULP without serde_json's float_roundtrip feature.
    assert!(
        decoded == response["result"]["structuredContent"],
        "JSON text and structured content differ"
    );
}

#[test]
fn cli_mcp_json_parity_accepts_equivalent_cpu_metrics() {
    let response: Value = serde_json::from_str(
        r#"{"result":{"content":[{"type":"text","text":"Fixture CPU metric"},{"type":"text","text":"{\n  \"cpu_percent\": 0.18452200293540955\n}"}],"structuredContent":{"cpu_percent":0.18452200293540955},"isError":false}}"#,
    ).unwrap();
    assert_tool_json_parity(&response);
}

fn free_port_content(response: &Value) -> Value {
    assert_eq!(response["result"]["isError"], false);
    let content: Value = serde_json::from_str(
        response["result"]["content"][1]["text"]
            .as_str()
            .expect("JSON-only backwards-compatible text content"),
    )
    .expect("free-port result is valid JSON");
    assert!(content["port"]
        .as_u64()
        .is_some_and(|port| (1..=65535).contains(&port)));
    content
}

fn assert_listener_alive(listener: &TcpListener) {
    let address = listener.local_addr().expect("fixture listener address");
    let _client = TcpStream::connect_timeout(&address, Duration::from_secs(1))
        .expect("fixture listener still accepts TCP connections");
    listener
        .set_nonblocking(true)
        .expect("nonblocking fixture accept");
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match listener.accept() {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "fixture connection was not accepted"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("fixture listener accept: {error}"),
        }
    }
}

#[test]
fn cli_mcp_discovers_tools_context_and_workflows_and_calls_typed_read_tools() {
    let mut client = McpClient::start(VERSIONS[0]);
    let discovery = client.request("tools/list", json!({}));
    let tools = &discovery["result"]["tools"];
    assert_eq!(
        names(tools),
        BTreeSet::from([
            "list_ports",
            "explain_port",
            "inspect_port",
            "find_free_port",
            "wait_for_port",
            "get_topology",
            "list_agents",
            "list_analyzers",
            "list_insights",
            "get_agent_flow",
            "stop_agent_ports",
            "plan_cluster_stop",
            "stop_port"
        ])
    );
    for tool in tools.as_array().unwrap() {
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(tool["outputSchema"]["type"], "object");
        assert!(
            tool["outputSchema"]["properties"].is_object()
                || tool["outputSchema"]["anyOf"].is_array()
        );
        assert_eq!(
            tool["annotations"]["readOnlyHint"],
            !matches!(
                tool["name"].as_str(),
                Some("stop_port" | "stop_agent_ports")
            )
        );
    }
    let stop = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "stop_port")
        .unwrap();
    assert_eq!(stop["annotations"]["destructiveHint"], true);
    assert_eq!(stop["annotations"]["idempotentHint"], false);

    let agent_stop = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "stop_agent_ports")
        .unwrap();
    assert_eq!(agent_stop["annotations"]["destructiveHint"], true);
    assert_eq!(agent_stop["annotations"]["idempotentHint"], false);
    assert_eq!(
        agent_stop["inputSchema"]["properties"]["dry_run"]["default"],
        true
    );

    let resource = client.request("resources/list", json!({}));
    assert_eq!(resource["result"]["resources"].as_array().unwrap().len(), 1);
    assert_eq!(resource["result"]["resources"][0]["uri"], "holdmap://guide");
    let guide = client.request("resources/read", json!({"uri": "holdmap://guide"}));
    assert_eq!(guide["result"]["contents"][0]["uri"], "holdmap://guide");
    assert!(guide["result"]["contents"][0]["text"]
        .as_str()
        .unwrap()
        .contains("not a reservation"));
    assert_eq!(
        client.request("resources/templates/list", json!({}))["result"]["resourceTemplates"],
        json!([])
    );

    let prompts = client.request("prompts/list", json!({}));
    assert_eq!(
        names(&prompts["result"]["prompts"]),
        BTreeSet::from(["diagnose_port", "prepare_dev_server", "inspect_agents"])
    );
    for (params, expected_tool) in [
        (
            json!({"name": "diagnose_port", "arguments": {"port": "03000"}}),
            "explain_port",
        ),
        (json!({"name": "prepare_dev_server"}), "find_free_port"),
        (
            json!({"name": "inspect_agents", "arguments": {"agent": "pid:1234"}}),
            "list_agents",
        ),
    ] {
        let prompt = client.request("prompts/get", params.clone());
        let message = &prompt["result"]["messages"][0];
        assert_eq!(message["role"], "user");
        assert_eq!(message["content"]["type"], "text");
        assert!(message["content"]["text"]
            .as_str()
            .unwrap()
            .contains(expected_tool));
        if params["name"] == "diagnose_port" {
            assert!(message["content"]["text"]
                .as_str()
                .unwrap()
                .contains("{\"port\":3000}"));
        }
    }

    let free = client.request(
        "tools/call",
        json!({"name": "find_free_port", "arguments": {}}),
    );
    assert_eq!(
        free_port_content(&free),
        free["result"]["structuredContent"]
    );
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("local readiness fixture");
    let port = listener.local_addr().unwrap().port();
    let ready = client.request("tools/call", json!({"name": "wait_for_port", "arguments": {"port": port, "timeout_s": 0.5, "until_free": false}}));
    assert_eq!(ready["result"]["isError"], false);
    assert_eq!(ready["result"]["structuredContent"]["port"], port);
    assert_eq!(ready["result"]["structuredContent"]["ready"], true);
    assert_listener_alive(&listener);
    assert_eq!(
        client.request("resources/read", json!({"uri": "holdmap://unknown"}))["error"]["code"],
        -32002
    );
    assert_eq!(
        client.request("resources/read", json!({"uri": "file:///etc/passwd"}))["error"]["code"],
        -32002
    );
    assert_eq!(
        client.request("resources/read", json!({"uri": 42}))["error"]["code"],
        -32602
    );
    client.finish();
}

#[test]
fn cli_mcp_negotiates_every_supported_version_without_mixed_error_results() {
    for version in VERSIONS {
        let mut client = McpClient::start(version);
        let discovery = client.request("tools/list", json!({}));
        let tools = discovery["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 13);
        for tool in tools {
            assert_eq!(tool.get("outputSchema").is_some(), version >= "2025-06-18");
            assert_eq!(tool.get("title").is_some(), version >= "2025-06-18");
            assert_eq!(tool.get("annotations").is_some(), version >= "2025-03-26");
            if matches!(
                tool["name"].as_str(),
                Some("stop_port" | "stop_agent_ports")
            ) {
                assert_eq!(
                    tool["inputSchema"]["properties"]["dry_run"]["default"],
                    true
                );
                assert_eq!(
                    tool["inputSchema"]["then"]["required"],
                    json!(["confirmation_id"])
                );
            }
        }
        let free = client.request(
            "tools/call",
            json!({"name": "find_free_port", "arguments": {}}),
        );
        let content = free_port_content(&free);
        if version >= "2025-06-18" {
            assert_eq!(free["result"]["structuredContent"], content);
        } else {
            assert!(free["result"].get("structuredContent").is_none());
        }
        assert_eq!(
            client.request("resources/list", json!({}))["result"]["resources"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            client.request("prompts/list", json!({}))["result"]["prompts"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            client.request("unknown_method", json!({}))["error"]["code"],
            -32601
        );
        assert_eq!(
            client.request("tools/call", json!({"name": "unknown_tool"}))["error"]["code"],
            -32602
        );
        assert_eq!(
            client.request("resources/read", json!({"uri": "holdmap://unknown"}))["error"]["code"],
            -32002
        );
        for arguments in [Value::Null, json!([]), json!(true), json!(42), json!("{}")] {
            let malformed = client.request(
                "tools/call",
                json!({"name":"inspect_port", "arguments":arguments}),
            );
            assert_eq!(malformed["error"]["code"], -32602);
            assert!(malformed.get("result").is_none());
        }
        assert_eq!(client.request("ping", json!({}))["result"], json!({}));
        client.finish();
    }
}

#[test]
fn cli_mcp_port_inspection_alias_probes_only_the_owned_listener_and_preserves_it() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("local HTTP fixture");
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + SESSION_TIMEOUT;
        let mut requests = 0;
        while !server_stop.load(Ordering::Acquire) && Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_millis(500)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_millis(500)))
                        .unwrap();
                    let mut request = [0_u8; 1024];
                    let mut size = 0;
                    while size < request.len()
                        && Instant::now() < deadline
                        && !request[..size].windows(4).any(|part| part == b"\r\n\r\n")
                    {
                        match stream.read(&mut request[size..]) {
                            Ok(0) | Err(_) => break,
                            Ok(read) => size += read,
                        }
                    }
                    if size > 0 {
                        assert!(request[..size].starts_with(b"GET / HTTP/1.1\r\n"));
                        let body = "<title>Owned MCP fixture</title>";
                        let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nServer: holdmap-test\r\nConnection: close\r\n\r\n{body}", body.len());
                        stream.write_all(response.as_bytes()).unwrap();
                        requests += 1;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("owned HTTP fixture accept: {error}"),
            }
        }
        requests
    });
    let fixture = FixtureServer {
        stop,
        worker: Some(server),
    };
    let mut client = McpClient::start(VERSIONS[0]);
    let mut headline = None;
    for name in ["explain_port", "inspect_port"] {
        let response = client.request(
            "tools/call",
            json!({"name":name,"arguments":{"port":address.port()}}),
        );
        assert_eq!(response["result"]["isError"], false);
        let data = &response["result"]["structuredContent"];
        assert_eq!(data["port"], address.port());
        assert_eq!(data["status"], "busy");
        assert!(data["details"].is_array());
        assert_eq!(data["http"]["status"], 200);
        assert_eq!(data["http"]["title"], "Owned MCP fixture");
        assert!(tool_text(&response).contains("Owned MCP fixture"));
        assert_tool_json_parity(&response);
        if let Some(expected) = &headline {
            assert_eq!(&data["headline"], expected);
        } else {
            headline = Some(data["headline"].clone());
        }
        TcpStream::connect_timeout(&address, Duration::from_secs(1))
            .expect("inspection leaves its owned fixture alive");
    }
    client.finish();
    assert_eq!(fixture.finish(), 2);
}

#[test]
fn cli_mcp_agent_flow_is_separate_from_opt_in_inventory_and_has_text_parity() {
    for version in [VERSIONS[0], VERSIONS[3]] {
        let mut client = McpClient::start(version);
        let flow = client.request(
            "tools/call",
            json!({
                "name":"get_agent_flow", "arguments":{"agent":"holdmap-no-such-agent-fixture"}
            }),
        );
        assert_eq!(flow["result"]["isError"], false);
        let text = flow["result"]["content"][1]["text"].as_str().unwrap();
        let data: Value = serde_json::from_str(text).unwrap();
        assert_eq!(data["nodes"], json!([]));
        assert_eq!(data["edges"], json!([]));
        assert!(data["limits"].is_array());
        assert!(tool_text(&flow).contains("tool calls and payloads are not included"));
        if version >= "2025-06-18" {
            assert_eq!(flow["result"]["structuredContent"], data);
        } else {
            assert!(flow["result"].get("structuredContent").is_none());
        }
        let rejected = client.request(
            "tools/call",
            json!({
                "name":"get_agent_flow", "arguments":{"include_capabilities":true}
            }),
        );
        assert_eq!(rejected["result"]["isError"], true);
        assert!(tool_text(&rejected).contains("Unknown argument `include_capabilities`"));
        client.finish();
    }
}

#[test]
fn cli_mcp_rejects_malformed_and_misplaced_stop_controls_before_acting() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("local stop-validation fixture");
    let port = listener.local_addr().unwrap().port();
    let mut client = McpClient::start(VERSIONS[0]);
    for flag in ["dry_run", "force", "allow_non_dev"] {
        let mut arguments = json!({"port": port});
        arguments[flag] = json!("true");
        let response = client.request(
            "tools/call",
            json!({"name": "stop_port", "arguments": arguments}),
        );
        assert_eq!(response["result"]["isError"], true);
        assert!(tool_text(&response).contains(flag));
        assert!(tool_text(&response).contains("must be boolean"));
        assert_listener_alive(&listener);
    }
    // Validly typed execution requests still need a matching preview from this session.
    // Neither missing nor invented handles can reach collection/execution or stop the fixture.
    for arguments in [
        json!({"port":port,"dry_run":false}),
        json!({"port":port,"dry_run":false,"confirmation_id":"unknown-session-1"}),
        json!({"agent":"fixture-agent","dry_run":false}),
        json!({"agent":"fixture-agent","dry_run":false,"confirmation_id":"unknown-session-1"}),
    ] {
        let name = if arguments.get("port").is_some() {
            "stop_port"
        } else {
            "stop_agent_ports"
        };
        let response = client.request("tools/call", json!({"name":name,"arguments":arguments}));
        assert_eq!(response["result"]["isError"], true);
        let text = tool_text(&response);
        assert!(text.contains("confirmation_id") || text.contains("confirmation"));
        assert_listener_alive(&listener);
    }
    // Agent-scoped stops share strict pre-execution validation and default to a preview.
    for flag in ["dry_run", "force"] {
        let mut arguments = json!({"agent": std::process::id().to_string()});
        arguments[flag] = json!("true");
        let response = client.request(
            "tools/call",
            json!({"name": "stop_agent_ports", "arguments": arguments}),
        );
        assert_eq!(response["result"]["isError"], true);
        assert!(tool_text(&response).contains(&format!("arguments.{flag}` must be boolean")));
        assert_listener_alive(&listener);
    }
    let agent_stop = client.request("tools/call", json!({"name": "stop_agent_ports", "arguments": {"agent": std::process::id().to_string()}, "dry_run": true}));
    assert_eq!(agent_stop["error"]["code"], -32602);
    assert_listener_alive(&listener);
    let misplaced = client.request(
        "tools/call",
        json!({"name": "stop_port", "arguments": {"port": port}, "dry_run": true}),
    );
    assert_eq!(misplaced["error"]["code"], -32602);
    assert!(misplaced["error"]["message"]
        .as_str()
        .unwrap()
        .contains("inside arguments"));
    assert_listener_alive(&listener);
    let envelope = client.exchange(json!({"method": "tools/call", "params": {"name": "stop_port", "arguments": {"port": port}}, "dry_run": true}));
    assert_eq!(envelope["error"]["code"], -32600);
    assert!(envelope["error"]["message"]
        .as_str()
        .unwrap()
        .contains("params.arguments"));
    assert_listener_alive(&listener);
    let unknown = client.request(
        "tools/call",
        json!({"name": "stop_port", "arguments": {"port": port, "allow_protected": true}}),
    );
    assert_eq!(unknown["result"]["isError"], true);
    assert!(tool_text(&unknown).contains("Unknown argument `allow_protected`"));
    assert_listener_alive(&listener);
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));
    client.finish();
}
