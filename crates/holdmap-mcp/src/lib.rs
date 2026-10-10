#![warn(missing_docs)]
//! # holdmap-mcp
//!
//! A dependency-light [Model Context Protocol](https://modelcontextprotocol.io) server over stdio.
//! Discovery metadata, validation and live execution have separate responsibilities. Holdmap
//! core owns process inspection and stop policy; this adapter validates every tool input first.
//! Protected and other users' processes remain refused, with no agent override.

mod catalog;
mod confirmation;
mod prompts;
mod resources;
mod stops;
mod tools;
mod validation;

#[cfg(test)]
mod tests;

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use tools::ToolError;

const SUPPORTED_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
// Bound allocations from a client before parsing or dispatching any request. The oldest
// supported protocols permit batches, so their work also needs a separate per-frame bound.
const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_BATCH_MESSAGES: usize = 64;

enum Frame {
    Message(Vec<u8>),
    TooLarge,
}

/// Read and, when necessary, discard one newline-delimited frame without retaining more
/// than the limit. An oversized frame cannot become a second, valid request after truncation.
fn read_frame<R: BufRead>(input: &mut R) -> io::Result<Option<Frame>> {
    let mut message = Vec::new();
    let mut too_large = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(if too_large {
                Some(Frame::TooLarge)
            } else if message.is_empty() {
                None
            } else {
                Some(Frame::Message(message))
            });
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let length = newline.unwrap_or(available.len());
        if !too_large {
            if length > MAX_FRAME_BYTES.saturating_sub(message.len()) {
                message.clear();
                too_large = true;
            } else {
                message.extend_from_slice(&available[..length]);
            }
        }
        input.consume(length + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(Some(if too_large {
                Frame::TooLarge
            } else {
                Frame::Message(message)
            }));
        }
    }
}

struct Session {
    version: &'static str,
    confirmations: confirmation::Confirmations,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: SUPPORTED_VERSIONS[0],
            confirmations: confirmation::Confirmations::default(),
        }
    }
}

/// Run the server until stdin closes, retaining the negotiated protocol version.
///
/// Frames are limited to 1 MiB and legacy batches to 64 messages. Oversized frames are
/// discarded completely, receive an error, and cannot dispatch tools or desynchronize stdin.
pub fn serve<R: BufRead, W: Write>(mut input: R, mut output: W) -> io::Result<()> {
    let mut session = Session::default();
    while let Some(frame) = read_frame(&mut input)? {
        let response = match frame {
            Frame::TooLarge => Some(session.compatible(error(
                Value::Null,
                -32600,
                "MCP frame exceeds 1 MiB; no requests from this frame were executed",
            ))),
            Frame::Message(line) if line.iter().all(u8::is_ascii_whitespace) => continue,
            Frame::Message(line) => match serde_json::from_slice::<Value>(&line) {
            Ok(Value::Array(batch)) if batch.len() > MAX_BATCH_MESSAGES => {
                Some(session.compatible(error(
                    Value::Null,
                    -32600,
                    "MCP batches are limited to 64 messages; no requests from this batch were executed",
                )))
            }
            Ok(Value::Array(batch)) if session.version < "2025-06-18" && !batch.is_empty() => {
                let responses: Vec<Value> = batch
                    .into_iter()
                    .filter_map(|message| session.handle(message))
                    .collect();
                if responses.is_empty() {
                    None
                } else {
                    Some(Value::Array(responses))
                }
            }
            Ok(message) => session.handle(message),
            Err(e) => {
                Some(session.compatible(error(Value::Null, -32700, &format!("parse error: {e}"))))
            }
            },
        };
        if let Some(response) = response {
            writeln!(output, "{}", serde_json::to_string(&response)?)?;
            output.flush()?;
        }
    }
    Ok(())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Handle one standalone JSON-RPC message using the latest supported version.
///
/// Returns `None` for valid notifications. Use [`serve`] to retain version negotiation
/// across a stdio session, including compatibility with older clients and stop confirmation
/// handles. Standalone calls do not retain previews for later execution.
pub fn handle(message: Value) -> Option<Value> {
    Session::default().handle(message)
}

impl Session {
    fn handle(&mut self, message: Value) -> Option<Value> {
        let valid_id = message
            .get("id")
            .filter(|id| id.is_string() || id.is_number());
        let response_id = valid_id.cloned().unwrap_or(Value::Null);
        let valid_envelope = message.is_object()
            && message["jsonrpc"] == "2.0"
            && message["method"].is_string()
            && message.get("params").is_none_or(Value::is_object)
            && (message.get("id").is_none() || valid_id.is_some());
        if !valid_envelope {
            return Some(self.compatible(error(response_id, -32600, "Invalid JSON-RPC request: expected an object with jsonrpc=2.0, a method string, object params and a string or number id")));
        }
        if let Some(key) = message
            .as_object()
            .expect("validated request")
            .keys()
            .find(|key| !matches!(key.as_str(), "jsonrpc" | "id" | "method" | "params"))
        {
            return Some(self.compatible(error(
                response_id,
                -32600,
                &format!(
                    "Unknown request field `{key}`; tool inputs belong inside params.arguments"
                ),
            )));
        }
        let id = message.get("id")?.clone(); // Valid notifications have no response or side effects.
        let method = message["method"].as_str().expect("validated method");
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        let response = match method {
            "initialize" => self.initialize(id, &params),
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({"tools": catalog::list()})),
            "tools/call" => call(id, &params, &mut self.confirmations),
            "resources/list" => ok(id, resources::list()),
            "resources/templates/list" => ok(id, json!({"resourceTemplates": []})),
            "resources/read" => match params["uri"].as_str().filter(|uri| !uri.is_empty()) {
                None => error(id, -32602, "resources/read requires a nonempty string uri"),
                Some(uri) => match resources::read(uri) {
                    Some(resource) => ok(id, resource),
                    None => error(id, -32002, &format!("Resource not found: {uri}")),
                },
            },
            "prompts/list" => ok(id, prompts::list()),
            "prompts/get" => match prompts::get(&params) {
                Ok(prompt) => ok(id, prompt),
                Err(message) => error(id, -32602, &message),
            },
            _ => error(id, -32601, &format!("method not found: {method}")),
        };
        Some(self.compatible(response))
    }

    fn initialize(&mut self, id: Value, params: &Value) -> Value {
        let Some(requested) = params["protocolVersion"].as_str() else {
            return error(id, -32602, "initialize requires a string protocolVersion");
        };
        self.confirmations.clear();
        self.version = SUPPORTED_VERSIONS
            .iter()
            .copied()
            .find(|version| *version == requested)
            .unwrap_or(SUPPORTED_VERSIONS[0]);
        ok(
            id,
            json!({
                "protocolVersion": self.version,
                "capabilities": {
                    "tools": {"listChanged": false},
                    "resources": {"subscribe": false, "listChanged": false},
                    "prompts": {"listChanged": false}
                },
                "serverInfo": {"name": "holdmap", "title": "Holdmap", "version": holdmap_core::VERSION},
                "instructions": catalog::INSTRUCTIONS
            }),
        )
    }

    fn compatible(&self, mut response: Value) -> Value {
        // Since 2025-11-25 an uncorrelated error has no id, rather than id:null.
        if self.version >= "2025-11-25" && response["error"].is_object() && response["id"].is_null()
        {
            response
                .as_object_mut()
                .expect("JSON-RPC response object")
                .remove("id");
        }
        if self.version < "2025-06-18" {
            if let Some(result) = response.get_mut("result").and_then(Value::as_object_mut) {
                result.remove("structuredContent");
                if let Some(info) = result.get_mut("serverInfo").and_then(Value::as_object_mut) {
                    info.remove("title");
                }
                for collection in ["tools", "resources", "prompts"] {
                    if let Some(items) = result.get_mut(collection).and_then(Value::as_array_mut) {
                        for item in items.iter_mut().filter_map(Value::as_object_mut) {
                            item.remove("title");
                            item.remove("outputSchema");
                            if self.version < "2025-03-26" {
                                item.remove("annotations");
                            }
                        }
                    }
                }
            }
        }
        response
    }
}

fn call(id: Value, params: &Value, confirmations: &mut confirmation::Confirmations) -> Value {
    for key in params.as_object().expect("validated request params").keys() {
        if key == "task" {
            return error(
                id,
                -32602,
                "Task-augmented execution is not supported by this server",
            );
        }
        if !matches!(key.as_str(), "name" | "arguments" | "_meta") {
            return error(
                id,
                -32602,
                &format!(
                    "Unknown tools/call parameter `{key}`; tool inputs belong inside arguments"
                ),
            );
        }
    }
    if params
        .get("_meta")
        .is_some_and(|metadata| !metadata.is_object())
    {
        return error(id, -32602, "tools/call _meta must be an object");
    }
    if params["_meta"]
        .get("progressToken")
        .is_some_and(|token| !token.is_string() && !token.is_number())
    {
        return error(
            id,
            -32602,
            "tools/call _meta.progressToken must be a string or number",
        );
    }
    let Some(name) = params["name"].as_str().filter(|name| !name.is_empty()) else {
        return error(id, -32602, "tools/call requires a nonempty string name");
    };
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match tools::call(name, &args, confirmations) {
        Ok((summary, data)) => ok(
            id,
            json!({
                "content": [
                    {"type": "text", "text": summary},
                    {"type": "text", "text": serde_json::to_string_pretty(&data).unwrap_or_default()}
                ],
                "structuredContent": data,
                "isError": false
            }),
        ),
        Err(ToolError::Unknown) => error(id, -32602, &format!("unknown tool: {name}")),
        Err(ToolError::Failed(message)) => ok(
            id,
            json!({
                "content": [{"type": "text", "text": message}],
                "isError": true
            }),
        ),
    }
}
