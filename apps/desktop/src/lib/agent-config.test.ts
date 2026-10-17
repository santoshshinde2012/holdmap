import { describe, expect, it } from "vitest";
import { configSummary, isUnpinned, launchLine, riskySettings, runState } from "./agent-config";
import type { Agent, AgentMcpServer } from "./types";

const server = (over: Partial<AgentMcpServer>): AgentMcpServer => ({
  id: "s", name: "s", scope: "user", source_path: "/h/.claude.json", project_path: null, transport: "stdio", enabled: null, evidence: "observed", ...over,
});
const agent = (capabilities: Agent["capabilities"]): Agent => ({ capabilities } as Agent);

describe("agent configuration view model", () => {
  it("summarises only an inspected inventory", () => {
    expect(configSummary(agent(undefined))).toBeNull();
    expect(configSummary(agent({ inspected: false, mcp_servers: [], skills: [], limits: [], more_mcp_servers: 0, more_skills: 0 }))).toBeNull();
    const s = configSummary(agent({
      inspected: true, limits: [], more_mcp_servers: 1, more_skills: 0,
      mcp_servers: [
        server({ launch: { runner: "npx", package: "a", pinned: false }, runtime: { state: "running", pids: [4] } }),
        server({ launch: { runner: "npx", package: "b@1.0.0", pinned: true } }),
      ],
      skills: [{ id: "k", name: "k", scope: "user", source_path: "/k", project_path: null, evidence: "observed" }],
      tool_rules: [{ rule: "Bash(ls)", decision: "allow", scope: "user", source_path: "/s" }],
    }));
    expect(s).toEqual({ mcp: 3, running: 1, skills: 1, instructions: 0, rules: 1, unpinned: 1 });
  });

  it("flags only enabled, fetching, unpinned launches", () => {
    expect(isUnpinned(server({ launch: { runner: "uvx", package: "x", pinned: false } }))).toBe(true);
    expect(isUnpinned(server({ launch: { runner: "uvx", package: "x", pinned: false }, enabled: false }))).toBe(false);
    expect(isUnpinned(server({ launch: { runner: "binary", package: "x", pinned: null } }))).toBe(false);
  });

  it("keeps configured and observed apart", () => {
    expect(runState(server({ runtime: { state: "running", pids: [7, 9] } }))).toMatchObject({ tone: "ok", detail: "Observed: PID 7, 9 under this agent." });
    expect(runState(server({ runtime: { state: "not_seen", pids: [] } })).label).toBe("Not running");
    expect(runState(server({ transport: "http" })).detail).toMatch(/Remote server/);
    expect(runState(server({ enabled: false, runtime: { state: "running", pids: [1] } })).tone).toBe("off");
  });

  it("describes the launch without arguments", () => {
    expect(launchLine(server({ launch: { runner: "docker", package: "mcp/postgres:1.2", pinned: true } }))).toBe("docker mcp/postgres:1.2");
    expect(launchLine(server({ transport: "http", remote_host: "mcp.linear.app" }))).toBe("mcp.linear.app");
    expect(launchLine(server({ transport: "unknown" }))).toBe("transport unknown");
  });

  it("names settings that skip approvals or the sandbox", () => {
    const a = agent({ inspected: true, mcp_servers: [], skills: [], limits: [], more_mcp_servers: 0, more_skills: 0, settings: [
      { key: "sandbox_mode", value: "danger-full-access", scope: "user", source_path: "/c" },
      { key: "approval_policy", value: "on-request", scope: "user", source_path: "/c" },
    ] });
    expect(riskySettings(a).map((r) => r.why)).toEqual(["runs without a sandbox"]);
  });
});
