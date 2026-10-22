import { describe, expect, it } from "vitest";
import { avoidCollisions, footprint, hasCollisions, mapBounds, radialLayout, remoteGroupId, shorten } from "./machine-map";
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

  it("collapses two or more remote hosts into one count node that expands", () => {
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const remotes = cursor.links.filter((l) => l.kind === "remote").length;
    expect(remotes).toBeGreaterThan(1);
    const collapsed = footprint(cursor, 50).filter((n) => n.kind === "remote");
    expect(collapsed).toHaveLength(1);
    expect(collapsed[0]).toMatchObject({ count: remotes, label: `${remotes} remote hosts`, target: { view: "expand", groupId: remoteGroupId(cursor.id) } });
    const open = footprint(cursor, 50, new Set([remoteGroupId(cursor.id)])).filter((n) => n.kind === "remote");
    expect(open.filter((n) => !n.collapse)).toHaveLength(remotes);
    expect(open[0]).toMatchObject({ collapse: true, target: { view: "expand", groupId: remoteGroupId(cursor.id) } });
  });

  it("shortens long labels in the middle and keeps the full text for the tooltip", () => {
    expect(shorten("short")).toBe("short");
    const long = "[2001:db8:4700:4408::ac40:9bd1]:443";
    const s = shorten(long);
    expect(s.length).toBe(24);
    expect(s).toContain("…");
    expect(s.startsWith("[2001:db8")).toBe(true);
    expect(s.endsWith(":443")).toBe(true);
  });

  it("moves labels apart until none overlap, even with every remote host shown", () => {
    const all = new Set(report.agents.map((a) => remoteGroupId(a.id)));
    for (const expanded of [new Set<string>(), all]) {
      for (const k of [1, 1.4, 1.8]) {
        const layout = radialLayout(report.agents, undefined, undefined, undefined, undefined, expanded);
        const fixed = avoidCollisions(layout, k);
        expect(hasCollisions(fixed, k)).toBe(false);
        // Sides stay put: a label keeps reading away from its agent.
        fixed.forEach((g, i) => g.nodes.forEach((n, j) => expect(n.angle).toBe(layout[i].nodes[j].angle)));
      }
    }
  });
});
