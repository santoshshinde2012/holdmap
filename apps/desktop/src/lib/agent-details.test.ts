import { describe, expect, it } from "vitest";
import { detailStats, factRows, firstSentence } from "./agent-details";
import { mockAgents } from "./mock";

describe("agent details view model", () => {
  it("keeps the first sentence and caps long text", () => {
    expect(firstSentence("Reads files. Also more.")).toBe("Reads files.");
    expect(firstSentence("x".repeat(100), 20)).toHaveLength(20);
    expect(firstSentence("x".repeat(100), 20).endsWith("…")).toBe(true);
  });

  it("builds short access rows with the full text kept for tooltips", () => {
    for (const a of mockAgents().agents) {
      for (const r of factRows(a)) {
        expect(r.label).toBeTruthy();
        expect(r.full.startsWith(r.text.replace(/…$/, ""))).toBe(true);
        expect(r.text.length).toBeLessThanOrEqual(r.level === "unknown" ? 64 : 90);
      }
    }
  });

  it("summarises folders, ports, links and processes", () => {
    const a = mockAgents().agents[1];
    const stats = detailStats(a);
    expect(stats.map((s) => s.label)).toEqual(["Folders", "Ports", "Links", "Processes"]);
    expect(stats[3].meta).toMatch(/% CPU · /);
  });
});
