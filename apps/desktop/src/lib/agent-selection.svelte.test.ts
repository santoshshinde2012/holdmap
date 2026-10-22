import { describe, expect, it } from "vitest";
import { createAgentSelection } from "./agent-selection.svelte";
import { agentHash, agentIdentity, findLinkedAgent, parseAgentHash, remember, resolveSelection } from "./agent-selection";
import type { Agent } from "./types";

const agent = (pid: number, name = "Claude", command = "/Applications/Claude.app"): Agent => ({
  id: `agent:${pid}`, product: name.toLowerCase(), name, vendor: "Anthropic", kind: "desktop", pid,
  process_name: name, command, started_at: 0, parent: null, memory_bytes: 0, cpu_percent: 0, processes: [],
  process_ids: [pid], more_processes: 0, folders: [], more_folders: 0, tools: [], more_tools: 0, ports: [],
  links: [], more_links: 0, access: {} as Agent["access"],
});

describe("agent selection rules", () => {
  it("keeps the exact id when it is still there", () => {
    const a = agent(1);
    expect(resolveSelection(remember(a), [agent(2, "Cursor"), a])).toBe(a);
  });
  it("follows a restarted agent to its new pid", () => {
    const next = agent(99);
    expect(resolveSelection(remember(agent(1)), [agent(2, "Cursor"), next])?.id).toBe("agent:99");
  });
  it("uses the command to tell same-named agents apart, and gives up when still ambiguous", () => {
    const memory = remember(agent(1, "Codex CLI", "codex --a"));
    const pick = agent(8, "Codex CLI", "codex --a");
    expect(resolveSelection(memory, [agent(7, "Codex CLI", "codex --b"), pick])).toBe(pick);
    expect(resolveSelection(memory, [agent(7, "Codex CLI", "x"), agent(8, "Codex CLI", "y")])).toBeNull();
  });
  it("returns null when the agent is gone or nothing was selected", () => {
    expect(resolveSelection(remember(agent(1)), [agent(2, "Cursor")])).toBeNull();
    expect(resolveSelection(null, [agent(1)])).toBeNull();
  });
  it("identity has no pid", () => {
    expect(agentIdentity(agent(1))).toBe(agentIdentity(agent(2)));
  });
  it("parses and builds deep links", () => {
    expect(parseAgentHash(agentHash("agent:12"))).toBe("agent:12");
    expect(parseAgentHash("#ui-gallery")).toBeNull();
    expect(parseAgentHash("#agent=%E0%A4")).toBeNull();
    expect(agentHash(null)).toBe("");
    const list = [agent(1), agent(2, "Cursor")];
    expect(findLinkedAgent("agent:2", list)?.name).toBe("Cursor");
    expect(findLinkedAgent("claude", list)?.id).toBe("agent:1");
    expect(findLinkedAgent("nope", list)).toBeNull();
  });
});

describe("AgentSelection store", () => {
  it("selects, clears and steps with wrap-around", () => {
    const s = createAgentSelection();
    const ids = ["agent:1", "agent:2", "agent:3"];
    s.step(ids, 1);
    expect(s.id).toBe("agent:1");
    s.step(ids, -1);
    expect(s.id).toBe("agent:3");
    s.clear();
    expect(s.id).toBeNull();
    s.step(ids, -1);
    expect(s.id).toBe("agent:3");
  });
  it("survives a refresh where the agent's pid changed", () => {
    const s = createAgentSelection();
    const list = [agent(1), agent(2, "Cursor")];
    s.select("agent:1", list);
    s.reconcile([agent(2, "Cursor"), agent(41)]);
    expect(s.id).toBe("agent:41");
    s.reconcile([agent(2, "Cursor"), agent(41)]);
    expect(s.id).toBe("agent:41");
  });
  it("drops the selection when the agent quits, and ignores refreshes with nothing selected", () => {
    const s = createAgentSelection();
    s.reconcile([agent(1)]);
    expect(s.id).toBeNull();
    s.select("agent:1", [agent(1)]);
    s.reconcile([agent(2, "Cursor")]);
    expect(s.id).toBeNull();
  });
  it("an id selected without its agent still resolves by exact id", () => {
    const s = createAgentSelection();
    s.select("agent:5");
    s.reconcile([agent(5)]);
    expect(s.id).toBe("agent:5");
  });
});
