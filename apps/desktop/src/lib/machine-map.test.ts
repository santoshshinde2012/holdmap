import { describe, expect, it } from "vitest";
import { footprint, mapBounds, radialLayout } from "./machine-map";
import { mockAgents } from "./mock";

const report = mockAgents(undefined, true);

describe("machine map", () => {
  it("builds a footprint from observed folders, ports, enabled MCP servers and remote links", () => {
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const f = footprint(cursor, 50);
    expect(f.filter((n) => n.kind === "port").map((n) => n.label)).toEqual(cursor.ports.map((p) => `:${p.port}`));
    expect(f.some((n) => n.label === "legacy-browser")).toBe(false);
    expect(f.find((n) => n.label === "filesystem")).toMatchObject({ live: true, warn: true });
  });

  it("fans each agent's footprint inside its own sector", () => {
    const layout = radialLayout(report.agents);
    for (const g of layout) for (const n of g.nodes) expect(Math.abs(n.angle - g.angle)).toBeLessThanOrEqual(Math.PI / report.agents.length + 1e-9);
  });

  it("fits the view box around every node and its label", () => {
    const layout = radialLayout(report.agents);
    const b = mapBounds(layout, 1.4);
    for (const g of layout) for (const n of g.nodes) {
      expect(n.x).toBeGreaterThan(b.x); expect(n.x).toBeLessThan(b.x + b.w);
      expect(n.y).toBeGreaterThan(b.y); expect(n.y).toBeLessThan(b.y + b.h);
    }
    expect(mapBounds(layout, 1.4).w).toBeGreaterThan(mapBounds(layout, 1).w);
  });
});
