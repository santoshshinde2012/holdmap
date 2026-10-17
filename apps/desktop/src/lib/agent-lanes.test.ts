import { describe, expect, it } from "vitest";
import { lanes, oldest, shortAge, timeScale } from "./agent-lanes";
import { MOCK_SNAPSHOT, mockAgents } from "./mock";

const report = mockAgents(undefined, true);
const now = Date.now();

describe("agent lanes", () => {
  it("maps time onto a monotonic log scale ending at now", () => {
    const s = timeScale(now, now - 2 * 86_400_000);
    expect(s.x(now)).toBe(1);
    expect(s.x(now - 2 * 86_400_000)).toBeGreaterThan(0);
    expect(s.x(now - 60_000)).toBeGreaterThan(s.x(now - 3_600_000));
    expect(s.x(now - 3_600_000)).toBeGreaterThan(s.x(now - 86_400_000));
    expect(s.ticks.map((t) => t.label)).toContain("1d");
    expect(s.ticks.every((t, i, a) => i === 0 || t.x < a[i - 1].x)).toBe(true);
  });

  it("formats short ages", () => {
    expect([shortAge(5 * 60_000), shortAge(3 * 3_600_000), shortAge(2 * 86_400_000)]).toEqual(["5m", "3h", "2d"]);
  });

  it("builds one lane per agent plus unowned dev servers, with bars only where a start time is known", () => {
    const ls = lanes(report, MOCK_SNAPSHOT.entries, now);
    expect(ls.filter((l) => l.agent).length).toBe(report.agents.length);
    const unowned = ls.at(-1)!;
    expect(unowned.agent).toBeNull();
    expect(unowned.bars.length).toBeGreaterThan(0);
    for (const l of ls) for (const b of l.bars) expect(Number.isFinite(b.start)).toBe(true);
    const cursor = ls.find((l) => l.agent?.product === "cursor")!;
    expect(cursor.mcp.map((m) => m.name)).not.toContain("legacy-browser");
    expect(cursor.mcp.find((m) => m.name === "filesystem")).toMatchObject({ running: true, warn: true });
    expect(oldest(ls, now)).toBeLessThan(now);
  });
});
