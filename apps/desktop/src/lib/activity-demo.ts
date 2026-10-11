/** Browser examples only. The renderer labels these recordings as demo data. */
import type { AgentsReport } from "./types";
import type { ActivitySnapshot, ActivitySpan, AgentFlowNode, AgentFlowReport, RecordingStatus } from "./agent-flow/model";

const anchor = Date.now() - 20_000;
function span(id: string, name: string, kind: ActivitySpan["kind"], start: number, end: number | null, overrides: Partial<ActivitySpan> = {}): ActivitySpan {
  return { id, name, kind, trace_id: "demo-agent-workflow", parent_id: null, session_id: "demo-session", agent_id: null, parent_agent_id: null,
    provider: "open_telemetry", source: "otlp", start_at_ms: anchor + start, end_at_ms: end == null ? null : anchor + end,
    duration_ms: end == null ? null : end - start, status: end == null ? "requested" : "ok", input_bytes: null, output_bytes: null,
    pid: null, process_started_at: null, ...overrides };
}
const examples: ActivitySpan[] = [
  span("agent", "Planning agent", "agent", 0, 9200),
  span("model", "Model request", "model", 40, 720, { parent_id: "agent", input_bytes: 2014, output_bytes: 640 }),
  span("read", "mcp__filesystem__read_file", "tool", 800, 1420, { parent_id: "agent", input_bytes: 72, output_bytes: 6144 }),
  span("search", "mcp__search__query", "tool", 840, 1840, { parent_id: "agent", input_bytes: 128, output_bytes: 2560 }),
  span("delegate", "Delegate to reviewer", "handoff", 1900, 7650, { parent_id: "agent" }),
  span("reviewer", "Review agent", "agent", 2100, 7400, { parent_id: "delegate", agent_id: "demo-reviewer", parent_agent_id: "demo-planner" }),
  span("review-model", "Model request", "model", 2200, 2680, { parent_id: "reviewer", agent_id: "demo-reviewer" }),
  span("test", "Run checks", "tool", 2900, 7100, { parent_id: "reviewer", agent_id: "demo-reviewer", status: "unknown", input_bytes: 90 }),
  span("test-execution", "Tool execution", "step", 3050, 7100, { parent_id: "test", agent_id: "demo-reviewer", status: "error", output_bytes: 384 }),
  span("summarize", "Model request", "model", 7800, 9150, { parent_id: "agent", input_bytes: 4408, output_bytes: 1280 }),
  span("pending", "mcp__browser__open", "tool", 11000, null, { trace_id: "", session_id: "demo-hook-session", source: "hook", provider: "codex", status: "requested", input_bytes: 96 }),
];
let cleared = false, active = false;
let started: number | null = null;
export function demoActivity(): ActivitySnapshot {
  return { spans: cleared ? [] : examples.map((item) => ({ ...item })), received: cleared ? 0 : examples.length, dropped: 0,
    limits: ["Browser demo: these records are simulated. No local recorder or agent integration runs in the browser.", "Sizes are serialized payload sizes; no message content is included."] };
}
export function demoRecordingStatus(): RecordingStatus {
  return { active, endpoint: null, started_at_ms: started, received: demoActivity().received, dropped: 0 };
}
export function startDemoRecording() { active = true; started = Date.now(); cleared = false; }
export function stopDemoRecording() { active = false; started = null; }
export function clearDemoActivity() { cleared = true; }

export function demoAgentFlow(report: AgentsReport): AgentFlowReport {
  const nodes: AgentFlowNode[] = [], edges: AgentFlowReport["edges"] = [];
  const ids = new Set<string>();
  const add = (node: AgentFlowNode) => { if (!ids.has(node.id)) { ids.add(node.id); nodes.push(node); } };
  const base = { entry_id: null, address: null, port: null, protocol: null, tool_kind: null };
  for (const agent of report.agents) {
    const root = `process:${agent.pid}:${agent.started_at}`;
    add({ ...base, id: root, kind: "agent", label: agent.name, agent_id: agent.id, pid: agent.pid, start_token: agent.started_at, evidence: "observed" });
    const processIds = new Map<number, string>([[agent.pid, root]]);
    for (const tool of agent.tools) {
      const id = `process:${tool.pid}:${agent.started_at}`;
      processIds.set(tool.pid, id);
      add({ ...base, id, kind: "tool", label: tool.name, agent_id: agent.id, pid: tool.pid, start_token: agent.started_at, tool_kind: tool.kind, evidence: tool.evidence });
    }
    for (const tool of agent.tools) {
      const parent = processIds.get(tool.ppid ?? agent.pid) ?? root;
      const id = processIds.get(tool.pid)!;
      edges.push({ id: `parent:${id}`, from: parent, to: id, kind: "parent", evidence: "observed", connections: 0, source_pids: [] });
    }
    for (const port of agent.ports) {
      const id = `listener:${port.entry_id}`;
      add({ ...base, id, kind: "listener", label: port.label, agent_id: agent.id, pid: port.pid, start_token: null,
        entry_id: port.entry_id, address: "127.0.0.1", port: port.port, protocol: port.protocol, evidence: "observed" });
      const holder = processIds.get(port.pid ?? agent.pid) ?? root;
      edges.push({ id: `owns:${holder}:${id}`, from: holder, to: id, kind: "owns_listener", evidence: "observed", connections: 0, source_pids: port.pid == null ? [] : [port.pid] });
    }
    for (const link of agent.links) {
      const id = `endpoint:${link.address}:${link.port}`;
      add({ ...base, id, kind: link.kind === "remote" ? "remote" : "listener", label: link.label, agent_id: null,
        pid: link.pid, start_token: null, entry_id: link.entry_id, address: link.address, port: link.port, protocol: "tcp", evidence: "observed" });
      edges.push({ id: `tcp:${root}:${id}`, from: root, to: id, kind: "tcp", evidence: "observed", connections: link.connections, source_pids: [agent.pid] });
    }
  }
  return { taken_at_ms: report.taken_at_ms, nodes, edges, omitted_nodes: 0, omitted_edges: 0,
    limits: ["Browser demo: these process and socket relationships are simulated.", "A parent process or TCP peer does not prove a tool invocation or message direction."] };
}
