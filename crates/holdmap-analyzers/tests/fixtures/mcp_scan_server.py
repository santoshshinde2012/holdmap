#!/usr/bin/env python3
"""A tiny, real MCP stdio server for holdmap's tests (tests/mcp_analyzers.rs).

It speaks newline-delimited JSON-RPC 2.0. It offers one tool, scan_skill(path), which reports
SARIF for lines that tell an agent to ignore its instructions. It also reports what the test
needs to check about its containment: any HOLDMAP_* environment variable it was given, and
whether it could write outside the folder it was handed. It never uses the network.
Test fixture only, not an analyzer anyone should install.
"""
import json
import re
import os
import sys

TOOL = {
    "name": "scan_skill",
    "description": "Scan one skill folder (holdmap test fixture).",
    "inputSchema": {
        "type": "object",
        "properties": {"path": {"type": "string"}, "format": {"type": "string"}},
        "required": ["path"],
    },
}


def result(path):
    found = []
    for root, _dirs, files in os.walk(path):
        for name in sorted(files):
            full = os.path.join(root, name)
            try:
                with open(full, encoding="utf-8", errors="replace") as f:
                    for n, line in enumerate(f, 1):
                        if re.search(r"ignore (all )?previous instructions", line, re.I):
                            found.append(("instruction-override", "error", os.path.relpath(full, path), n,
                                          "Text tells the agent to ignore its instructions."))
            except OSError:
                pass
    for key in sorted(os.environ):
        if key.startswith("HOLDMAP_"):
            found.append(("env-leak", "error", "", 1, "The server was given " + key + "."))
    probe = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".write-probe")
    try:
        with open(probe, "w") as f:
            f.write("x")
        os.remove(probe)
        found.append(("wrote-outside", "warning", "", 1, "The server could write outside its folder."))
    except OSError:
        pass
    results = [{
        "ruleId": rule,
        "level": level,
        "message": {"text": text},
        "locations": [{"physicalLocation": {"artifactLocation": {"uri": uri}, "region": {"startLine": line}}}] if uri else [],
    } for rule, level, uri, line, text in found]
    return {"version": "2.1.0", "runs": [{"tool": {"driver": {"name": "mcp-scan-fixture"}}, "results": results}]}


def reply(msg_id, body=None, error=None):
    out = {"jsonrpc": "2.0", "id": msg_id}
    if error is not None:
        out["error"] = error
    else:
        out["result"] = body
    sys.stdout.write(json.dumps(out) + "\n")
    sys.stdout.flush()


for raw in sys.stdin:
    raw = raw.strip()
    if not raw:
        continue
    msg = json.loads(raw)
    method, msg_id = msg.get("method"), msg.get("id")
    if msg_id is None:
        continue  # notifications, e.g. notifications/initialized
    params = msg.get("params") or {}
    if method == "initialize":
        reply(msg_id, {
            "protocolVersion": params.get("protocolVersion", "2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "mcp-scan-fixture", "version": "1"},
        })
    elif method == "ping":
        reply(msg_id, {})
    elif method == "tools/list":
        reply(msg_id, {"tools": [TOOL]})
    elif method == "tools/call" and params.get("name") == "scan_skill":
        sarif = result(params.get("arguments", {}).get("path", ""))
        reply(msg_id, {"content": [{"type": "text", "text": json.dumps(sarif)}], "structuredContent": sarif, "isError": False})
    else:
        reply(msg_id, error={"code": -32601, "message": "method not found"})
