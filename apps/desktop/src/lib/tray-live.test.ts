import { describe, expect, it } from "vitest";
import { panelFrom } from "./tray-live";
import { demoPanel } from "./tray-panel";
import { MOCK_SNAPSHOT } from "./mock";
import { mockAgents } from "./mock-agents";

describe("tray panel from a live scan", () => {
  it("lists AI agents by RAM, tools apart, and needs from the same rules as Home", () => {
    const report = mockAgents(MOCK_SNAPSHOT);
    const p = panelFrom(MOCK_SNAPSHOT, report, demoPanel());
    const ai = report.agents.filter((a) => a.kind !== "tool");
    expect(p.agents).toBe(ai.length);
    expect(p.topAgents.length).toBe(Math.min(5, ai.length));
    expect(p.moreAgents).toBe(Math.max(0, ai.length - 5));
    for (let i = 1; i < p.topAgents.length; i++) expect(p.topAgents[i - 1].memory).toBeGreaterThanOrEqual(p.topAgents[i].memory);
    expect(p.tools.every((t) => report.agents.find((a) => `agent:${a.id}` === t.action)?.kind === "tool")).toBe(true);
    expect(p.needs?.length).toBeGreaterThan(0);
    expect(p.ports).toBe(MOCK_SNAPSHOT.entries.length);
    expect(p.cpuHistory.at(-1)).toBe(Number((p.cpu ?? 0).toFixed(1)));
  });
  it("shows CPU the OS refused as unknown, not 0%", () => {
    const report = mockAgents(MOCK_SNAPSHOT);
    const a = report.agents.find((x) => x.kind !== "tool")!;
    a.cpu_unknown = "refused"; a.memory_bytes = 1e12;
    expect(panelFrom(MOCK_SNAPSHOT, report, demoPanel()).topAgents[0].cpu).toBeNull();
  });
});
