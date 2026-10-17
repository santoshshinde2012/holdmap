import { describe, expect, it } from "vitest";
import { attention, overview } from "./home";
import { MOCK_SNAPSHOT, mockAgents } from "./mock";
import type { Agent, AgentsReport, PortEntry } from "./types";

const entries = MOCK_SNAPSHOT.entries;
const report = mockAgents();

function withAgent(patch: Partial<Agent>, base: AgentsReport = report): AgentsReport {
  return { ...base, agents: [{ ...base.agents[0], ...patch }, ...base.agents.slice(1)] };
}

describe("overview", () => {
  it("counts ports, dev servers, exposed listeners and agents", () => {
    const o = overview(entries, report);
    expect(o.ports).toBe(entries.length);
    expect(o.devServers).toBe(entries.filter((e) => e.is_dev).length);
    expect(o.exposed).toBe(entries.filter((e) => e.exposure === "all_interfaces").length);
    expect(o.agents).toBe(report.agents.length);
  });

  it("keeps missing data unknown instead of zero", () => {
    expect(overview(null, null)).toEqual({ agents: null, devServers: null, ports: null, exposed: null });
  });
});

describe("attention", () => {
  it("flags root agents and agent ports on all interfaces, high severity first", () => {
    const first = report.agents[0];
    const port = { ...first.ports[0] ?? { entry_id: "x", port: 5173, protocol: "tcp", pid: 1, process: null, label: "vite", role: "dev_server", project: null, framework: null }, exposure: "all_interfaces" as const };
    const r = withAgent({ access: { ...first.access, root: true }, ports: [port] });
    const items = attention([], r);
    expect(items.slice(0, 2).map((i) => i.id).sort()).toEqual([`exposed:${first.id}`, `root:${first.id}`]);
    expect(items[0].severity).toBe("high");
    expect(items[0].target).toEqual({ view: "agents", agentId: first.id });
  });

  it("reports an exposed dev server not owned by an agent, linking to the port", () => {
    const e: PortEntry = { ...entries[0], id: "loose", is_dev: true, exposure: "all_interfaces" };
    const items = attention([e], { ...report, agents: [] });
    expect(items).toEqual([expect.objectContaining({ id: "exposed-port:loose", target: { view: "list", entryId: "loose" } })]);
  });

  it("ignores loopback listeners and missing reports", () => {
    expect(attention([{ ...entries[0], exposure: "loopback" }], null)).toEqual([]);
  });
});
