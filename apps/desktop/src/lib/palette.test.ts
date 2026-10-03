import { describe, expect, it } from "vitest";
import { fuzzyScore, rank, type Command } from "./palette";

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
