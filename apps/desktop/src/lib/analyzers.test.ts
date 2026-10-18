import { describe, expect, it, vi } from "vitest";
import { SANDBOX_LABEL, agentCounts, agentFindings, ago, canRun, countsText, emptyCounts, findingWhere, flaggedAgents, insightNeeds, total, worst, type Insights } from "./analyzers";
import { DemoAnalyzers } from "./analyzers-demo";
import { attention, hotAgents } from "./home";
import { mockAgents } from "./mock";
import { InsightsStore, CHANGE_CHECK_MS } from "./insights-store.svelte";

const report = mockAgents(undefined, true);
const demoInsights = (): Insights => new DemoAnalyzers().insights();

describe("analyzer helpers", () => {
  it("counts, orders and describes severities", () => {
    const c = { ...emptyCounts(), medium: 2, critical: 1, low: 4 };
    expect(total(c)).toBe(7);
    expect(worst(c)).toBe("critical");
    expect(worst(emptyCounts())).toBeNull();
    expect(countsText(c)).toBe("1 critical, 2 medium, …");
    expect(countsText(c, 5)).toBe("1 critical, 2 medium, 4 low");
  });

  it("joins findings to agents through their subjects, most severe first", () => {
    const i = demoInsights();
    const claude = agentFindings(i, "claude-code");
    expect(claude.map((r) => r.finding.severity)).toEqual(["high", "medium"]);
    expect(agentFindings(i, "cursor").map((r) => r.finding.rule_id)).toEqual(["LP3"]);
    expect(agentFindings(i, "docker-desktop")).toEqual([]);
    expect(agentFindings(null, "cursor")).toEqual([]);
    expect(agentCounts(i, "claude-code")).toMatchObject({ high: 1, medium: 1, low: 0 });
  });

  it("drops reviewed and snoozed findings unless asked", () => {
    const demo = new DemoAnalyzers();
    demo.review("demo-p1", "reviewed");
    const i = demo.insights();
    expect(agentFindings(i, "claude-code").map((r) => r.finding.rule_id)).toEqual(["E1"]);
    expect(agentFindings(i, "claude-code", true)).toHaveLength(2);
    expect(i.open.high).toBe(0);
    demo.review("demo-p1", "open");
    expect(demo.insights().open.high).toBe(1);
  });

  it("raises one Needs-you item per agent with medium-or-worse findings", () => {
    const needs = insightNeeds(demoInsights(), report);
    expect(needs).toHaveLength(1);
    const claude = report.agents.find((a) => a.product === "claude-code")!;
    expect(needs[0]).toMatchObject({ id: "analyzer:claude-code", agentId: claude.id, severity: "high" });
    expect(needs[0].title).toContain("SkillSpector found 1 high, 1 medium in release-check");
    expect(insightNeeds(demoInsights(), null)).toEqual([]);
  });

  it("flags agents with high findings for the map ring", () => {
    const flagged = flaggedAgents(demoInsights(), report.agents);
    expect([...flagged]).toEqual([report.agents.find((a) => a.product === "claude-code")!.id]);
  });

  it("formats locations, run state and age", () => {
    const f = demoInsights().results[0].findings[0];
    expect(findingWhere(f)).toMatch(/:\d+$/);
    expect(findingWhere({ ...f, location: { file: null, line: null } })).toBeNull();
    expect(findingWhere({ ...f, location: { file: "SKILL.md", line: null } })).toBe("SKILL.md");
    expect(canRun(new DemoAnalyzers().list().find((a) => a.id === "holdmap.osv")!)).toBe(false);
    expect(ago(1000, 1000 + 30_000)).toBe("just now");
    expect(ago(0, 5 * 60_000)).toBe("5 min ago");
    expect(ago(0, 3 * 3_600_000)).toBe("3 h ago");
    expect(ago(0, 86_400_000)).toBe("1 day ago");
  });
});

describe("home attention with analyzer findings", () => {
  it("adds analyzer items and rings only high ones", () => {
    const base = attention(null, report);
    const withInsights = attention(null, report, demoInsights());
    const added = withInsights.filter((i) => i.rule === "analyzer");
    expect(withInsights.length).toBe(base.length + 1);
    const claude = report.agents.find((a) => a.product === "claude-code")!.id;
    expect(added[0].target).toEqual({ view: "agents", agentId: claude });
    expect(hotAgents(withInsights).has(report.agents.find((a) => a.product === "claude-code")!.id)).toBe(true);
  });
});

describe("demo analyzers", () => {
  it("start with SkillSpector on and OSV off; runs need approval", () => {
    const demo = new DemoAnalyzers();
    expect(demo.list().map((a) => [a.id, a.state])).toEqual([["nvidia.skillspector", "approved"], ["holdmap.osv", "not_approved"]]);
    expect(() => demo.run("holdmap.osv", report)).toThrow(/off/);
    demo.approve("holdmap.osv");
    expect(demo.run("holdmap.osv", report).analyzer).toBe("holdmap.osv");
    expect(demo.revoke("nvidia.skillspector")).toBe(true);
    expect(demo.insights().results.map((r) => r.analyzer)).toEqual(["holdmap.osv"]);
  });

  it("previews only pinned packages, with the exact request and no env for the built-in", () => {
    const p = new DemoAnalyzers().preview("holdmap.osv", report);
    expect(p.command).toBeNull();
    expect(p.env).toEqual([]);
    for (const s of p.subjects) expect(s.subject.kind).toBe("package");
    const body = JSON.parse(p.payload[0]) as { queries: { package: { ecosystem: string; name: string }; version: string }[] };
    expect(body.queries.length).toBe(p.subjects.length);
    for (const q of body.queries) expect(Object.keys(q).sort()).toEqual(["package", "version"]);
  });

  it("previews the SkillSpector command with --no-llm and only env names", () => {
    const p = new DemoAnalyzers().preview("nvidia.skillspector", report);
    expect(p.command).toContain("--no-llm");
    expect(p.env).not.toContain("OPENAI_API_KEY");
    expect(p.sandbox).toMatch(/best effort/);
    expect(p.sandbox).toMatch(/can still read your other files/);
    expect(p.schedule).toMatch(/^Only when you click Run now/);
  });

  it("runs network analyzers on change only when allowed, and a new approval resets it", () => {
    const d = new DemoAnalyzers();
    expect(() => d.setBackground("holdmap.osv", true)).toThrow(/Turn it on first/);
    expect(d.setBackground("nvidia.skillspector", true).background).toBe(true);
    expect(d.preview("nvidia.skillspector", report).schedule).toMatch(/you allowed/);
    expect(d.approve("nvidia.skillspector").background).toBe(false);
  });

  it("labels every sandbox level", () => {
    expect(Object.keys(SANDBOX_LABEL).sort()).toEqual(["basic", "linux_bubblewrap", "linux_landlock", "macos_seatbelt", "windows_job"]);
  });
});

describe("insights store", () => {
  const source = (approved: boolean) => ({
    analyzers: vi.fn(async () => (approved ? new DemoAnalyzers().list() : [])),
    insights: vi.fn(async () => demoInsights()),
    runChangedAnalyzers: vi.fn(async () => ["nvidia.skillspector"]),
  });

  it("does not run anything when no analyzer is on", async () => {
    const s = source(false);
    const store = new InsightsStore(s);
    await store.refresh();
    expect(await store.afterAgents(0)).toEqual([]);
    expect(s.runChangedAnalyzers).not.toHaveBeenCalled();
  });

  it("checks for changes at most once per interval and refreshes after a run", async () => {
    const s = source(true);
    const store = new InsightsStore(s);
    await store.refresh();
    expect(await store.afterAgents(0)).toEqual(["nvidia.skillspector"]);
    expect(await store.afterAgents(CHANGE_CHECK_MS - 1)).toEqual([]);
    expect(await store.afterAgents(CHANGE_CHECK_MS)).toEqual(["nvidia.skillspector"]);
    expect(s.runChangedAnalyzers).toHaveBeenCalledTimes(2);
    expect(s.insights).toHaveBeenCalledTimes(3);
  });

  it("keeps the error instead of throwing", async () => {
    const store = new InsightsStore({ ...source(true), analyzers: vi.fn(async () => { throw new Error("nope"); }) });
    await store.refresh();
    expect(store.error).toContain("nope");
  });
});
