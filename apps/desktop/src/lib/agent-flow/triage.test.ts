import { describe, expect, it } from "vitest";
import { activityRuns, graphWindow, spanNodeId } from "./build";
import { activityContext, activityRows } from "./triage";
import type { ActivitySpan } from "./model";

const span = (values: Partial<ActivitySpan> = {}): ActivitySpan => ({
  id: "root", trace_id: "trace-a", parent_id: null, session_id: null, agent_id: null, parent_agent_id: null,
  provider: "open_telemetry", source: "otlp", kind: "agent", name: "Agent interaction", start_at_ms: 1000,
  end_at_ms: 2000, duration_ms: 1000, status: "unknown", input_bytes: null, output_bytes: null,
  pid: null, process_started_at: null, ...values,
});

describe("recorded activity triage", () => {
  it("separates reported failures, missing completions and finished unknown outcomes", () => {
    const run = activityRuns([
      span(), span({ id: "failed", status: "error" }),
      span({ id: "requested", status: "requested", end_at_ms: null, duration_ms: null }),
      span({ id: "running", status: "running", end_at_ms: null, duration_ms: null }),
      span({ id: "unfinished-unknown", status: "unknown", end_at_ms: null, duration_ms: null }),
      span({ id: "cancelled", status: "cancelled" }),
    ])[0];
    expect(activityRows(run, "", "failed").map((s) => s.id)).toEqual(["failed"]);
    expect(activityRows(run, "", "incomplete").map((s) => s.id)).toEqual(["requested", "running", "unfinished-unknown"]);
    expect(activityRows(run, "", "all")).toHaveLength(6);
  });

  it("combines metadata search with the selected filter without changing a recorded run", () => {
    const run = activityRuns([span(), span({ id: "read", kind: "tool", name: "Filesystem read", status: "error", pid: 123 })])[0];
    const before = structuredClone(run);
    expect(activityRows(run, "filesystem pid:123", "failed").map((s) => s.id)).toEqual(["read"]);
    expect(activityRows(run, "filesystem pid:12", "failed")).toEqual([]);
    expect(activityRows(run, "OpenTelemetry agent telemetry", "failed")).toHaveLength(1);
    expect(run).toEqual(before);
  });

  it("orders by reported duration with observed zero before unknown durations and stable time ties", () => {
    const records = [
      span({ id: "unknown", duration_ms: null, start_at_ms: 500 }),
      span({ id: "zero", duration_ms: 0, start_at_ms: 600 }),
      span({ id: "later", duration_ms: 500, start_at_ms: 1500 }),
      span({ id: "earlier", duration_ms: 500, start_at_ms: 1000 }),
    ];
    const run = activityRuns(records)[0];
    expect(activityRows(run, "", "all", "duration").map((s) => s.id)).toEqual(["earlier", "later", "zero", "unknown"]);
    expect(activityRows(run).map((s) => s.id)).toEqual(["unknown", "zero", "earlier", "later"]);
    expect(run.spans).toEqual(records);
  });

  it("places unavailable timestamps after known starts or completion-only times", () => {
    const run = activityRuns([
      span({ id: "unavailable", start_at_ms: null, end_at_ms: null }),
      span({ id: "end-only", start_at_ms: null, end_at_ms: 500 }),
      span({ id: "start", start_at_ms: 1000 }),
    ])[0];
    expect(activityRows(run).map((s) => s.id)).toEqual(["end-only", "start", "unavailable"]);
  });

  it("shows matching spans and their reported ancestors without unrelated siblings", () => {
    const records = [span(), span({ id: "read", kind: "tool", parent_id: "root" }),
      span({ id: "failure", kind: "step", parent_id: "read", status: "error" }),
      span({ id: "other", kind: "tool", parent_id: "root" })];
    const run = activityRuns(records)[0], graph = activityContext(run, activityRows(run, "", "failed"), null);
    expect(graph.nodes.map((n) => n.span?.id)).toEqual(["root", "read", "failure"]);
    expect(graph.edges).toHaveLength(2);
    expect(graph.edges.every((e) => e.label === "Reported parent")).toBe(true);
  });

  it("retains an inspected span or edge and explicitly reported ancestry after filters hide it", () => {
    const records = [span(), span({ id: "child", parent_id: "root", kind: "tool" })];
    const run = activityRuns(records)[0];
    expect(activityContext(run, [], spanNodeId(records[1])).nodes.map((n) => n.span?.id)).toEqual(["root", "child"]);
    expect(activityContext(run, [], `parent:${spanNodeId(records[1])}`).edges).toHaveLength(1);
    expect(activityContext(run, [], "unavailable")).toEqual({ nodes: [], edges: [] });
  });

  it("does not manufacture parents from session membership or adjacent event timestamps", () => {
    const records = [span({ id: "first", trace_id: "", session_id: "session" }),
      span({ id: "second", trace_id: "", session_id: "session", parent_id: "first", start_at_ms: 1001, status: "error" })];
    const run = activityRuns(records)[0], graph = activityContext(run, activityRows(run, "", "failed"), null);
    expect(graph.nodes.map((n) => n.span?.id)).toEqual(["second"]);
    expect(graph.edges).toEqual([]);
  });

  it("bounds a large filtered graph while retaining a selected late record and the full source", () => {
    const records = Array.from({ length: 1000 }, (_, i) => span({ id: `record-${i}`, kind: i === 0 ? "agent" : "tool",
      parent_id: i === 0 ? null : "record-0", status: i % 2 ? "error" : "unknown" }));
    const run = activityRuns(records)[0], rows = activityRows(run, "", "failed"), chosen = spanNodeId(records[998]);
    const graph = graphWindow(activityContext(run, rows, chosen), chosen).graph;
    expect(rows).toHaveLength(500); expect(graph.nodes).toHaveLength(128);
    expect(graph.nodes.some((n) => n.id === chosen)).toBe(true);
    expect(graph.nodes.some((n) => n.span?.id === "record-0")).toBe(true);
    expect(run.spans).toHaveLength(1000);
  });

  it("keeps an inspected reported relationship when its endpoints exceed the graph window", () => {
    const records = Array.from({ length: 200 }, (_, i) => span({ id: `record-${i}`, parent_id: i === 0 ? null : "record-0" }));
    const run = activityRuns(records)[0], chosen = `parent:${spanNodeId(records[199])}`;
    const graph = graphWindow(activityContext(run, activityRows(run), chosen), chosen).graph;
    expect(graph.nodes).toHaveLength(128);
    expect(graph.edges.some((edge) => edge.id === chosen)).toBe(true);
    expect(graph.nodes.some((node) => node.span?.id === "record-199")).toBe(true);
  });

  it("terminates on explicitly reported parent cycles and accepts empty recordings", () => {
    const run = activityRuns([span({ id: "a", parent_id: "b" }), span({ id: "b", parent_id: "a" })])[0];
    expect(activityContext(run, [run.spans[0]], null).nodes).toHaveLength(2);
    expect(activityRows(null)).toEqual([]);
    expect(activityContext(null, [], null)).toEqual({ nodes: [], edges: [] });
  });
});
