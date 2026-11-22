import { describe, expect, it } from "vitest";
import { footprint, remoteGroupId, shorten } from "./footprint";
import { mockAgents } from "./mock-agents";

const report = mockAgents(undefined, true);

describe("agent footprint", () => {
  it("builds a footprint from observed folders, ports, enabled MCP servers and remote links", () => {
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const f = footprint(cursor, 50);
    expect(f.filter((n) => n.kind === "port").map((n) => n.label)).toEqual(cursor.ports.map((p) => `:${p.port}`));
    expect(f.some((n) => n.label === "legacy-browser")).toBe(false);
    expect(f.find((n) => n.label === "filesystem")).toMatchObject({ live: true, warn: true });
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

  it("shortens long labels in the middle and keeps both ends", () => {
    expect(shorten("short")).toBe("short");
    const long = "[2001:db8:4700:4408::ac40:9bd1]:443";
    const s = shorten(long);
    expect(s.length).toBe(24);
    expect(s).toContain("…");
    expect(s.startsWith("[2001:db8")).toBe(true);
    expect(s.endsWith(":443")).toBe(true);
  });
});
