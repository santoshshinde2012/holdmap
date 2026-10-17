import { describe, expect, it } from "vitest";
import { attention, attentionAgent, hotAgents, overview } from "./home";
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
    expect(overview(null, null)).toEqual({ agents: null, devServers: null, ports: null, exposed: null, mcpConfigured: null, mcpRunning: null });
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

describe("configuration rules", () => {
  const inspected = mockAgents(undefined, true);

  it("counts configured and running MCP servers only from inspected agents", () => {
    const o = overview(entries, inspected);
    expect(o.mcpConfigured).toBeGreaterThan(0);
    expect(o.mcpRunning).toBeLessThanOrEqual(o.mcpConfigured!);
    expect(overview(entries, report).mcpConfigured).toBeNull();
  });

  it("flags unpinned fetching servers and settings that skip approvals", () => {
    const first = inspected.agents[0];
    const caps = first.capabilities!;
    const r = withAgent({ capabilities: { ...caps,
      mcp_servers: [{ id: "g", name: "github", scope: "user", source_path: "/h/.claude.json", project_path: null, transport: "stdio", enabled: null, evidence: "observed", launch: { runner: "npx", package: "@modelcontextprotocol/server-github", pinned: false } }],
      settings: [{ key: "permissions.defaultMode", value: "bypassPermissions", scope: "user", source_path: "/Users/dev/.claude/settings.json" }],
    } }, inspected);
    const items = attention([], r).filter((i) => i.target.view === "agents" && i.target.agentId === first.id);
    expect(items.map((i) => i.rule).sort()).toEqual(["settings", "unpinned"]);
    expect(items.find((i) => i.rule === "unpinned")!.detail).toBe("github (npx @modelcontextprotocol/server-github) can change on the next launch.");
    expect(items.find((i) => i.rule === "settings")!.detail).toBe("permissions.defaultMode = bypassPermissions in .claude/settings.json");
  });
});

describe("attention ownership", () => {
  it("ties port items to the agent holding the port and keeps hot agents to agent-level rules", () => {
    const r = mockAgents(undefined, true);
    const items = attention(entries, r);
    const docker = r.agents.find((a) => a.product === "docker-desktop")!;
    const exposed = items.find((i) => i.rule === "exposed")!;
    expect(attentionAgent(exposed, r)).toBe(docker.id);
    expect(attentionAgent({ ...exposed, target: { view: "list", entryId: docker.ports[0].entry_id } }, r)).toBe(docker.id);
    expect(attentionAgent({ ...exposed, target: { view: "list", entryId: "nobody" } }, r)).toBeNull();
    expect(hotAgents(items).size).toBe(0);
    expect(hotAgents([{ ...exposed, rule: "root" }]).has(docker.id)).toBe(true);
  });
});

describe("overview rail and actions", () => {
  it("offers a stop plan only where stopping is the usual fix", () => {
    const items = attention(entries, mockAgents(undefined, true));
    for (const i of items) {
      if (i.rule === "exposed" || i.rule === "exposed-port") expect(i.stop).toBeTruthy();
      else expect(i.stop).toBeUndefined();
    }
  });
});

describe("unowned dev servers", () => {
  it("lists dev servers no agent started, once", async () => {
    const { unownedDevServers } = await import("./home");
    const { MOCK_SNAPSHOT, mockAgents } = await import("./mock");
    const report = mockAgents(undefined, true);
    const owned = new Set(report.agents.flatMap((a) => a.ports.map((p) => p.entry_id)));
    const loose = unownedDevServers(MOCK_SNAPSHOT.entries, report);
    expect(loose.length).toBeGreaterThan(0);
    expect(loose.every((e) => e.is_dev && !owned.has(e.id))).toBe(true);
    expect(unownedDevServers(null, report)).toEqual([]);
  });
});
