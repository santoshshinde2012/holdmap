import { describe, expect, it } from "vitest";
import { attentionCells, attentionSummary, chartPaths, chartTop, cpuParts, cpuSummary, cpuText, demoPanel, HISTORY_CELLS } from "./tray-panel";

describe("tray panel", () => {
  it("formats CPU the way the tray menu does", () => {
    expect(cpuText(7.44)).toBe("7.4%");
    expect(cpuText(12.4)).toBe("12%");
    expect(cpuText(-0)).toBe("0.0%");
    expect(cpuParts(7.4)).toEqual({ value: "7.4", unit: "%" });
    // Not measured yet: an ellipsis, never 0.
    expect(cpuParts(null)).toEqual({ value: "…", unit: "" });
  });

  it("scales the history chart to a readable top, at least the floor", () => {
    expect(chartTop([])).toBe(10);
    expect(chartTop([0.4, 2])).toBe(10);
    expect(chartTop([12.4])).toBe(15);
    expect(chartTop([73])).toBe(80);
    expect(chartTop([180])).toBe(200);
  });

  it("draws the newest scan at the right edge and fills in from the right", () => {
    expect(chartPaths([5], 100, 50)).toEqual({ area: "", line: "" });
    const { line, area } = chartPaths([0, 10], 100, 50, 11);
    // Two samples in 11 slots: the last at x=100, the one before 10 units to its left.
    expect(line).toBe("M90,50L100,0");
    expect(area).toBe("M90,50L100,0L100,50L90,50Z");
  });

  it("lays the attention history oldest first with empty cells before it", () => {
    const cells = attentionCells([0, 1, 2]);
    expect(cells).toHaveLength(HISTORY_CELLS);
    expect(cells.slice(0, HISTORY_CELLS - 3).every((c) => c === "none")).toBe(true);
    expect(cells.slice(-3)).toEqual(["ok", "medium", "high"]);
    expect(attentionCells(Array(80).fill(1))).toEqual(Array(HISTORY_CELLS).fill("medium"));
  });

  it("says what the charts show in words", () => {
    expect(attentionSummary([])).toBe("No scans yet.");
    expect(attentionSummary([0, 0])).toBe("Nothing needed you in the last 2 scans.");
    expect(attentionSummary([0, 2, 1])).toBe("Something needed you in 2 of the last 3 scans.");
    expect(cpuSummary([3])).toBe("Agent CPU appears after a few scans.");
    expect(cpuSummary([3, 12.4, 5])).toBe("Agent CPU over the last 3 scans: now 5.0%, peak 12%.");
  });

  it("ships sample data whose rows use tray action ids", () => {
    const demo = demoPanel();
    const actions = [...(demo.needs ?? []).map((n) => n.action), ...demo.dev.map((d) => d.action), ...demo.topAgents.map((a) => a.action)];
    for (const action of actions) expect(action).toMatch(/^(need:\d+:(agent|entry):|focus:\d+$|agent:)/);
    expect(demo.cpuHistory.length).toBeLessThanOrEqual(HISTORY_CELLS);
  });
});
