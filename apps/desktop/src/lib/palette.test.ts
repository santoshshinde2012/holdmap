import { describe, expect, it } from "vitest";
import { fuzzyScore, groupResults, IDLE_PORT_LIMIT, rank, type Command } from "./palette";

describe("command palette ranking", () => {
  const cmd = (title: string, keywords = ""): Command => ({ id: title, title, keywords, group: "Actions", run: () => {} });
  const cmds = [cmd("Stop :3000 — shop-web"), cmd("Refresh now"), cmd("Toggle dev servers filter", "dev"), cmd("Theme: dark")];
  it("prefers prefix and word-start matches", () => {
    expect(fuzzyScore("ref", "Refresh now")).toBeGreaterThan(fuzzyScore("ref", "Toggle dev servers filter"));
    expect(fuzzyScore("zzz", "Refresh now")).toBe(-1);
  });
  it("ranks the obvious command first", () => {
    expect(rank("stop 3000", cmds)[0].title).toContain("Stop :3000");
    expect(rank("dark", cmds)[0].title).toBe("Theme: dark");
    expect(rank("", cmds)).toHaveLength(4);
  });
});

describe("palette boost", () => {
  it("ranks boosted commands (selected port) first among equal matches", async () => {
    const { rank } = await import("./palette");
    const mk = (id: string, title: string, boost = 0) => ({ id, title, group: "Actions" as const, boost, run: () => {} });
    const out = rank("stop", [mk("a", "Stop :6080 novnc"), mk("b", "Stop :3000 shop-web", 12)]);
    expect(out[0].id).toBe("b");
  });
});

describe("palette sections", () => {
  const cmd = (id: string, group: Command["group"]): Command => ({ id, title: id, group, run: () => {} });
  it("keeps the fixed order without a query and follows the best match with one", () => {
    const list = [cmd("stop a", "Actions"), cmd(":3000", "Ports"), cmd("stop b", "Actions"), cmd("graph", "View")];
    expect(groupResults("", list).map((g) => g.name)).toEqual(["Ports", "Actions", "View"]);
    const q = groupResults("st", list);
    expect(q.map((g) => g.name)).toEqual(["Actions", "Ports", "View"]);
    expect(q[0].items.map((c) => c.id)).toEqual(["stop a", "stop b"]);
  });
  it("previews a few ports when idle so actions stay above the fold, and searches them all when typing", () => {
    const ports = Array.from({ length: IDLE_PORT_LIMIT + 4 }, (_, i) => cmd(`:300${i}`, "Ports"));
    const list = [...ports, cmd("graph", "View")];
    expect(groupResults("", list)[0].items).toHaveLength(IDLE_PORT_LIMIT);
    expect(groupResults("300", list)[0].items).toHaveLength(ports.length);
  });
});
