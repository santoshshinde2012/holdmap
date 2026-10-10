//! MCP adapter and wire-contract regression tests.

use super::*;
use crate::catalog::list as tools;
use crate::tools::{agents_result, ports_result};
use holdmap_core::agents::AgentsReport;

fn call(msg: Value) -> Value {
    let response = handle(msg.clone()).expect("response");
    if msg["method"] == "tools/call" && response["result"]["isError"] == false {
        let definition = catalog::definition(msg["params"]["name"].as_str().unwrap()).unwrap();
        validation::validate(
            &response["result"]["structuredContent"],
            &definition["outputSchema"],
        )
        .expect("successful result satisfies the published output contract");
        let text = response["result"]["content"][1]["text"].as_str().unwrap();
        let serialized: Value = serde_json::from_str(text)
            .expect("backwards-compatible text block contains valid JSON");
        assert!(serialized.is_object());
        // serde_json without float_roundtrip can parse a CPU f64 one ULP differently.
        // Compare the serialized contract directly without dumping live process data.
        assert_eq!(
            text,
            serde_json::to_string_pretty(&response["result"]["structuredContent"]).unwrap()
        );
    }
    response
}

#[test]
fn initialize_negotiates_version() {
    let r = call(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
    );
    assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(r["result"]["serverInfo"]["name"], "holdmap");
    let r = call(
        json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}),
    );
    assert_eq!(r["result"]["protocolVersion"], SUPPORTED_VERSIONS[0]);
}

#[test]
fn notifications_get_no_response() {
    assert!(handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
}

#[test]
fn lists_tools_with_annotations() {
    let r = call(json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}));
    let tools = r["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 9);
    let stop = tools.iter().find(|t| t["name"] == "stop_port").unwrap();
    assert_eq!(stop["annotations"]["destructiveHint"], true);
    assert_eq!(stop["annotations"]["idempotentHint"], false);
}

#[test]
fn unknown_method_and_tool() {
    assert_eq!(
        call(json!({"jsonrpc":"2.0","id":4,"method":"nope"}))["error"]["code"],
        -32601
    );
    assert_eq!(
        call(json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope"}}))
            ["error"]["code"],
        -32602
    );
}

#[test]
fn find_free_port_and_explain() {
    let r = call(
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"find_free_port","arguments":{}}}),
    );
    let port = r["result"]["structuredContent"]["port"].as_u64().unwrap();
    assert!(port > 0);
    let r = call(
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"explain_port","arguments":{"port":port}}}),
    );
    assert_eq!(r["result"]["structuredContent"]["status"], "free");
    let r = call(
        json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"explain_port","arguments":{"port":0}}}),
    );
    assert_eq!(r["result"]["isError"], true);
}

#[test]
fn refuses_to_stop_own_test_process() {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let r = call(
        json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"stop_port","arguments":{"port":port}}}),
    );
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Refused"));
}

#[test]
fn protected_processes_cannot_be_unlocked_by_an_agent() {
    let stop = tools()
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "stop_port")
        .cloned()
        .unwrap();
    assert!(stop["inputSchema"]["properties"]["allow_protected"].is_null());
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    // Unsupported flags fail validation before inspecting or signalling the owner.
    let r = call(
        json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"stop_port","arguments":{"port":port,"allow_protected":true,"allow_non_dev":true,"force":true}}}),
    );
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Unknown argument `allow_protected`"));
    assert!(l.local_addr().is_ok());
}

#[test]
fn explain_includes_details() {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let _client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let r = call(
        json!({"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"explain_port","arguments":{"port":port}}}),
    );
    let v = &r["result"]["structuredContent"];
    assert_eq!(v["details"][0]["bind_risk"]["level"], "low");
    assert!(v["details"][0]["tree"]["process"]["pid"].as_u64().is_some());
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Bind risk"));
}

#[test]
fn serve_loop_round_trip() {
    let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\nnot json\n";
    let mut out = Vec::new();
    serve(&input[..], &mut out).unwrap();
    let lines: Vec<&str> = std::str::from_utf8(&out).unwrap().lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"result\":{}"));
    assert!(lines[1].contains("-32700"));
}

#[test]
fn topology_and_cluster_plan_are_read_only() {
    let tools = tools();
    for name in ["get_topology", "plan_cluster_stop", "list_agents"] {
        let t = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], true, "{name}");
    }
    let r = call(
        json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"get_topology","arguments":{"all":true,"format":"mermaid"}}}),
    );
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("flowchart LR"));
    assert!(r["result"]["structuredContent"]["nodes"].is_array());
    let r = call(
        json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"plan_cluster_stop","arguments":{"cluster":"nope"}}}),
    );
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("No cluster named"));
}

#[test]
fn list_agents_returns_a_report() {
    let r = call(
        json!({"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"list_agents","arguments":{"agent":"no-such-agent"}}}),
    );
    assert_eq!(r["result"]["isError"], false);
    assert_eq!(r["result"]["structuredContent"]["agents"], json!([]));
    assert!(r["result"]["structuredContent"]["limits"].is_array());
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("No agents or developer tools match \"no-such-agent\"."));
}

fn agents_fixture() -> AgentsReport {
    serde_json::from_value(json!({
            "platform": "linux", "taken_at_ms": 7, "limits": ["Chats are never read."],
            "agents": [{
                "id": "agent:42", "product": "claude-code", "name": "Claude Code", "vendor": "Anthropic",
                "kind": "cli", "pid": 42, "process_name": "claude", "command": "claude", "started_at": 0,
                "parent": null, "memory_bytes": 1048576, "cpu_percent": 25.5,
                "processes": [{
                    "pid": 42, "ppid": 1, "name": "claude", "command": "claude", "role": "agent",
                    "cwd": "/srv/shop-web", "memory_bytes": 1048576, "cpu_percent": 25.5
                }],
                "more_processes": 3, "process_ids": [42, 43, 44, 45],
                "tools": [{
                    "pid": 43, "ppid": 42, "name": "filesystem-mcp", "kind": "mcp_server",
                    "command": "node filesystem-mcp --token=[redacted]", "cwd": "/srv/shop-web",
                    "evidence": "inferred", "ports": [], "memory_bytes": 1024, "cpu_percent": 1.0
                }],
                "more_tools": 2,
                "folders": [{
                    "path": "/srv/shop-web", "label": "shop-web", "project": null, "source": "agent",
                    "evidence": "observed", "pids": [42], "privacy_area": null, "note": null
                }, {
                    "path": "/srv/old-project", "label": "old-project", "project": null, "source": "recent",
                    "evidence": "inferred", "pids": [], "privacy_area": null, "note": null
                }],
                "more_folders": 3, "ports": [], "links": [], "more_links": 4,
                "access": {"user": "dev", "uid": 1000, "root": false, "mine": true, "facts": []}
            }]
        })).expect("agent report fixture")
}

#[test]
fn agent_queries_use_core_ownership_folder_and_tool_matching() {
    for query in ["45", "shop-web", "filesystem-mcp", "anthropic"] {
        let (_, data) = agents_result(agents_fixture(), &json!({"agent": query}));
        assert_eq!(data["agents"][0]["id"], "agent:42", "query {query}");
    }
    let (_, data) = agents_result(agents_fixture(), &json!({"agent": "no-such-tool"}));
    assert_eq!(data["agents"], json!([]));
}

#[test]
fn agent_summary_reports_stdio_tools_resources_and_omissions() {
    let (text, data) = agents_result(agents_fixture(), &json!({}));
    assert!(text.contains("works in shop-web; 3 tools; 0 ports; 4 connection targets; 4 processes; 25.5% CPU; 1.0 MB"), "{text}");
    assert!(!text.contains("works in shop-web, old-project"));
    assert!(text.contains("filesystem-mcp (pid 43): MCP server · inferred · no listening ports"));
    assert!(text.contains("Not listed: 2 tools, 3 folders, 3 processes, 4 connection targets."));
    assert_eq!(
        data["agents"][0]["tools"][0]["command"],
        "node filesystem-mcp --token=[redacted]"
    );
    assert_eq!(data["agents"][0]["process_ids"], json!([42, 43, 44, 45]));
    assert_eq!(data["limits"], json!(["Chats are never read."]));
}

#[test]
fn empty_agent_results_preserve_collection_limits_and_filter_context() {
    let report = AgentsReport {
        limits: vec!["The process table could not be collected.".into()],
        ..AgentsReport::default()
    };
    let (text, data) = agents_result(report.clone(), &json!({}));
    assert!(text.starts_with("No agents or developer tools running."));
    assert!(text.contains("Note: The process table could not be collected."));
    assert_eq!(
        data["limits"],
        json!(["The process table could not be collected."])
    );
    let (filtered, _) = agents_result(report, &json!({"agent": "shop-web"}));
    assert!(filtered.starts_with("No agents or developer tools match \"shop-web\"."));
    assert!(filtered.contains("Note: The process table could not be collected."));
}

#[test]
fn empty_partial_port_results_preserve_warnings_and_scan_metadata() {
    let snapshot = holdmap_core::Snapshot {
        entries: vec![],
        hidden_sockets: 0,
        platform: "fixture-platform".into(),
        taken_at_ms: 1234,
        scan_ms: 2,
        docker_available: false,
        warnings: vec![
            "Process metadata could not be collected.".into(),
            "IPv6 socket table unavailable.".into(),
        ],
    };
    let (text, data) = ports_result(&snapshot, &json!({"protocol": "tcp"}));
    assert!(text.starts_with("0 ports in use."));
    for warning in &snapshot.warnings {
        assert!(text.contains(&format!("Warning: {warning}")));
    }
    assert_eq!(data["ports"], json!([]));
    assert_eq!(data["hidden_sockets"], 0);
    assert_eq!(data["warnings"], json!(snapshot.warnings));
    assert_eq!(data["platform"], "fixture-platform");
    assert_eq!(data["taken_at_ms"], 1234);
    let definition = catalog::definition("list_ports").unwrap();
    validation::validate(&data, &definition["outputSchema"])
        .expect("partial result satisfies output contract");
}

#[test]
fn invalid_tool_arguments_return_actionable_errors() {
    let invalid = [
        ("list_ports", json!({"query": 3000})),
        ("list_ports", json!({"dev_only": "true"})),
        ("list_ports", json!({"protocol": "icmp"})),
        ("list_ports", json!({"typo": true})),
        ("explain_port", json!({})),
        ("explain_port", json!({"port": -1})),
        ("explain_port", json!({"port": 65536})),
        ("explain_port", json!({"port": 3000.5})),
        ("find_free_port", json!({"near": "3000"})),
        ("find_free_port", json!({"near": 0})),
        ("find_free_port", json!({"near": 65536})),
        ("wait_for_port", json!({"port": 3000, "timeout_s": 0.01})),
        ("wait_for_port", json!({"port": 3000, "timeout_s": 600.1})),
        ("wait_for_port", json!({"port": 3000, "timeout_s": "30"})),
        ("wait_for_port", json!({"port": 3000, "until_free": "true"})),
        ("get_topology", json!({"all": "true"})),
        ("get_topology", json!({"format": "html"})),
        ("list_agents", json!({"agent": 42})),
        ("stop_agent_ports", json!({})),
        ("stop_agent_ports", json!({"agent": 42})),
        ("stop_agent_ports", json!({"agent": ""})),
        (
            "stop_agent_ports",
            json!({"agent": "codex", "allow_non_dev": true}),
        ),
        ("plan_cluster_stop", json!({})),
        ("plan_cluster_stop", json!({"cluster": ""})),
        ("plan_cluster_stop", json!({"cluster": null})),
    ];
    for (name, arguments) in invalid {
        let response = call(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
        );
        assert_eq!(response["result"]["isError"], true, "{name}: {arguments}");
        assert!(response["result"]["content"][0]["text"].as_str().is_some());
        assert!(response["result"].get("structuredContent").is_none());
    }
    for arguments in [json!([]), Value::Null, json!(true), json!("{}"), json!(123)] {
        let response = call(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_ports","arguments":arguments}}),
        );
        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["content"][0]["text"],
            "`arguments` must be object"
        );
    }
}

#[test]
fn malformed_stop_flags_never_reach_execution() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    for flag in ["dry_run", "force", "allow_non_dev"] {
        for invalid in [json!("true"), json!(1), Value::Null] {
            let mut arguments = json!({"port": address.port()});
            arguments[flag] = invalid;
            let response = call(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"stop_port","arguments":arguments}}),
            );
            assert_eq!(response["result"]["isError"], true);
            let message = response["result"]["content"][0]["text"].as_str().unwrap();
            assert_eq!(message, format!("`arguments.{flag}` must be boolean"));
            // The validation response, rather than a policy refusal, proves that execution
            // has not progressed to planning; the live listener remains available too.
            assert!(std::net::TcpStream::connect(address).is_ok());
        }
    }
}

#[test]
fn discovery_schemas_are_closed_and_describe_real_result_fields() {
    let definitions = tools();
    for tool in definitions.as_array().unwrap() {
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(tool["outputSchema"]["type"], "object");
        assert!(
            tool["outputSchema"]["properties"].is_object()
                || tool["outputSchema"]["anyOf"].is_array()
        );
        for (key, property) in tool["inputSchema"]["properties"].as_object().unwrap() {
            if let Some(default) = property.get("default") {
                let mut arguments = json!({});
                arguments[key] = default.clone();
                if tool["inputSchema"]["required"]
                    .as_array()
                    .is_some_and(|keys| keys.contains(&json!("port")))
                {
                    arguments["port"] = json!(3000);
                }
                if tool["inputSchema"]["required"]
                    .as_array()
                    .is_some_and(|keys| keys.contains(&json!("agent")))
                {
                    arguments["agent"] = json!("codex");
                }
                validation::validate(&arguments, &tool["inputSchema"])
                    .expect("published defaults are valid");
            }
        }
    }
    let agents = catalog::definition("list_agents").unwrap();
    validation::validate(
        &serde_json::to_value(agents_fixture()).unwrap(),
        &agents["outputSchema"],
    )
    .unwrap();
    assert!(
        agents["outputSchema"]["properties"]["agents"]["items"]["properties"]["process_ids"]
            .is_object()
    );
    assert!(
        catalog::definition("list_ports").unwrap()["outputSchema"]["properties"]["ports"]["items"]
            ["properties"]["protected"]
            .is_object()
    );
}

#[test]
fn stop_schema_accepts_existing_plan_and_execution_reports() {
    let schema = &catalog::definition("stop_port").unwrap()["outputSchema"];
    let plan = holdmap_core::ActionPlan {
        target: ":3000".into(),
        owners: vec![],
        summary: "No owner".into(),
        steps: vec![],
        blocked: None,
        warnings: vec![],
        risk: holdmap_core::Risk::Low,
    };
    validation::validate(&serde_json::to_value(plan).unwrap(), schema).unwrap();
    validation::validate(
        &serde_json::to_value(holdmap_core::StopReport::default()).unwrap(),
        schema,
    )
    .unwrap();
}

#[test]
fn rejects_invalid_rpc_envelopes_without_dispatching() {
    for message in [
        json!([]),
        json!(null),
        json!(42),
        json!({}),
        json!({"id":1,"method":"ping"}),
        json!({"jsonrpc":"1.0","id":1,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":null,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":{},"method":"ping"}),
        json!({"jsonrpc":"2.0","id":1,"method":42}),
        json!({"jsonrpc":"2.0","id":1,"method":"ping","params":[]}),
        json!({"jsonrpc":"2.0","id":1,"method":"ping","params":null}),
    ] {
        assert_eq!(
            handle(message.clone()).unwrap()["error"]["code"],
            -32600,
            "{message}"
        );
    }
    assert!(handle(json!({"jsonrpc":"2.0","method":"unknown_notification"})).is_none());
    assert_eq!(
        call(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{}}))["error"]["code"],
        -32602
    );
}

fn stdio(messages: &[Value]) -> Vec<Value> {
    let input = messages
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = Vec::new();
    serve(input.as_bytes(), &mut output).unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn stdio_negotiation_preserves_legacy_wire_contracts() {
    for version in SUPPORTED_VERSIONS {
        let responses = stdio(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"find_free_port"}}),
        ]);
        let modern = *version >= "2025-06-18";
        assert_eq!(responses[0]["result"]["protocolVersion"], *version);
        let tool = &responses[1]["result"]["tools"][0];
        assert_eq!(tool.get("title").is_some(), modern);
        assert_eq!(tool.get("outputSchema").is_some(), modern);
        assert_eq!(tool.get("annotations").is_some(), *version >= "2025-03-26");
        assert_eq!(
            responses[2]["result"].get("structuredContent").is_some(),
            modern
        );
        assert!(serde_json::from_str::<Value>(
            responses[2]["result"]["content"][1]["text"]
                .as_str()
                .unwrap()
        )
        .unwrap()["port"]
            .as_u64()
            .is_some());
    }
}

#[test]
fn modern_sessions_reject_batches_legacy_sessions_support_them() {
    for version in SUPPORTED_VERSIONS {
        let responses = stdio(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version}}),
            json!([{"jsonrpc":"2.0","id":2,"method":"ping"}]),
        ]);
        if *version >= "2025-06-18" {
            assert_eq!(responses[1]["error"]["code"], -32600);
        } else {
            assert_eq!(responses[1][0]["id"], 2);
            assert_eq!(responses[1][0]["result"], json!({}));
        }
    }
}

#[test]
fn discovery_resources_and_prompts_are_available_through_rpc() {
    let initialize = call(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":SUPPORTED_VERSIONS[0]}}),
    );
    assert_eq!(
        initialize["result"]["capabilities"]["resources"],
        json!({"subscribe": false, "listChanged": false})
    );
    assert_eq!(
        initialize["result"]["capabilities"]["prompts"],
        json!({"listChanged": false})
    );
    assert!(initialize["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("list_agents"));
    let resources = call(json!({"jsonrpc":"2.0","id":2,"method":"resources/list"}));
    assert_eq!(
        resources["result"]["resources"][0]["uri"],
        "holdmap://guide"
    );
    assert_eq!(
        call(json!({"jsonrpc":"2.0","id":3,"method":"resources/templates/list"}))["result"]
            ["resourceTemplates"],
        json!([])
    );
    let guide = call(
        json!({"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"holdmap://guide"}}),
    );
    assert_eq!(guide["result"]["contents"][0]["uri"], "holdmap://guide");
    for params in [json!({}), json!({"uri": 1}), json!({"uri": ""})] {
        assert_eq!(
            call(json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":params}))
                ["error"]["code"],
            -32602
        );
    }
    assert_eq!(
        call(
            json!({"jsonrpc":"2.0","id":6,"method":"resources/read","params":{"uri":"holdmap://unknown"}})
        )["error"]["code"],
        -32002
    );
    let prompts = call(json!({"jsonrpc":"2.0","id":7,"method":"prompts/list"}));
    assert_eq!(prompts["result"]["prompts"].as_array().unwrap().len(), 3);
    let prompt = call(
        json!({"jsonrpc":"2.0","id":8,"method":"prompts/get","params":{"name":"inspect_agents","arguments":{"agent":"codex"}}}),
    );
    assert_eq!(prompt["result"]["messages"][0]["role"], "user");
    assert_eq!(
        call(json!({"jsonrpc":"2.0","id":9,"method":"prompts/get","params":{"name":"unknown"}}))
            ["error"]["code"],
        -32602
    );
}

#[test]
fn misplaced_stop_flags_and_malformed_metadata_are_rejected() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let response = call(
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"stop_port","arguments":{"port":address.port()},"dry_run":true}}),
    );
    assert_eq!(response["error"]["code"], -32602);
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("inside arguments"));
    assert!(std::net::TcpStream::connect(address).is_ok());
    let response = call(
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"find_free_port","_meta":[]}}),
    );
    assert_eq!(response["error"]["code"], -32602);
    let response = call(
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"find_free_port","_meta":{"progressToken":"check"}}}),
    );
    assert_eq!(response["result"]["isError"], false);
}

#[test]
fn integer_ports_follow_json_schema_without_rounding() {
    let definition = catalog::definition("find_free_port").unwrap();
    validation::validate(&json!({"near": 3000.0}), &definition["inputSchema"]).unwrap();
    assert!(validation::validate(&json!({"near": 3000.1}), &definition["inputSchema"]).is_err());
}

#[test]
fn empty_batches_are_invalid_in_every_supported_session() {
    for version in SUPPORTED_VERSIONS {
        let responses = stdio(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version}}),
            json!([]),
        ]);
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[1]["error"]["code"], -32600);
        if *version >= "2025-11-25" {
            assert!(responses[1].get("id").is_none());
        } else {
            assert_eq!(responses[1]["id"], Value::Null);
            assert!(responses[1].get("id").is_some());
        }
    }
}

#[test]
fn request_envelope_controls_metadata_tokens_and_tasks_are_validated() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let response = call(
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"stop_port","arguments":{"port":address.port()}},"dry_run":true}),
    );
    assert_eq!(response["error"]["code"], -32600);
    assert_eq!(response["id"], 1);
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("params.arguments"));
    assert!(std::net::TcpStream::connect(address).is_ok());
    for token in [json!(true), Value::Null, json!([]), json!({})] {
        let response = call(
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"stop_port","arguments":{"port":address.port()},"_meta":{"progressToken":token}}}),
        );
        assert_eq!(response["error"]["code"], -32602);
        assert!(std::net::TcpStream::connect(address).is_ok());
    }
    let response = call(
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"stop_port","arguments":{"port":address.port()},"task":{"ttl":60000}}}),
    );
    assert_eq!(response["error"]["code"], -32602);
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not supported"));
    assert!(std::net::TcpStream::connect(address).is_ok());
}

#[test]
fn uncorrelated_error_ids_follow_the_negotiated_version() {
    for version in SUPPORTED_VERSIONS {
        let input = format!(
            "{}\nnot json\n{}\n",
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version}}),
            json!({"jsonrpc":"2.0","id":null,"method":"ping"})
        );
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output).unwrap();
        let responses: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(responses[1]["error"]["code"], -32700);
        assert_eq!(responses[2]["error"]["code"], -32600);
        for response in &responses[1..] {
            assert_eq!(response.get("id").is_some(), *version < "2025-11-25");
            if *version < "2025-11-25" {
                assert_eq!(response["id"], Value::Null);
            }
        }
    }
}

#[test]
fn legacy_errors_never_gain_a_result_field() {
    for version in ["2024-11-05", "2025-03-26"] {
        let responses = stdio(&[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version}}),
            json!({"jsonrpc":"2.0","id":2,"method":"unknown"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"unknown"}}),
        ]);
        for response in &responses[1..] {
            assert!(response.get("error").is_some());
            assert!(response.get("result").is_none());
        }
    }
}

#[test]
fn agent_stops_validate_controls_before_inspecting_or_signalling() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    for flag in ["dry_run", "force"] {
        for invalid in [json!("false"), json!(1), Value::Null] {
            let mut arguments = json!({"agent": std::process::id().to_string()});
            arguments[flag] = invalid;
            let response = call(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"stop_agent_ports","arguments":arguments}}),
            );
            assert_eq!(response["result"]["isError"], true);
            assert_eq!(
                response["result"]["content"][0]["text"],
                format!("`arguments.{flag}` must be boolean")
            );
            assert!(std::net::TcpStream::connect(address).is_ok());
        }
    }
    let definition = catalog::definition("stop_agent_ports").unwrap();
    assert_eq!(
        definition["inputSchema"]["properties"]["dry_run"]["default"],
        true
    );
    assert_eq!(definition["annotations"]["destructiveHint"], true);
    assert_eq!(definition["annotations"]["idempotentHint"], false);
}

#[test]
fn agent_stop_schema_accepts_plans_reports_and_empty_results() {
    let schema = &catalog::definition("stop_agent_ports").unwrap()["outputSchema"];
    let plan = holdmap_core::ActionPlan {
        target: ":3000".into(),
        owners: vec![],
        summary: "No owner".into(),
        steps: vec![],
        blocked: None,
        warnings: vec![],
        risk: holdmap_core::Risk::Low,
    };
    validation::validate(
        &json!({"plans": [plan], "reports": [holdmap_core::StopReport::default()]}),
        schema,
    )
    .unwrap();
    validation::validate(
        &json!({"agents": ["agent:42"], "plans": [], "reports": []}),
        schema,
    )
    .unwrap();
}

#[test]
fn every_agent_kind_is_in_the_output_contract_and_summary() {
    use holdmap_core::agents::AgentKind;
    let schema = &catalog::definition("list_agents").unwrap()["outputSchema"];
    for kind in [
        AgentKind::Cli,
        AgentKind::Ide,
        AgentKind::Desktop,
        AgentKind::Extension,
        AgentKind::Host,
        AgentKind::Tool,
    ] {
        let mut fixture = agents_fixture();
        fixture.agents[0].kind = kind;
        let (text, data) = agents_result(fixture, &json!({}));
        assert!(text.contains(kind.label()));
        validation::validate(&data, schema).unwrap();
    }
}
