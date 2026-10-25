import { describe, expect, it, vi } from "vitest";
import { DemoAnalyzers } from "./analyzers-demo";
import { presetViews } from "./analyzer-presets";
import { analyzerCards, applyFilter, emptyFilter, findingItems, firstSentence, groupFindings, runProgress, securitySummary, severityCounts } from "./security-view";
import { SecurityRuns } from "./security-runs.svelte";

const demo = new DemoAnalyzers();
const insights = demo.insights();
const analyzers = demo.list();
const agents = [{ product: "claude-code", name: "Claude Code" }, { product: "cursor", name: "Cursor" }, { product: "docker-desktop", name: "Docker Desktop" }];

describe("securitySummary", () => {
  it("counts open findings, last run and coverage", () => {
    const s = securitySummary(insights, analyzers, agents, 10);
    expect(s.open).toBe(4);
    expect(s.counts.high).toBe(1);
    expect(s.lastRun).toBe(Math.max(...insights.results.map((r) => r.run_at_ms)));
    expect(s.analyzersOn).toBe(1);
    expect(s.agents).toEqual({ scanned: 2, total: 3 });
    expect(s.skills.total).toBe(10);
    expect(s.skills.scanned).toBeGreaterThan(0);
  });
  it("is empty without insights", () => {
    const s = securitySummary(null, [], agents, 0);
    expect(s).toMatchObject({ open: 0, lastRun: null, analyzersOn: 0, agents: { scanned: 0, total: 3 }, skills: { scanned: 0, total: 0 } });
  });
});

describe("findings", () => {
  const items = findingItems(insights, analyzers, new Map([["claude-code", "Claude Code"]]));
  it("joins subject, analyzer name, location and evidence, most severe first", () => {
    const top = items[0];
    expect(top.finding.severity).toBe("high");
    expect(top.analyzerName).toBe("SkillSpector");
    expect(top.where).toBe("SKILL.md:12");
    expect(top.agents).toContain("Claude Code");
    expect(top.evidence.map((e) => e.label)).toEqual(expect.arrayContaining(["Rule", "Risk score"]));
  });
  it("filters by severity, query, agent and state", () => {
    expect(applyFilter(items, { ...emptyFilter(), severities: ["medium"] })).toHaveLength(2);
    expect(applyFilter(items, { ...emptyFilter(), query: "publish.sh" }).map((i) => i.finding.title)).toEqual(["External Transmission"]);
    expect(applyFilter(items, { ...emptyFilter(), agent: "cursor" }).every((i) => i.subject?.agents.includes("cursor"))).toBe(true);
    expect(applyFilter(items, { ...emptyFilter(), query: "nothing-matches" })).toEqual([]);
  });
  it("groups by severity in order, and by skill or analyzer by worst first", () => {
    const open = applyFilter(items, emptyFilter());
    expect(groupFindings(open, "severity").map((g) => g.key)).toEqual(["high", "medium", "low"]);
    const skills = groupFindings(open, "skill");
    expect(skills[0].severity).toBe("high");
    expect(skills.reduce((n, g) => n + g.items.length, 0)).toBe(open.length);
    expect(groupFindings(open, "analyzer").map((g) => g.label)).toEqual(["SkillSpector"]);
    expect(severityCounts(open)).toMatchObject({ high: 1, medium: 2, low: 1 });
  });
});

describe("analyzerCards", () => {
  it("gives each analyzer one primary action; only Run acts directly", () => {
    const cards = analyzerCards(analyzers, presetViews(() => false));
    const by = Object.fromEntries(cards.map((c) => [c.id, c]));
    expect(by["nvidia.skillspector"]).toMatchObject({ status: "on", action: "run", actionLabel: "Run", initial: "S" });
    expect(by["holdmap.osv"]).toMatchObject({ status: "off", action: "enable", target: "MCP packages" });
    const presets = cards.filter((c) => !analyzers.some((a) => a.id === c.id));
    expect(presets.length).toBe(4);
    expect(presets.every((c) => c.action === "install" && c.status === "not_installed")).toBe(true);
  });
  it("marks a changed analyzer for review and an installed preset as addable", () => {
    const changed = analyzers.map((a) => (a.id === "nvidia.skillspector" ? { ...a, state: "changed" as const } : a));
    const p = presetViews(() => false).map((x, i) => (i === 0 ? { ...x, installed: true } : x));
    const cards = analyzerCards(changed, p);
    expect(cards.find((c) => c.id === "nvidia.skillspector")).toMatchObject({ status: "changed", action: "review" });
    expect(cards.find((c) => c.id === p[0].id)).toMatchObject({ status: "available", action: "add" });
  });
  it("keeps the first sentence", () => {
    expect(firstSentence("Checks things. And more.")).toBe("Checks things");
    expect(firstSentence("No period")).toBe("No period");
  });
});

describe("runs", () => {
  it("summarises progress", () => {
    expect(runProgress([]).phase).toBe("idle");
    expect(runProgress([{ id: "a", name: "A", phase: "running", note: null }, { id: "b", name: "B", phase: "idle", note: null }])).toMatchObject({ done: 0, total: 2, phase: "running" });
    expect(runProgress([{ id: "a", name: "A", phase: "done", note: null }, { id: "b", name: "B", phase: "failed", note: "x" }])).toMatchObject({ done: 2, phase: "failed" });
  });
  it("runs only approved analyzers, one at a time, then refreshes", async () => {
    const order: string[] = [];
    const run = vi.fn(async (id: string) => { order.push(id); if (id === "b") throw new Error("boom"); return { status: "complete" as const }; });
    const refresh = vi.fn(async () => {});
    const r = new SecurityRuns(run, refresh);
    await r.start([{ id: "a", name: "A", state: "approved" }, { id: "off", name: "Off", state: "not_approved" }, { id: "b", name: "B", state: "approved" }]);
    expect(order).toEqual(["a", "b"]);
    expect(r.progress).toMatchObject({ done: 2, total: 2, phase: "failed" });
    expect(r.steps[1].note).toContain("boom");
    expect(refresh).toHaveBeenCalledOnce();
    r.clear();
    expect(r.steps).toEqual([]);
    await r.start([{ id: "off", name: "Off", state: "not_approved" }]);
    expect(run).toHaveBeenCalledTimes(2);
  });
});
