import { describe, expect, it } from "vitest";
import { DemoAnalyzers } from "./analyzers-demo";
import { attention, overview } from "./home";
import { mockAgents, MOCK_SNAPSHOT } from "./mock";
import { findingItems, securitySummary } from "./security-view";
import { homeReport, homeSummaryText, redactHome, reportFilename, securityReport, securitySarif, securitySummaryText } from "./report-export";

const demo = new DemoAnalyzers();
const insights = demo.insights();
const agents = [{ product: "claude-code", name: "Claude Code" }, { product: "cursor", name: "Cursor" }];
const items = findingItems(insights, demo.list(), new Map(agents.map((a) => [a.product, a.name])));
const summary = securitySummary(insights, demo.list(), agents, 4);
const NOW = Date.UTC(2026, 9, 23, 6, 0, 0);

describe("redactHome", () => {
  it("replaces user folders on every platform and leaves other paths alone", () => {
    expect(redactHome("/Users/ana/proj/SKILL.md")).toBe("~/proj/SKILL.md");
    expect(redactHome("/home/ana")).toBe("~");
    expect(redactHome("C:\\Users\\ana\\proj\\a.md")).toBe("~/proj/a.md");
    expect(redactHome("/opt/tool/a.md")).toBe("/opt/tool/a.md");
    expect(redactHome("skills/a.md")).toBe("skills/a.md");
  });
});

describe("security exports", () => {
  it("JSON carries counts, coverage and each open finding without evidence", () => {
    const r = securityReport(items, summary, NOW);
    expect(r).toMatchObject({ schema: "holdmap-report/1", kind: "security", generated_at: "2026-10-23T06:00:00.000Z", open: summary.open });
    expect(r.findings).toHaveLength(items.length);
    expect(r.counts.high).toBe(1);
    const text = JSON.stringify(r);
    for (const i of items) for (const e of i.evidence.filter((e) => e.label !== "Rule" && e.label !== "Category")) expect(text).not.toContain(e.value);
  });
  it("SARIF 2.1.0 has one result per finding, mapped levels, rules and fingerprints", () => {
    const s = securitySarif(items);
    expect(s.version).toBe("2.1.0");
    expect(s.runs).toHaveLength(1);
    expect(s.runs[0].results).toHaveLength(items.length);
    expect(s.runs[0].tool.driver.name).toBe("Holdmap");
    const ids = new Set(s.runs[0].tool.driver.rules.map((r) => r.id));
    for (const res of s.runs[0].results) {
      expect(ids.has(res.ruleId)).toBe(true);
      expect(["error", "warning", "note"]).toContain(res.level);
      expect(res.partialFingerprints.holdmap).toBeTruthy();
    }
    expect(s.runs[0].results.find((r) => r.level === "error")).toBeTruthy();
    expect(JSON.parse(JSON.stringify(s))).toEqual(s);
  });
  it("never leaks a home folder name", () => {
    const withHome = items.map((i) => ({ ...i, finding: { ...i.finding, location: { file: "/Users/zelda-q/secret-project/SKILL.md", line: 3 } } }));
    expect(JSON.stringify(securityReport(withHome, summary, NOW))).not.toContain("zelda-q");
    expect(JSON.stringify(securitySarif(withHome))).not.toContain("zelda-q");
  });
  it("summary text lists severity counts and each finding", () => {
    const t = securitySummaryText(items, summary, NOW);
    expect(t).toContain("Holdmap security summary · 2026-10-23");
    expect(t).toContain("4 open findings (1 high, 2 medium, 1 low)");
    for (const i of items) expect(t).toContain(i.finding.title);
  });
  it("says so when there is nothing to report", () => {
    const empty = securitySummary(null, [], [], 0);
    expect(securitySummaryText([], empty, NOW)).toContain("No findings.");
    expect(securitySarif([]).runs[0].results).toEqual([]);
  });
});

describe("home exports", () => {
  const report = mockAgents(MOCK_SNAPSHOT);
  const entries = MOCK_SNAPSHOT.entries;
  const o = overview(entries, report);
  const needs = attention(entries, report, null);
  it("summarises the overview and what needs attention", () => {
    const t = homeSummaryText(o, needs, NOW);
    expect(t).toContain("Holdmap summary · 2026-10-23");
    expect(t).toContain(`${o.agents} agents, ${o.ports} listening`);
    expect(t).toContain(`Needs you (${needs.length}):`);
  });
  it("JSON has no command lines, folders or environment", () => {
    const j = homeReport(o, needs, report, NOW);
    expect(j.agents).toHaveLength(report.agents.length);
    const text = JSON.stringify(j);
    for (const a of report.agents) {
      expect(text).not.toContain(a.command);
      for (const f of a.folders) expect(text).not.toContain(f.path);
    }
    expect(Object.keys(j.agents[0]).sort()).toEqual(["kind", "memory_bytes", "name", "pid", "ports", "processes", "vendor"]);
  });
  it("names files holdmap-<what>-<date>.<ext>, which the native save accepts", () => {
    expect(reportFilename("security", NOW, "sarif")).toBe("holdmap-security-2026-10-23.sarif");
    expect(reportFilename("overview", NOW, "json")).toMatch(/^holdmap-[a-z-]+-\d{4}-\d{2}-\d{2}\.json$/);
  });
});
