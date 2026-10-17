import { describe, expect, it } from "vitest";
import { briefing, footprint, lanes, radialLayout } from "./home-concepts";
import { MOCK_SNAPSHOT, mockAgents } from "./mock";

const report = mockAgents(undefined, true);
const entries = MOCK_SNAPSHOT.entries;

describe("home concepts", () => {
  it("builds a footprint only from observed folders, ports, configured MCP servers and remote links", () => {
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const f = footprint(cursor, 50);
    expect(f.filter((n) => n.kind === "port").map((n) => n.label)).toEqual(cursor.ports.map((p) => `:${p.port}`));
    expect(f.some((n) => n.label === "legacy-browser")).toBe(false); // disabled servers are left out
    expect(f.find((n) => n.label === "filesystem")).toMatchObject({ live: true, warn: true });
  });

  it("fans each agent's footprint inside its own sector", () => {
    const layout = radialLayout(report.agents);
    expect(layout).toHaveLength(report.agents.length);
    for (const g of layout) for (const n of g.nodes) expect(Math.abs(n.angle - g.angle)).toBeLessThanOrEqual(Math.PI / report.agents.length + 1e-9);
  });

  it("writes a brief from real data with links and the first thing that needs you", () => {
    const { parts, items } = briefing(entries, report);
    const text = parts.map((p) => p.text).join("");
    expect(text).toMatch(/^3 agents are running\./);
    expect(text).toContain("Claude Code holds :3001 in shop-api and runs sandboxed.");
    expect(items.length).toBeGreaterThan(0);
    expect(parts.filter((p) => p.target).length).toBeGreaterThan(3);
    expect(briefing(null, null).parts[0].text).toMatch(/first look/);
  });

  it("puts lanes on the time axis, clipping agents that started before the window", () => {
    const now = report.taken_at_ms || Date.now();
    const from = now - 6 * 3600_000;
    const ls = lanes(report, entries, from, now);
    expect(ls.every((l) => l.start >= from && l.marks.every((m) => m.at >= from && m.at <= now + 1000))).toBe(true);
    expect(ls.some((l) => l.clipped)).toBe(true);
  });
});
