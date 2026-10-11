# Agent flow and local recording

Current source / Unreleased adds **Agents → Flow** in the desktop and shared browser demo.
Version 0.4.0 downloads predate this view. Build current source to use native recording.

## Read the flow

Start with **Connections**. Nodes represent observed agents, child processes, listeners and
network endpoints. Edges distinguish parent relationships, listener ownership and observed TCP
sockets. Select an edge to see its source PIDs and evidence. Process lineage can identify a
likely MCP server, including a stdio server without a listening port; it cannot establish that
an agent invoked a particular tool. Socket counts measure observed connections, not messages,
traffic direction, payload bytes or agent intent. Remote SSH reports do not enable recording
or local power actions on the remote machine.

Switch to **Calls** for explicitly reported activity. The run picker separates recorded traces
from sessions without parent spans. A graph edge requires a reported parent within the same
trace. Parallel tools occupy distinct records. Agent and handoff spans can show delegation when
the exporter supplies that relationship; common session membership and event order cannot
establish internal communication. The timeline remains useful when no causal graph exists.
A missing parent or unknown outcome stays visible instead of being filled in. Unsupported
signals are rejected and counted as dropped records. The failed execution step beneath a Claude tool wrapper is distinct from that wrapper.

Select a node or timeline item to inspect its source, status, duration and available serialized
input/output sizes. These sizes describe the reported serialized value or, for Holdmap MCP,
the individual compact JSON-RPC envelope. They are not network transfer totals. Provider events
without size fields show **Unknown**. Recorded identities are pseudonyms and do not authorize
opening folders, stopping services or any other process action.

Search keeps matching relationships in context. Fit, zoom and the accessible participant list
support navigating larger graphs. The inspector stays readable in narrow windows. **Pause**
freezes view updates; recording continues until **Stop recording**. Refresh remains available
while paused. Navigation discards late responses so another view cannot overwrite the current
selection or reuse an old port selection for keyboard actions.

## Start a desktop recording

1. Open **Agents → Flow → Calls**, then **Start recording** on the local desktop.
2. Open **Setup**, select Codex or Claude, and copy the reviewed configuration example.
3. Apply the example yourself and restart that agent in a new terminal. Keep Holdmap open.
4. Run agent work, then inspect reported calls. Exporter batching and completed-span delivery
   can delay arrival. A stop closes the endpoint; unmatched requests become unknown outcomes.
5. Clear retained metadata when finished. A full app quit loses all recording metadata.

Setup never edits agent configuration automatically. Each new recording has an OS-assigned loopback
port and a fresh 256-bit bearer token. The token appears only in explicitly requested setup or
CLI credentials; it is absent from status, graphs, snapshots and errors. Treat copied setup as
private and remove stale agent exporter settings when finished. Recording is not persistent
across a full quit/restart and does not start again automatically.

The public website uses labelled examples. It cannot start a collector or perform OS actions.

## CLI and hooks

Start a collector in one terminal:

```sh
holdmap activity serve
# Optionally limit its lifetime:
holdmap activity serve --duration 1h --json
```

Use the printed endpoint and token in a second terminal. The values below are placeholders,
not a working recording session:

```sh
export HOLDMAP_ACTIVITY_ENDPOINT='http://127.0.0.1:PORT'
export HOLDMAP_ACTIVITY_TOKEN='TOKEN_FROM_ACTIVITY_SERVE'
holdmap activity setup --provider codex
holdmap activity setup --provider claude
holdmap activity snapshot --json
holdmap mcp --record-activity
```

For PowerShell, set the equivalent `$env:HOLDMAP_ACTIVITY_ENDPOINT` and
`$env:HOLDMAP_ACTIVITY_TOKEN`, then request `activity setup --provider claude --windows`.
`activity serve` stays active until Ctrl+C or its optional duration. The desktop and CLI
collectors are separate sessions: configure producers for the collector you intend to inspect.

`holdmap activity hook --provider codex` or `--provider claude` consumes a supported hook's JSON
from stdin, projects only metadata, and sends it to that collector. Configure lifecycle hooks
explicitly according to the provider documentation, including tool failure events where
supported. Reuse each provider's tool-use ID so parallel calls with the same name stay separate.
Hooks return no stdout and treat recording errors as advisory, preserving the agent's approval
and execution flow. A pre-tool hook means **Requested**, since it can run before permission is
granted. Missing completion is not success. A Codex PostToolUse with an explicit nonzero numeric exit code is a failure; missing
or ambiguous outcome fields remain unknown. Do not enable hooks and telemetry for the same events unless you
intend to inspect separate source records; this release does not deduplicate across sources.

MCP recording is opt-in per Holdmap server invocation. The observer receives fixed catalog
names, a local call ordinal, times, outcome and serialized envelope sizes. It receives no
arguments, results, error messages, JSON-RPC IDs or client metadata. A bounded worker posts
metadata without putting diagnostics into protocol stdout. Missing collector configuration and
recording failures leave MCP tools operational. Calls to other servers require their own
instrumentation; this observer cannot see them.

## Provider coverage and research

The implementation uses provider wire contracts and a synthetic fixture generated by the
actual Codex OTel exporter, not private conversations or agent logs.

| Source | What is supported | Limits that affect the picture |
|---|---|---|
| OS snapshot | Process ancestry, listeners and observed TCP peers | No tool calls, messages, encrypted payloads or stdio exchanges |
| Codex OTLP JSON logs | Known tool result events, reported call/session IDs, result and duration | Session grouping is not a parent trace; finished events arrive in batches |
| Claude enhanced OTLP JSON traces | Agent/model/tool wrappers, execution steps and supplied trace parents | Provider beta; spans arrive after completion, and dropped parents remain missing |
| Codex/Claude hooks | Supported lifecycle events with tool-use IDs | Requested state precedes execution; no guessed causal edges |
| Holdmap MCP observer | This server's validated catalog calls and outcomes | Does not observe other MCP servers or general agent messages |
| Generic OTLP JSON traces | Recognized agent, model, tool and execution-step operations with supplied parents | Exporter-specific coverage; no automatic SDK installation or universal capture |

Codex's configuration documents opt-in OpenTelemetry and prompt logging controls.
[Advanced configuration](https://developers.openai.com/codex/config-advanced/) and the
[configuration reference](https://developers.openai.com/codex/config-reference/) guide the
reviewable JSON log exporter. The current exporter emits numeric duration as a text attribute,
uses `conversation.id`, and can put a source location in top-level `eventName`; normalization
uses the supported `event.name` attribute and reported event timestamp. The regression fixture
records those details, with provenance pinned to
[the Codex source](https://github.com/openai/codex/tree/8c0f7011674145da7e10627769fb221c74fbf7a0/codex-rs/otel).

[Codex hooks](https://developers.openai.com/codex/hooks/) describe call identity and lifecycle
coverage. Hooks may arrive out of order, and some hosted operations have no local tool hook.
Normalization therefore pairs by identity and protects completed outcomes from late starts.

[Claude monitoring](https://code.claude.com/docs/en/monitoring-usage) documents enhanced tracing,
parent propagation, content controls and completed-span delivery. Setup selects trace export
only, disables content flags, detailed beta tracing and the shell raw-body variable. Use Claude
Code v2.1.212 or newer for Content-Length HTTP export; older chunked exporters are refused. Existing user
or managed raw-body settings must also be disabled; managed policy can override local exporter
configuration. Review the effective destination before launching. Tool wrappers retain unknown
outcome when the source leaves it unset; an execution child supplies its own result.

[MCP transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
and [tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) distinguish
newline-delimited stdio, protocol errors and tool errors. The observer preserves existing
protocol negotiation and output while recording those outcomes separately. No new discovery
or agent execution capability is introduced.

[OpenTelemetry GenAI conventions](https://github.com/open-telemetry/semantic-conventions-genai)
provide operation and trace relationships. Instrumentation remains dependent on each producer.
An OpenAI Agents SDK trace processor would need an explicit exporter bridge; this release does
not claim a built-in SDK adapter. Gemini and other recognized agents retain passive OS
visibility; their hook contracts are not silently treated as Codex/Claude contracts.

## Privacy, security and bounds

The collector binds only `127.0.0.1` and requires its session bearer token. It rejects browser
Origins, nonlocal Host headers, ambiguous framing, chunked bodies, unsupported compression,
oversized requests and unsupported endpoints. It has four workers, eight queued connections,
an absolute one-second request deadline, a 256 KiB body limit, a 256-record request limit and
a 120-request-per-second admission limit. Setup uses JSON, no compression and smaller export
batches. Large individual producer payloads can still exceed the limit and be rejected.

Producers may transmit rich OTLP or hook payloads to the local parser. The parser immediately
projects allowlisted metadata, discarding prompts, arguments, results, raw errors, headers,
paths and unrelated attributes before storage. Identifiers become domain-separated SHA-256
pseudonyms. Names are bounded and sanitized; known credential-shaped names are rejected.
Sanitization is not a guarantee that an arbitrary custom label is safe, so use metadata-only
exporters and keep source content controls disabled. The collector is not a general-purpose
telemetry proxy and forwards nothing externally.

Explicit normalized metadata at `/v1/activity` can also report handoff records with agent
identities. Generic OTLP operation parsing does not invent a handoff from an arbitrary span.

Retention is at most 1,000 spans in memory. Large runs render at most 128 graph nodes, show the
omitted count, and keep every retained record accessible in the timeline; selecting another
record brings its reported parent context into the graph. Counters and limits disclose evicted or rejected
records; OTLP partial-success responses report rejected records. Snapshots are bounded and
authenticated. Source snapshots cannot write extra fields into the strict normalized schema.
Conflicting immutable identities are refused, and late starts cannot erase a completed failure.
A collector stop joins its bounded workers. No persistence, traffic interception, chat scanning,
remote collector, replayed execution or automatic stop is provided.

## Design and validation

`holdmap-core::agents::flow` is a pure projection from one consistent scan and agent report.
`holdmap-activity` separates contracts, source adapters, metadata policy, memory storage,
transport, client and setup generation. Hook and OTLP adapters share validation and sanitization;
typed routes have exhaustive dispatch and one credential/request-limit policy. Configuration
generation validates credentials without constructing an HTTP client.

`holdmap-mcp::observer` is a narrow injectable observation interface with a no-op default.
The CLI's `activity/mcp.rs` adapts observer events and owns bounded asynchronous delivery;
activity commands handle CLI arguments, hooks and rendering. Native recording commands share
one authorized, serialized session boundary. Svelte `RecordingControls` owns recording/setup
actions and `AgentFlowView` owns graph/run selection; graph building, layout and polling remain
separate. Unknown recording status is shown explicitly. These boundaries apply SOLID through
concrete responsibilities without adding an interface for every function.

Tests exercise actual loopback HTTP, slow/partial requests, hostile framing and payloads,
parallel same-name hooks, reversed completion, the Codex exporter fixture, MCP protocol output,
source-field exclusion, native opt-in lifecycle, stale asynchronous responses, graph evidence,
keyboard isolation, both themes and narrow layouts. They use owned fixtures or injected effects,
never real shutdown or stops of user processes. See [architecture](architecture.md) and
[contributor checks](../CONTRIBUTING.md) for the complete validation workflow.
