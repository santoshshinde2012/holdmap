import { describe, expect, it } from "vitest";
import { capabilityItems, runningMcpItems } from "./agent-capabilities";
import { buildCapabilityMap, CAPABILITY_MAP_LIMITS, CAPABILITY_MAP_NODE_HEIGHT, CAPABILITY_MAP_NODE_WIDTH } from "./agent-capability-map";
import { mockAgents } from "./mock";
import type { Agent } from "./types";

const template = mockAgents(undefined, true).agents[0];
function agent(id: string): Agent {
  return { ...structuredClone(template), id, name: `Agent ${id}`, pid: 50_000 + Number(id.replace(/\D/g, "") || 0), tools: [],
    capabilities: { inspected: true, mcp_servers: [], skills: [], limits: [], more_mcp_servers: 0, more_skills: 0 } };
}
function mcp(owner: Agent, name: string, source = `/config/${owner.id}.json`) {
  owner.capabilities!.mcp_servers.push({ id: `${owner.id}:${name}`, name, source_path: source, scope: "user", project_path: null,
    transport: "stdio", enabled: null, evidence: "observed" });
}
function skill(owner: Agent, name: string, source: string) {
  owner.capabilities!.skills.push({ id: `${owner.id}:${name}`, name, source_path: source, scope: "user", project_path: null, evidence: "observed" });
}
function runtime(owner: Agent, pid: number, name = "filesystem") {
  owner.tools.push({ pid, ppid: owner.pid, name, kind: "mcp_server", command: "private-command-fixture", cwd: null,
    evidence: "observed", ports: [], memory_bytes: 1, cpu_percent: 0 });
}
const inputFor = (agents: Agent[]) => ({ agents, items: capabilityItems(agents), running: runningMcpItems(agents) });

describe("capability ownership map", () => {
  it("shares an exact declaration and skill file once while connecting every real owner", () => {
    const a = agent("a"), b = agent("b");
    for (const owner of [a, b]) {
      mcp(owner, "filesystem", "/config/shared.json");
      skill(owner, "review", "/skills/review/SKILL.md");
    }
    const input = inputFor([a, b]);
    const graph = buildCapabilityMap(input);
    expect(graph.nodes.map((node) => node.kind)).toEqual(["mcp", "agent", "agent", "skill"]);
    expect(graph.edges).toHaveLength(4);
    for (const item of input.items) {
      expect(graph.nodes.find((node) => node.id === item.id)?.ownerIds).toEqual([a.id, b.id]);
      expect(graph.edges.filter((edge) => edge.to === item.id).map((edge) => edge.from).sort()).toEqual([a.id, b.id]);
    }
    expect(graph.edges.map((edge) => edge.label)).toContain("Configuration observed");
    expect(graph.edges.map((edge) => edge.label)).toContain("Skill file observed");
    expect(graph.omitted).toEqual({ agents: 0, inventory: 0, running: 0, ownerEdges: 0 });
  });

  it("keeps repeated aliases in different sources and same-name runtime observations separate", () => {
    const a = agent("a"), b = agent("b");
    mcp(a, "filesystem"); mcp(b, "filesystem"); runtime(a, 900, "filesystem");
    const graph = buildCapabilityMap(inputFor([a, b]));
    expect(graph.nodes.filter((node) => node.kind === "mcp")).toHaveLength(2);
    const candidate = graph.nodes.find((node) => node.kind === "running")!;
    expect(candidate.ownerIds).toEqual([a.id]);
    expect(candidate.item).toBeUndefined();
    expect(graph.edges).toHaveLength(2);
    expect(graph.edges.every((edge) => edge.to !== candidate.id && edge.from !== candidate.id)).toBe(true);
    expect(graph.lanes.find((lane) => lane.id === "running")?.sub).toContain("separate from definitions");
  });

  it("shows empty and unsupported agents with honest inventory states", () => {
    const empty = agent("empty"), missing = agent("missing"), unsupported = agent("unsupported");
    delete missing.capabilities;
    unsupported.capabilities!.inspected = false;
    const graph = buildCapabilityMap(inputFor([unsupported, missing, empty]));
    expect(graph.nodes.filter((node) => node.kind === "agent")).toHaveLength(3);
    expect(graph.nodes.find((node) => node.id === empty.id)).toMatchObject({ inventoryState: "inspected", emptyHint: "No entries reported" });
    expect(graph.nodes.find((node) => node.id === missing.id)).toMatchObject({ inventoryState: "not_reported", emptyHint: "Inventory not reported" });
    expect(graph.nodes.find((node) => node.id === unsupported.id)).toMatchObject({ inventoryState: "not_inspected", emptyHint: "Sources not inspected" });
    expect(buildCapabilityMap({ ...inputFor([empty, missing]), filtered: true, selectedAgent: empty.id }).nodes.map((node) => node.id)).toEqual([empty.id]);
    expect(buildCapabilityMap({ ...inputFor([empty, missing]), filtered: true }).nodes).toEqual([]);
    empty.capabilities!.more_skills = 5;
    expect(buildCapabilityMap(inputFor([empty])).nodes[0].emptyHint).toBe("5 entries omitted by discovery");
  });

  it("filters agents to exact visible owners and explicit selection without inventing matches", () => {
    const a = agent("a"), b = agent("b"), unrelated = agent("other"), selected = agent("selected");
    mcp(a, "shared", "/config/shared.json"); mcp(b, "shared", "/config/shared.json"); mcp(unrelated, "other");
    const input = inputFor([a, b, unrelated, selected]);
    const graph = buildCapabilityMap({ ...input, items: input.items.filter((item) => item.name === "shared"), filtered: true, selectedAgent: selected.id });
    expect(graph.nodes.filter((node) => node.kind === "agent").map((node) => node.id)).toEqual([a.id, b.id, selected.id]);
    expect(graph.edges.map((edge) => edge.from).sort()).toEqual([a.id, b.id]);
    const noMatches = buildCapabilityMap({ ...input, items: [], filtered: true, selectedAgent: unrelated.id });
    expect(noMatches.nodes.map((node) => node.id)).toEqual([unrelated.id]);
    expect(noMatches.nodes[0].emptyHint).toBe("No matching definitions");
  });

  it("keeps layout and signature stable across input order and resource refreshes", () => {
    const a = agent("a"), b = agent("b");
    mcp(a, "zeta"); mcp(b, "alpha"); runtime(a, 900);
    const original = buildCapabilityMap(inputFor([a, b]));
    const refreshed = structuredClone([b, a]);
    for (const owner of refreshed) {
      owner.memory_bytes = 99_999; owner.cpu_percent = 99; owner.command = "private-refresh-command";
      for (const tool of owner.tools) { tool.memory_bytes = 88_888; tool.cpu_percent = 88; }
    }
    const input = inputFor(refreshed);
    const next = buildCapabilityMap({ ...input, items: [...input.items].reverse(), running: [...input.running].reverse(), selectedId: a.id });
    expect(next.signature).toBe(original.signature);
    expect(next.positions).toEqual(original.positions);
    expect(next.nodes.find((node) => node.id === a.id)?.agent?.memory_bytes).toBe(99_999);
    expect(next.signature).not.toContain("private-");
    expect(next.nodes.filter((node) => node.kind === "mcp").map((node) => node.label)).toEqual(["alpha", "zeta"]);
  });

  it("bounds large reports, retains explicit selections and accounts for omitted associations", () => {
    const agents = Array.from({ length: 35 }, (_, index) => agent(String(index).padStart(2, "0")));
    for (const owner of agents) {
      for (let index = 0; index < 3; index++) mcp(owner, `server-${owner.id}-${index}`);
      for (let index = 0; index < 2; index++) runtime(owner, 90_000 + Number(owner.id) * 2 + index, `runtime-${owner.id}-${index}`);
    }
    const input = inputFor(agents), selected = input.items.find((item) => item.owners[0].id === "33")!;
    const graph = buildCapabilityMap({ ...input, filtered: true, selectedId: selected.id, selectedAgent: "34" });
    expect(graph.shown.agents).toBeLessThanOrEqual(CAPABILITY_MAP_LIMITS.agents);
    expect(graph.shown.inventory).toBe(CAPABILITY_MAP_LIMITS.inventory);
    expect(graph.shown.running).toBe(CAPABILITY_MAP_LIMITS.running);
    expect(graph.total).toEqual({ agents: 35, inventory: 105, running: 70 });
    expect(graph.omitted).toEqual({ agents: graph.total.agents - graph.shown.agents, inventory: 41, running: 46, ownerEdges: 41 });
    expect(graph.nodes.some((node) => node.id === selected.id)).toBe(true);
    expect(graph.nodes.some((node) => node.id === "34")).toBe(true);
    expect(graph.nodes.find((node) => node.id === "34")?.emptyHint).not.toBe("No matching definitions");
    expect(graph.edges.every((edge) => graph.positions.has(edge.from) && graph.positions.has(edge.to))).toBe(true);
    for (const node of graph.nodes.filter((node) => node.kind === "agent" && node.id !== "34")) {
      expect(graph.nodes.some((entry) => entry.ownerIds.includes(node.id))).toBe(true);
    }
    expect(graph.nodes.length).toBeLessThanOrEqual(112);
  });

  it("retains an empty explicit agent before a selected shared definition's many owners", () => {
    const agents = Array.from({ length: 30 }, (_, index) => agent(String(index).padStart(2, "0")));
    for (const owner of agents.slice(0, -1)) mcp(owner, "shared", "/config/shared.json");
    agents.at(-1)!.capabilities!.inspected = false;
    const input = inputFor(agents), selected = input.items[0];
    const graph = buildCapabilityMap({ ...input, filtered: true, selectedAgent: "29", selectedId: selected.id });
    expect(graph.nodes.find((node) => node.id === "29")?.emptyHint).toBe("Sources not inspected");
    expect(graph.nodes.find((node) => node.id === selected.id)?.ownerIds).toHaveLength(29);
    expect(graph.shown.agents).toBe(24);
    expect(graph.edges).toHaveLength(23);
    expect(graph.omitted).toEqual({ agents: 6, inventory: 0, running: 0, ownerEdges: 6 });
  });

  it("retains a selected runtime candidate beyond the cap without adding runtime edges", () => {
    const owner = agent("owner");
    for (let index = 0; index < 40; index++) runtime(owner, 90_000 + index, `runtime-${String(index).padStart(2, "0")}`);
    const input = inputFor([owner]), chosen = input.running.at(-1)!;
    const graph = buildCapabilityMap({ ...input, selectedId: chosen.id });
    expect(graph.nodes.some((node) => node.id === chosen.id)).toBe(true);
    expect(graph.shown.running).toBe(24);
    expect(graph.omitted.running).toBe(16);
    expect(graph.edges).toEqual([]);
  });

  it("bounds geometry for long labels, reserves lane headings and never overlaps cards", () => {
    const owner = agent("owner"); owner.name = "界".repeat(400);
    mcp(owner, "long-server-".repeat(80)); skill(owner, "long-skill-".repeat(80), "/skills/long/SKILL.md"); runtime(owner, 900);
    const graph = buildCapabilityMap(inputFor([owner]));
    for (const node of graph.nodes) {
      const lane = graph.lanes.find((lane) => lane.id === (node.kind === "agent" ? "agents" : node.kind))!;
      expect(node.width).toBe(CAPABILITY_MAP_NODE_WIDTH); expect(node.height).toBe(CAPABILITY_MAP_NODE_HEIGHT);
      expect(node.position.y).toBeGreaterThanOrEqual(lane.y + 72);
      expect(node.position.x + node.width).toBeLessThanOrEqual(lane.x + lane.width);
      expect(node.position.y + node.height).toBeLessThanOrEqual(lane.y + lane.height);
      expect(graph.positions.get(node.id)).toEqual(node.position);
    }
    for (let a = 0; a < graph.nodes.length; a++) for (let b = a + 1; b < graph.nodes.length; b++) {
      const first = graph.nodes[a], second = graph.nodes[b];
      expect(first.position.x + first.width <= second.position.x || second.position.x + second.width <= first.position.x
        || first.position.y + first.height <= second.position.y || second.position.y + second.height <= first.position.y).toBe(true);
    }
  });

  it("lays MCP and skills beside agents, with runtime below agents and no empty columns", () => {
    const owner = agent("owner");
    mcp(owner, "filesystem"); skill(owner, "review", "/skills/review/SKILL.md"); runtime(owner, 900);
    const graph = buildCapabilityMap(inputFor([owner]));
    const node = (kind: string) => graph.nodes.find((node) => node.kind === kind)!;
    expect(node("mcp").position.x).toBeLessThan(node("agent").position.x);
    expect(node("agent").position.x).toBeLessThan(node("skill").position.x);
    expect(node("running").position.x).toBe(node("agent").position.x);
    expect(node("running").position.y).toBeGreaterThan(node("agent").position.y + node("agent").height);
    expect(graph.edges.find((edge) => edge.to === node("mcp").id)?.sourceHandle).toBe("mcp");
    expect(graph.edges.find((edge) => edge.to === node("skill").id)?.sourceHandle).toBe("skill");
    const input = inputFor([owner]);
    const mcpOnly = buildCapabilityMap({ ...input, items: input.items.filter((item) => item.kind === "mcp"), running: [] });
    expect(mcpOnly.lanes.map((lane) => lane.id)).toEqual(["mcp", "agents"]);
    expect(mcpOnly.bounds.width).toBeLessThan(graph.bounds.width);
    const emptyAgent = buildCapabilityMap({ ...input, items: [], running: [], selectedAgent: owner.id, filtered: true });
    expect(emptyAgent.lanes.map((lane) => lane.id)).toEqual(["agents"]);
    expect(emptyAgent.nodes.map((node) => node.id)).toEqual([owner.id]);
    expect(emptyAgent.bounds.width).toBeLessThan(mcpOnly.bounds.width);
    const empty = buildCapabilityMap({ agents: [], items: [], running: [] });
    expect(empty.lanes).toEqual([]);
    expect(empty.bounds).toEqual({ width: 0, height: 0 });
  });
});
