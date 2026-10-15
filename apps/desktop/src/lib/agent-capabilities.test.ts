import { describe, expect, it } from "vitest";
import { mockAgents } from "./mock";
import { agentMatches } from "./agents";
import { capabilityItems, capabilityLimits, capabilityMatches, enabledLabel, filterCapabilities, runningMcpItems } from "./agent-capabilities";

const report = () => mockAgents(undefined, true);
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
    expect(filterCapabilities(rows, "", "all", "all", "unknown")).toEqual([unknown]);
    expect(filterCapabilities(rows, "", "skill", "project", "all").map((row) => row.name)).toEqual(["frontend-checks", "release-check"]);
  });

  it("deduplicates identical shared skill files across products, retaining distinct projected metadata", () => {
    const agents = report().agents;
    const shared = structuredClone(agents[1].capabilities!.skills[0]);
    agents[0].capabilities!.skills.push({ ...shared, id: "claude:user:code-review" });
    let rows = capabilityItems(agents).filter((row) => row.name === "code-review");
    expect(rows).toHaveLength(1);
    expect(rows[0].owners.map((owner) => owner.name)).toEqual(["Claude Code", "Cursor"]);
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
