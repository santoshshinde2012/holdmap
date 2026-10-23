import { describe, expect, it } from "vitest";
import { activityMatches, activityName, activityRuns, activityStatus, callGraph, duration, graphSignature, graphWindow, isIncomplete, missingParent, observedGraph, payloadSize, spanNodeId } from "./build";
import { LABEL_EDGE_LIMIT, layoutGraph, toFlow } from "./layout";
import type { ActivitySpan, AgentFlowNode, AgentFlowReport } from "./model";

const span = (values: Partial<ActivitySpan> = {}): ActivitySpan => ({ id: "root", trace_id: "trace-a", parent_id: null,
  session_id: null, agent_id: null, parent_agent_id: null, provider: "claude", source: "otlp", kind: "agent", name: "Agent interaction",
  start_at_ms: 1000, end_at_ms: 2000, duration_ms: 1000, status: "ok", input_bytes: null, output_bytes: null,
  pid: null, process_started_at: null, ...values });
const node = (id: string, kind: AgentFlowNode["kind"], values: Partial<AgentFlowNode> = {}): AgentFlowNode => ({ id, kind, label: id,
  agent_id: null, pid: null, start_token: null, entry_id: null, address: null, port: null, protocol: null, tool_kind: null, evidence: "observed", ...values });
const report = (): AgentFlowReport => ({ taken_at_ms: 1000,
  nodes: [node("agent:123:456", "agent", { label: "Claude Code", pid: 123, start_token: 456 }), node("tool:124:457", "tool", { label: "Filesystem MCP", pid: 124, tool_kind: "mcp_server", evidence: "inferred" }), node("tcp:3000", "listener", { label: "Web server", address: "127.0.0.1", port: 3000, protocol: "tcp" }), node("udp:3000", "listener", { label: "UDP server", address: "127.0.0.1", port: 3000, protocol: "udp" }), node("remote", "remote", { label: "Remote endpoint", address: "192.0.2.1", port: 443 })],
  edges: [{ id: "parent", from: "agent:123:456", to: "tool:124:457", kind: "parent", evidence: "observed", connections: 0, source_pids: [] }, { id: "listener", from: "tool:124:457", to: "tcp:3000", kind: "owns_listener", evidence: "observed", connections: 0, source_pids: [] }, { id: "socket", from: "tool:124:457", to: "remote", kind: "tcp", evidence: "observed", connections: 2, source_pids: [124] }],
  limits: ["No message payloads collected"], omitted_nodes: 0, omitted_edges: 0 });

describe("observed agent connections", () => {
  it("preserves socket sources, evidence and protocol without claiming message direction", () => {
    const g = observedGraph(report());
    expect(g.nodes.find((n) => n.id === "tool:124:457")?.evidence).toBe("inferred");
    expect(g.nodes.find((n) => n.id === "tcp:3000")?.sub).toContain("TCP");
    expect(g.nodes.find((n) => n.id === "udp:3000")?.sub).toContain("UDP");
    expect(g.edges.find((e) => e.id === "socket")).toMatchObject({ label: "2 live sockets", observed: { source_pids: [124] } });
    expect(g.edges.map((e) => e.label).join(" ")).not.toMatch(/message|invocation|bytes|traffic/i);
    expect(toFlow(g, layoutGraph(g, false), null).edges.every((e) => e.markerEnd == null && e.animated !== true)).toBe(true);
  });
  it("labels every edge on a small graph, but only the picked card's edges on a busy one", () => {
    const small = observedGraph(report());
    expect(toFlow(small, layoutGraph(small, false), null).edges.every((e) => e.data?.labelled)).toBe(true);
    const busy = { ...small, edges: Array.from({ length: LABEL_EDGE_LIMIT + 1 }, (_, i) => ({ ...small.edges[i % small.edges.length], id: `e${i}` })) };
    expect(toFlow(busy, layoutGraph(busy, false), null).edges.some((e) => e.data?.labelled)).toBe(false);
    const picked = toFlow(busy, layoutGraph(busy, false), "e2").edges.filter((e) => e.data?.labelled).map((e) => e.id);
    expect(picked).toEqual(["e2"]);
  });
  it("searches process identity and retains only adjacent relationship context", () => {
    const g = observedGraph(report(), "124");
    expect(g.nodes.map((n) => n.id)).toEqual(["agent:123:456", "tool:124:457", "tcp:3000", "remote"]);
    expect(observedGraph(report(), "missing").nodes).toEqual([]);
    expect(observedGraph(report(), "pid:124").nodes.map((n) => n.id)).toEqual(g.nodes.map((n) => n.id));
    expect(observedGraph(report(), "port:3000").nodes.map((n) => n.id)).toEqual(["tool:124:457", "tcp:3000", "udp:3000"]);
    expect(observedGraph(report(), "filesystem mcp").nodes.map((n) => n.id)).toEqual(g.nodes.map((n) => n.id));
    expect(observedGraph(report(), "pid:12").nodes).toEqual([]);
  });
  it("drops dangling edges without manufacturing an unknown participant", () => {
    const r = report(); r.edges.push({ ...r.edges[0], id: "dangling", to: "missing" });
    expect(observedGraph(r).edges.some((e) => e.id === "dangling")).toBe(false);
  });
  it("keeps layout identity stable when live socket counts change", () => {
    const r = report(), before = observedGraph(r); r.edges[2].connections = 17;
    const after = observedGraph(r);
    expect(graphSignature(after)).toBe(graphSignature(before));
    expect(layoutGraph(after, false)).toEqual(layoutGraph(before, false));
  });
});

describe("reported call correlation", () => {
  it("counts explicit skill activations separately and searches reported MCP aliases without inventing joins", () => {
    const records = [
      span({ id: "skill", kind: "skill", name: "Skill", trace_id: "", session_id: "reported", status: "unknown", start_at_ms: null, duration_ms: null }),
      span({ id: "mcp", kind: "tool", name: "lookup", mcp_server: "filesystem", trace_id: "", session_id: "reported" }),
    ];
    const run = activityRuns(records)[0];
    expect(run).toMatchObject({ calls: 1, skills: 1, errors: 0 });
    expect(activityName(records[0])).toBe("Skill activation (name not reported)");
    expect(activityStatus(records[0])).toBe("Activation reported");
    expect(isIncomplete(records[0])).toBe(false);
    expect(activityMatches(records[1], "MCP filesystem")).toBe(true);
    expect(callGraph(run).edges).toEqual([]);
  });

  it("represents nested and parallel calls using only provided parent span IDs", () => {
    const runs = activityRuns([span(), span({ id: "read", parent_id: "root", kind: "tool", name: "Read" }), span({ id: "search", parent_id: "root", kind: "tool", name: "Search", start_at_ms: 1200 }), span({ id: "execution", parent_id: "read", kind: "step", name: "Execution" })]);
    const graph = callGraph(runs[0]);
    expect(runs[0].calls).toBe(2);
    expect(graph.edges).toHaveLength(3);
    const positions = layoutGraph(graph, true);
    expect(positions.get(spanNodeId(runs[0].spans[1]))?.x).toBe(positions.get(spanNodeId(runs[0].spans[2]))?.x);
    expect(positions.get(spanNodeId(runs[0].spans[3]))?.x).toBeGreaterThan(positions.get(spanNodeId(runs[0].spans[1]))!.x);
  });
  it("groups hook records by session without fabricating parent edges", () => {
    const records = [span({ trace_id: "", session_id: "session", source: "hook", id: "first", parent_id: null }), span({ trace_id: "", session_id: "session", source: "hook", id: "second", parent_id: "first", kind: "tool" })];
    const runs = activityRuns(records);
    expect(runs[0]).toMatchObject({ kind: "session", label: "Recorded session", calls: 1 });
    expect(callGraph(runs[0]).edges).toEqual([]);
  });
  it("keeps uncorrelated events separate and does not infer communication from time", () => {
    const runs = activityRuns([span({ trace_id: "", id: "a", start_at_ms: 1000 }), span({ trace_id: "", id: "b", start_at_ms: 1001 })]);
    expect(runs).toHaveLength(2); expect(runs.every((r) => r.kind === "event")).toBe(true);
    expect(runs.flatMap((r) => callGraph(r).edges)).toEqual([]);
  });
  it("retains a failed execution step while leaving the tool wrapper status unknown", () => {
    const runs = activityRuns([span({ id: "tool", kind: "tool", status: "unknown" }), span({ id: "execution", kind: "step", parent_id: "tool", status: "error" })]);
    const graph = callGraph(runs[0]);
    expect(runs[0].calls).toBe(1); expect(runs[0].errors).toBe(1);
    expect(graph.nodes.find((n) => n.span?.id === "tool")?.status).toBe("unknown");
    expect(graph.nodes.find((n) => n.span?.id === "execution")?.status).toBe("error");
  });
  it("never borrows a parent from a different trace and describes missing parents", () => {
    const runs = activityRuns([span({ id: "root", trace_id: "other" }), span({ id: "child", parent_id: "root", kind: "tool" })]);
    const child = runs.find((r) => r.spans[0].id === "child")!;
    expect(callGraph(child).edges).toHaveLength(0); expect(missingParent(child.spans[0], child)).toBe(true);
  });
  it("does not equate requested activity with completed execution", () => {
    const s = span({ kind: "tool", status: "requested", end_at_ms: null, duration_ms: null });
    expect(isIncomplete(s)).toBe(true); expect(duration(s)).toBe("No completion received");
  });
  it("bounds cyclic parent layout and drops self-parent edges", () => {
    const run = activityRuns([span({ id: "a", parent_id: "b" }), span({ id: "b", parent_id: "a" }), span({ id: "self", parent_id: "self" })])[0];
    const graph = callGraph(run), positions = layoutGraph(graph, true);
    expect(graph.edges).toHaveLength(2);
    expect([...positions.values()].every((p) => Number.isFinite(p.x) && p.x <= 8 * 340)).toBe(true);
  });
  it("bounds a 1000-span graph while preserving selected reported ancestry and the full run", () => {
    const records = Array.from({ length: 1000 }, (_, i) => span({ id: `record-${i}`, kind: i === 0 ? "agent" : "tool", parent_id: i === 0 ? null : `record-${i - 1}` }));
    const run = activityRuns(records)[0], full = callGraph(run), chosen = spanNodeId(records[999]);
    const view = graphWindow(full, chosen);
    expect(view.graph.nodes).toHaveLength(128); expect(view.omitted).toBe(872);
    expect(view.graph.nodes.some((n) => n.id === chosen)).toBe(true);
    expect(view.graph.nodes.some((n) => n.id === spanNodeId(records[998]))).toBe(true);
    const visible = new Set(view.graph.nodes.map((n) => n.id));
    expect(view.graph.edges.every((e) => visible.has(e.from) && visible.has(e.to))).toBe(true);
    expect(run.spans).toHaveLength(1000); expect(full.nodes).toHaveLength(1000);
  });
  it("keeps status updates out of the structural layout signature", () => {
    const s = span({ status: "running", end_at_ms: null }), first = callGraph(activityRuns([s])[0]);
    const updated = callGraph(activityRuns([{ ...s, status: "ok", end_at_ms: 2000 }])[0]);
    expect(graphSignature(first)).toBe(graphSignature(updated));
  });
  it("distinguishes unknown payload sizes from observed zero bytes", () => {
    expect(payloadSize(null)).toBe("Not provided"); expect(payloadSize(0)).toBe("0 B"); expect(payloadSize(2048)).toBe("2.0 KiB");
  });
  it("searches metadata and counts tool spans rather than collector updates or execution steps", () => {
    const records = [span(), span({ id: "tool", kind: "tool", name: "Filesystem read" }), span({ id: "step", kind: "step", name: "Execution" })];
    expect(activityRuns(records, "filesystem")[0].calls).toBe(1);
    expect(activityRuns(records, "notfound")).toEqual([]);
    expect(activityRuns([span({ pid: 123, kind: "tool" })], "pid:123")).toHaveLength(1);
    expect(activityRuns([span({ pid: 123 })], "pid:12")).toHaveLength(0);
  });
});
