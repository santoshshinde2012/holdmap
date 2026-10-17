import { describe, expect, it } from "vitest";
import { mockAgents } from "./mock";
import { agentMatches } from "./agents";
import { capabilityContextGroups, capabilityItems, capabilityLabel, capabilityLimits, capabilityMatches, enabledLabel, filterCapabilities, runningMcpItems } from "./agent-capabilities";
import type { Agent } from "./types";

const report = () => mockAgents(undefined, true);

function sameContext(id: string): Agent {
  const agent = structuredClone(report().agents[1]);
  agent.id = id; agent.pid = 60_000 + Number(id.replace(/\D/g, "") || 0);
  agent.tools = [];
  return agent;
}

describe("capability context grouping", () => {
  it("groups identical source contexts without claiming a shared session or losing members", () => {
    const agents = [sameContext("root-3"), sameContext("root-1"), sameContext("root-4"), sameContext("root-2")];
    const groups = capabilityContextGroups(agents);
    expect(groups).toHaveLength(1);
    expect(groups[0].agents.map(agent => agent.id)).toEqual(["root-1", "root-2", "root-3", "root-4"]);
    expect(groups[0].representative).toBe(agents[1]);
    expect(groups[0].name).toBe("Cursor");
    expect(groups[0].projectPaths).toContain("/Users/dev/code/shop-web");
    expect(capabilityItems(groups[0].agents).every(item => item.owners.length === 4)).toBe(true);
  });

  it("keeps identity and order stable across samples, member changes and reordered metadata", () => {
    const a = sameContext("a"), b = sameContext("b");
    const original = capabilityContextGroups([b, a])[0];
    b.memory_bytes += 10_000; b.cpu_percent = 99; b.pid = 99_000;
    b.command = "private-command-not-an-identity"; b.started_at += 300;
    b.folders.reverse(); b.capabilities!.mcp_servers.reverse(); b.capabilities!.skills.reverse(); b.capabilities!.limits.reverse();
    // Provider-local IDs are not a different source observation.
    b.capabilities!.skills[0].id = "different-provider-local-id";
    const next = capabilityContextGroups([a, b])[0];
    expect(next.id).toBe(original.id);
    expect(next.agents.map(agent => agent.id)).toEqual(["a", "b"]);
    expect(capabilityContextGroups([sameContext("replacement")])[0].id).toBe(original.id);
    expect(next.id).not.toContain("private-");
  });

  it("separates exact product, account and observed project/source differences", () => {
    const base = sameContext("base");
    const mutations: Array<(agent: Agent) => void> = [
      agent => { agent.product += "-different"; },
      agent => { agent.vendor += "-different"; },
      agent => { agent.name += " different"; },
      agent => { agent.access.uid = 777; },
      agent => { agent.folders[0].path += "/other"; },
      agent => { agent.capabilities!.mcp_servers[0].source_path += ".other"; },
      agent => { agent.capabilities!.mcp_servers[0].enabled = !agent.capabilities!.mcp_servers[0].enabled; },
      agent => { agent.capabilities!.skills[0].description = "Another purpose"; },
    ];
    for (const mutate of mutations) {
      const other = sameContext("other"); mutate(other);
      expect(capabilityContextGroups([base, other])).toHaveLength(2);
    }
  });

  it("keeps every differing inspection, coverage and omission state separate", () => {
    const base = sameContext("base");
    const mutations: Array<(agent: Agent) => void> = [
      agent => { delete agent.capabilities; },
      agent => { agent.capabilities!.inspected = false; },
      agent => { agent.capabilities!.limits.push("A bounded source could not be inspected."); },
      agent => { agent.capabilities!.more_skills++; },
      agent => { agent.capabilities!.more_mcp_servers++; },
      agent => { agent.more_folders++; },
      agent => { agent.more_tools++; },
    ];
    for (const mutate of mutations) {
      const other = sameContext("other"); mutate(other);
      expect(capabilityContextGroups([base, other])).toHaveLength(2);
    }
    const missing = sameContext("missing"), uninspected = sameContext("uninspected");
    delete missing.capabilities;
    uninspected.capabilities = { inspected: false, mcp_servers: [], skills: [], limits: [], more_mcp_servers: 0, more_skills: 0 };
    expect(capabilityContextGroups([missing, uninspected])).toHaveLength(2);
  });

  it("excludes recent history from current project identity and retains runtime owners independently", () => {
    const a = sameContext("a"), b = sameContext("b");
    b.folders.push({ ...structuredClone(b.folders[0]), path: "/history/another-project", source: "recent" });
    const runtimeTemplate = report().agents[1].tools.find(tool => tool.kind === "mcp_server")!;
    a.tools.push({ ...structuredClone(runtimeTemplate), pid: 80_001 });
    b.tools.push({ ...structuredClone(runtimeTemplate), pid: 80_002, name: "different-runtime" });
    const groups = capabilityContextGroups([a, b]);
    expect(groups).toHaveLength(1);
    expect(groups[0].projectPaths).not.toContain("/history/another-project");
    const running = runningMcpItems(groups[0].agents);
    expect(running.map(item => item.tool.pid).sort()).toEqual([80_001, 80_002]);
    expect(running.find(item => item.tool.pid === 80_001)?.owners.map(owner => owner.id)).toEqual(["a"]);
    expect(running.find(item => item.tool.pid === 80_002)?.owners.map(owner => owner.id)).toEqual(["b"]);
  });
});

describe("capability inventory projections", () => {
  it("keeps default scans free of configuration inventory", () => {
    expect(mockAgents().agents.every((agent) => !agent.capabilities)).toBe(true);
    expect(capabilityItems(mockAgents().agents)).toEqual([]);
  });

  it("models unsupported providers as uninspected, never an empty successful inventory", () => {
    const unsupported = report().agents.find((agent) => agent.name === "Docker Desktop")!;
    expect(unsupported.capabilities?.inspected).toBe(false);
    expect(capabilityItems([unsupported])).toEqual([]);
    expect(capabilityLimits([unsupported]).join(" ")).toContain("not supported for Docker Desktop");
  });

  it("preserves same-name declarations in different sources and keeps runtime independent", () => {
    const agents = report().agents;
    const rows = capabilityItems(agents);
    expect(rows.filter((row) => row.name === "filesystem")).toHaveLength(2);
    expect(new Set(rows.map((row) => row.id)).size).toBe(rows.length);
    const runtime = runningMcpItems(agents);
    expect(runtime).toHaveLength(1);
    expect(runtime[0].tool.pid).toBe(52070);
    expect(rows.every((row) => !row.id.startsWith("running:"))).toBe(true);
    expect(rows.every((row) => !("tool" in row))).toBe(true);
  });

  it("gives repeated aliases distinct accessible names with full source and owner context", () => {
    const rows = capabilityItems(report().agents).filter((row) => row.name === "filesystem");
    const names = rows.map(capabilityLabel);
    expect(new Set(names).size).toBe(2);
    expect(names).toContain("Inspect MCP server filesystem, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.cursor/mcp.json");
    expect(names).toContain("Inspect MCP server filesystem, Project scope. Agents: Claude Code (PID 51200). Project: /Users/dev/code/shop-api. Source: /Users/dev/code/shop-api/.mcp.json");
  });

  it("shares exact declarations across agent instances and retains all associations", () => {
    const agent = report().agents[1];
    const copy = { ...structuredClone(agent), id: "another-instance", pid: 62000 };
    const rows = capabilityItems([agent, copy]);
    expect(rows).toHaveLength(5);
    expect(rows.every((row) => row.owners.map((owner) => owner.pid).join(",") === "52000,62000")).toBe(true);
    expect(capabilityItems([copy, agent]).map((row) => row.id)).toEqual(rows.map((row) => row.id));
  });

  it("never turns unknown enablement into enabled, or a skill file into a running process", () => {
    const rows = capabilityItems(report().agents);
    const unknown = rows.find((row) => row.owners[0].name === "Claude Code" && row.kind === "mcp")!;
    expect(enabledLabel(unknown)).toBe("Enablement unknown");
    expect(filterCapabilities(rows, "", "all", "all", "enabled")).toHaveLength(2);
    expect(filterCapabilities(rows, "", "all", "all", "disabled").map((row) => row.name)).toEqual(["legacy-browser"]);
    const unknownRows = filterCapabilities(rows, "", "all", "all", "unknown");
    expect(unknownRows[0]).toEqual(unknown);
    expect(unknownRows.every((row) => row.owners[0].name === "Claude Code" && row.kind === "mcp")).toBe(true);
    expect(filterCapabilities(rows, "", "skill", "project", "all").map((row) => row.name)).toEqual(["frontend-checks", "release-check"]);
  });

  it("deduplicates identical shared skill files across products, retaining distinct projected metadata", () => {
    const agents = report().agents;
    const shared = structuredClone(agents[1].capabilities!.skills[0]);
    agents[0].capabilities!.skills.push({ ...shared, id: "claude:user:code-review" });
    let rows = capabilityItems(agents).filter((row) => row.name === "code-review");
    expect(rows).toHaveLength(1);
    expect(rows[0].owners.map((owner) => owner.name)).toEqual(["Claude Code", "Cursor"]);
    expect(capabilityLabel(rows[0])).toContain("Agents: Claude Code (PID 51200), Cursor (PID 52000).");
    expect(capabilityLabel(rows[0])).toContain("Source: /Users/dev/.agents/skills/code-review/SKILL.md");
    agents[0].capabilities!.skills.at(-1)!.description = "A different observed projection";
    rows = capabilityItems(agents).filter((row) => row.name === "code-review");
    expect(rows).toHaveLength(2);
  });

  it("prefers declared skill names but keeps folder identity and purpose searchable", () => {
    const agent = report().agents[1];
    agent.capabilities!.skills[1].declared_name = "UI verification";
    const row = capabilityItems([agent]).find((row) => row.name === "UI verification")!;
    expect(capabilityMatches(row, "frontend-checks")).toBe(true);
    expect(capabilityMatches(row, "accessibility")).toBe(true);
    expect(agentMatches(agent, "UI verification")).toBe(true);
    expect(agentMatches(agent, "accessibility")).toBe(true);
  });

  it("searches safe inventory metadata and exact process IDs, never command or secret values", () => {
    const agents = report().agents;
    const row = capabilityItems(agents).find((row) => row.name === "frontend-checks")!;
    expect(capabilityMatches(row, "cursor shop-web skill")).toBe(true);
    expect(capabilityMatches(row, "pid:52000")).toBe(true);
    expect(capabilityMatches(row, "5200")).toBe(false);
    agents[1].command = "secret-token-only";
    agents[1].tools[0].command = "private-tool-command-only";
    expect(capabilityMatches(row, "secret-token-only")).toBe(false);
    expect(capabilityMatches(runningMcpItems(agents)[0], "private-tool-command-only")).toBe(false);
    expect(agentMatches(agents[1], "frontend-checks")).toBe(true);
    expect(agentMatches(agents[1], "secret-token-only")).toBe(false);
  });

  it("keeps unknown, incomplete and truncated inventories distinct from absence", () => {
    const agents = report().agents;
    delete agents[0].capabilities;
    agents[1].capabilities!.more_mcp_servers = 4;
    agents[1].capabilities!.more_skills = 7;
    agents[2].capabilities!.inspected = false;
    const limits = capabilityLimits(agents).join(" ");
    expect(limits).toContain("not reported by this backend");
    expect(limits).toContain("4 more MCP definitions");
    expect(limits).toContain("7 more skill files");
    expect(limits).toContain("have not been inspected");
    expect(limits).toContain("not supported for Docker Desktop");
  });
});
