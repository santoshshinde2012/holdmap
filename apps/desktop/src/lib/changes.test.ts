import { describe, expect, it } from "vitest";
import { ago, diffAgents, diffPorts, mergeChanges, stopEvents } from "./changes";
import type { Agent, HistoryEntry, PortEntry } from "./types";

const entry = (id: string, port: number): PortEntry => ({ id, port, label: "node", pid: 10, project: null, framework: null } as PortEntry);
const agent = (id: string, name: string): Agent => ({ id, name, pid: 5 } as Agent);

describe("recent changes", () => {
  it("treats the first scan as a baseline", () => {
    expect(diffPorts(null, [entry("a", 3000)], 1)).toEqual([]);
    expect(diffAgents(null, [agent("x", "Codex")], 1)).toEqual([]);
  });

  it("reports opened and closed listeners and started or exited agents", () => {
    const ports = diffPorts([entry("a", 3000)], [entry("b", 5173)], 9);
    expect(ports.map((e) => [e.kind, e.title])).toEqual([["port_opened", ":5173 opened"], ["port_closed", ":3000 closed"]]);
    expect(ports[0].target).toEqual({ view: "list", entryId: "b" });
    const agents = diffAgents([agent("x", "Codex")], [agent("y", "Claude Code")], 9);
    expect(agents.map((e) => e.title)).toEqual(["Claude Code started", "Codex exited"]);
  });

  it("merges newest first without duplicates and within the bound", () => {
    const stops = stopEvents([{ at_ms: 5, port: 3000, pid: 1, label: "vite", project: "web" } as HistoryEntry]);
    const merged = mergeChanges(stops, [...diffPorts([], [entry("b", 1)], 8), ...stops], 2);
    expect(merged.map((e) => e.kind)).toEqual(["port_opened", "stopped"]);
    expect(merged[1].detail).toBe("web");
  });
});

describe("ago", () => {
  it("is compact and never negative", () => {
    expect(ago(1_000, 1_000)).toBe("just now");
    expect(ago(0, 5 * 60_000)).toBe("5m ago");
    expect(ago(0, 3 * 3_600_000 + 1)).toBe("3h ago");
    expect(ago(0, 2 * 86_400_000)).toBe("2d ago");
    expect(ago(10_000, 0)).toBe("just now");
  });
});
