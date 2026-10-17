import { describe, expect, it } from "vitest";
import { cardModel, miniLayout } from "./agent-card";
import { footprint } from "./machine-map";
import { MOCK_SNAPSHOT, mockAgents } from "./mock";
import { nextLayout } from "./home-layout";

const report = mockAgents(undefined, true);
const by = (p: string) => report.agents.find((a) => a.product === p)!;

describe("agent card model", () => {
  it("counts projects, ports, MCP and skills from the scan and config", () => {
    const m = cardModel(by("cursor"), MOCK_SNAPSHOT.entries);
    expect(m.ports.map((p) => p.port)).toEqual(by("cursor").ports.map((p) => p.port));
    expect(m.mcp?.servers.find((s) => s.name === "filesystem")).toMatchObject({ running: true, warn: true });
    expect(m.skills).not.toBeNull();
    expect(m.unknownReason).toBeNull();
  });

  it("keeps unreadable configuration unknown instead of zero, with the reason", () => {
    const m = cardModel(by("docker-desktop"), MOCK_SNAPSHOT.entries);
    expect(m.mcp).toBeNull();
    expect(m.skills).toBeNull();
    expect(m.unknownReason).toMatch(/not supported/i);
    expect(m.ports.some((p) => p.warn)).toBe(true);
  });

  it("carries env and header names only, never values", () => {
    const off = mockAgents(undefined, false);
    expect(cardModel(off.agents[0], MOCK_SNAPSHOT.entries)).toMatchObject({ mcp: null, configOff: true });
    for (const a of report.agents) for (const s of cardModel(a, null).mcp?.servers ?? []) for (const n of s.names) expect(n).toMatch(/^[A-Za-z0-9_-]+$/);
  });

  it("places each kind in its own quadrant", () => {
    const pts = miniLayout(footprint(by("cursor"), 20), 40);
    for (const p of pts) {
      if (p.node.kind === "project") expect(p.y).toBeLessThan(0);
      if (p.node.kind === "port") expect(p.x).toBeGreaterThan(0);
      if (p.node.kind === "mcp") expect(p.y).toBeGreaterThan(0);
      if (p.node.kind === "remote") expect(p.x).toBeLessThan(0);
    }
  });

  it("flips between the two Home layouts", () => {
    expect(nextLayout("cards")).toBe("map");
    expect(nextLayout("map")).toBe("cards");
  });
});
