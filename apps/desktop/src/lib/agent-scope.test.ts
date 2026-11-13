import { describe, expect, it } from "vitest";
import { MOCK_SNAPSHOT } from "./mock";
import { mockAgents } from "./mock-agents";
import { demoActivity, demoAgentFlow } from "./activity-demo";
import { activityRuns } from "./agent-flow/build";
import { agentForEntry, agentForPids, agentRuns, agentSocketLinks, scopeFlowReport, spanBelongsTo } from "./agent-scope";

describe("agent scope", () => {
  const report = mockAgents();
  const [claude, cursor] = [report.agents.find((a) => a.product === "claude-code")!, report.agents.find((a) => a.product === "cursor")!];

  it("keeps an agent's own flow nodes and their direct neighbours only", () => {
    const flow = demoAgentFlow(report);
    const scoped = scopeFlowReport(flow, cursor)!;
    expect(scoped.nodes.length).toBeLessThan(flow.nodes.length);
    expect(scoped.nodes.some((n) => n.agent_id === cursor.id)).toBe(true);
    expect(scoped.nodes.some((n) => n.kind === "agent" && n.agent_id !== cursor.id)).toBe(false);
    const ids = new Set(scoped.nodes.map((n) => n.id));
    expect(scoped.edges.every((e) => ids.has(e.from) && ids.has(e.to))).toBe(true);
    expect(scopeFlowReport(null, cursor)).toBeNull();
  });

  it("lists only live TCP relationships for the agent", () => {
    const links = agentSocketLinks(demoAgentFlow(report), cursor);
    expect(links.length).toBe(cursor.links.length);
    expect(links.every((l) => l.edge.kind === "tcp")).toBe(true);
  });

  it("attributes reported spans by agent, owned PID or provider", () => {
    const spans = demoActivity().spans;
    expect(spans.some((s) => spanBelongsTo(s, claude))).toBe(true);
    expect(spans.some((s) => spanBelongsTo(s, cursor))).toBe(false);
    expect(spanBelongsTo({ ...spans[0], pid: cursor.pid }, cursor)).toBe(true);
    const runs = activityRuns(spans);
    expect(agentRuns(runs, cursor)).toEqual([]);
    expect(agentRuns(runs, claude).length).toBeGreaterThan(0);
  });

  it("finds the agent that owns a port or a process", () => {
    const port = cursor.ports.find((p) => p.entry_id)!;
    const entry = MOCK_SNAPSHOT.entries.find((e) => e.id === port.entry_id)!;
    expect(agentForEntry(report.agents, entry)?.id).toBe(cursor.id);
    expect(agentForPids(report.agents, [cursor.pid])?.id).toBe(cursor.id);
    expect(agentForPids(report.agents, [1])).toBeNull();
    expect(agentForPids(report.agents, [])).toBeNull();
  });
});
