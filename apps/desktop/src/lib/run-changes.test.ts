import { describe, expect, it } from "vitest";
import { runChanges, type AnalyzerResult, type Finding, type RunStatus } from "./analyzers";
import { DemoAnalyzers } from "./analyzers-demo";
import { applyFilter, changeSummary, emptyFilter, findingItems } from "./security-view";

const f = (fingerprint: string, severity: Finding["severity"] = "low"): Finding => ({
  fingerprint, analyzer: "a.b", rule_id: "r", category: null, severity, title: `t ${fingerprint}`, subject: "s", location: { file: null, line: null }, fix: null, refs: [], state: "open",
});
const run = (status: RunStatus, findings: Finding[], run_at_ms = 1): AnalyzerResult => ({
  analyzer: "a.b", analyzer_version: null, run_at_ms, status, coverage: null, subjects: [], findings, details: [], notes: [],
});

describe("runChanges (mirrors the Rust rules)", () => {
  it("finds new and fixed findings by fingerprint, most severe fixed first", () => {
    const c = runChanges(run("complete", [f("kept"), f("gone"), f("gone-high", "high")], 7), run("complete", [f("kept"), f("fresh")]))!;
    expect(c).toMatchObject({ new: ["fresh"], new_total: 1, fixed_total: 2, since_ms: 7 });
    expect(c.fixed[0].fingerprint).toBe("gone-high");
  });
  it("has nothing to compare on a first run or after a failed run", () => {
    expect(runChanges(undefined, run("complete", [f("a")]))).toBeNull();
    expect(runChanges(run("failed", []), run("complete", [f("a")]))).toBeNull();
    expect(runChanges(run("complete", [f("a")]), run("failed", []))).toBeNull();
  });
  it("never claims fixes from a partial run", () => {
    const c = runChanges(run("complete", [f("a"), f("b")]), run("partial", [f("a"), f("c")]))!;
    expect(c).toMatchObject({ new_total: 1, fixed_total: 0, fixed: [] });
  });
});

describe("new findings in the Security tab", () => {
  const changed = { results: [{ ...run("complete", [f("x", "high"), f("y")]), changes: { new: ["x"], new_total: 1, fixed: [{ fingerprint: "z", severity: "medium" as const, title: "Old", subject: "s" }], fixed_total: 3, since_ms: 5 } }], open: { critical: 0, high: 1, medium: 0, low: 1, info: 0 } };
  it("marks new findings and filters to them", () => {
    const items = findingItems(changed, []);
    expect(items.find((i) => i.finding.fingerprint === "x")?.isNew).toBe(true);
    expect(items.find((i) => i.finding.fingerprint === "y")?.isNew).toBe(false);
    expect(applyFilter(items, { ...emptyFilter(), newOnly: true }).map((i) => i.finding.fingerprint)).toEqual(["x"]);
  });
  it("sums what changed since the previous runs", () => {
    expect(changeSummary(changed, [])).toMatchObject({ compared: 1, new: 1, fixed: 3, sinceMs: 5, fixedTop: [{ title: "Old", severity: "medium" }] });
    expect(changeSummary({ results: [run("complete", [])], open: changed.open }, []).compared).toBe(0);
  });
  it("a first run marks nothing new", () => {
    expect(findingItems({ results: [run("complete", [f("x")])], open: changed.open }, []).every((i) => !i.isNew)).toBe(true);
  });
});

describe("demo: rescans and review reasons", () => {
  it("a rescan compares with the previous run and history keeps the counts", () => {
    const demo = new DemoAnalyzers();
    const id = demo.list().find((a) => a.state === "approved")!.id;
    const report = { agents: [], generated_at_ms: 0 } as never;
    const second = demo.run(id, report);
    expect(second.changes).toBeTruthy();
    expect(demo.history(id)[0]).toMatchObject({ new: second.changes!.new_total, fixed: second.changes!.fixed_total });
  });
  it("keeps the reason with a review and drops it on reopen", () => {
    const demo = new DemoAnalyzers();
    const fp = demo.insights().results[0].findings[0].fingerprint;
    demo.review(fp, "reviewed", "Documented webhook");
    expect(demo.insights().results[0].findings.find((x) => x.fingerprint === fp)?.note).toBe("Documented webhook");
    demo.review(fp, "open");
    expect(demo.insights().results[0].findings.find((x) => x.fingerprint === fp)?.note).toBeNull();
  });
});
