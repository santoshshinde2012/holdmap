import { describe, expect, it } from "vitest";
import { scannersFor, subjectCounts, type AnalyzerView, type Insights } from "./analyzers";

const finding = (subject: string, severity: "high" | "low", state: "open" | "reviewed" = "open") =>
  ({ fingerprint: subject + severity + state, analyzer: "a", rule_id: "r", category: null, severity, title: "t", subject, location: { file: null, line: null }, fix: null, refs: [], state });

describe("subject findings", () => {
  it("counts open findings for one subject across analyzers", () => {
    const insights = { open: { critical: 0, high: 2, medium: 0, low: 1, info: 0 }, results: [
      { analyzer: "a", analyzer_version: null, run_at_ms: 1, status: "complete", coverage: 1, subjects: [], details: [], notes: [], findings: [finding("s1", "high"), finding("s1", "low", "reviewed"), finding("s2", "low")] },
      { analyzer: "b", analyzer_version: null, run_at_ms: 1, status: "complete", coverage: 1, subjects: [], details: [], notes: [], findings: [finding("s1", "high")] },
    ] } as Insights;
    expect(subjectCounts(insights, "s1")).toMatchObject({ high: 2, low: 0 });
    expect(subjectCounts(null, "s1").high).toBe(0);
  });
  it("offers the OSV lookup for MCP servers and skill analyzers for skills", () => {
    const views = [{ id: "osv", kind: "builtin" }, { id: "spector", kind: "cli" }, { id: "remote", kind: "mcp" }] as AnalyzerView[];
    expect(scannersFor(views, "mcp").map((a) => a.id)).toEqual(["osv"]);
    expect(scannersFor(views, "skill").map((a) => a.id)).toEqual(["spector", "remote"]);
  });
});
