import { describe, expect, it } from "vitest";
import { bandOf, explain, risks, risksCsv, risksMarkdown, sortRisks } from "./risk";
import { bytes, type TrustReport } from "./trust";
import type { AnalyzerResult, Finding, Severity } from "./analyzers";

const f = (fp: string, severity: Severity, subject: string, state: Finding["state"] = "open"): Finding => ({ fingerprint: fp, analyzer: "a", rule_id: "r", category: null, severity, title: `t ${fp}`, subject, location: { file: null, line: null }, fix: null, refs: [], state });
const result = (findings: Finding[]): AnalyzerResult => ({
  analyzer: "a", analyzer_version: null, run_at_ms: 0, status: "ok", coverage: null, details: [], notes: [], findings,
  subjects: [{ id: "s1", kind: "skill", name: "one", agents: ["claude-code"], dir: "/x/one" }, { id: "s2", kind: "skill", name: "two", agents: ["codex"], dir: "/x/two" }],
} as unknown as AnalyzerResult);

describe("risk (mirror of Rust risk::risks)", () => {
  it("bands by points, any critical is Critical", () => {
    expect([0, 1, 9, 10, 24, 25, 39, 40].map((p) => bandOf(p, false))).toEqual(["none", "low", "low", "elevated", "elevated", "high", "high", "critical"]);
    expect(bandOf(1, true)).toBe("critical");
  });
  it("adds open findings once per fingerprint, skips closed and info, adds changed skills", () => {
    const r = result([f("1", "high", "s1"), f("1", "high", "s1"), f("2", "medium", "s1"), f("3", "critical", "s1", "reviewed"), f("4", "info", "s1"), f("5", "low", "s2")]);
    const trust: TrustReport = { changed: 1, untrusted: 0, skills: [{ subject: r.subjects[0], state: "changed", file_count: 1, total_bytes: 1, partial: false, first_seen_ms: 0, seen_ms: 0, changed_since_last_scan: true }] };
    const list = risks([r], trust);
    expect(list.map((e) => `${e.scope}:${e.key}:${e.band}:${e.points}`)).toEqual(["agent:claude-code:high:30", "folder:/x/one:high:30", "agent:codex:low:1", "folder:/x/two:low:1"]);
    expect(explain(list[0])).toBe("1 high, 1 medium, 1 changed skill");
    expect(list[0].top.map((c) => c.fingerprint)).toEqual(["1", "2"]);
  });
  it("sorts and exports safely", () => {
    const list = risks([result([f("1", "high", "s1"), f("5", "low", "s2")])], null);
    expect(sortRisks(list, "key", "asc").map((e) => e.key)[0]).toBe("/x/one");
    const csv = risksCsv(list, (e) => (e.key === "codex" ? "=evil" : e.key));
    expect(csv.split("\r\n")[0]).toBe("scope,key,band,points,critical,high,medium,low,changed_skills,why");
    expect(csv).toContain(",'=evil,");
    const md = risksMarkdown(list, 0);
    expect(md).toContain("| scope | key | band |");
    expect(md).toContain("| agent | claude-code | Elevated | 15 |");
    expect(risksMarkdown([], 0)).toContain("No risks.");
    expect([bytes(10), bytes(2048), bytes(3 * 1024 * 1024)]).toEqual(["10 B", "2.0 KB", "3.0 MB"]);
  });
});

describe("demo rescans", async () => {
  const { varied } = await import("./analyzers-demo");
  const base = { analyzer: "x", run_at_ms: 0, subjects: [], findings: [{ fingerprint: "demo-lp3" }, { fingerprint: "other" }], notes: [] } as never;
  it("vary on every other rescan and say they're demo data", () => {
    const even = varied(base, 0), odd = varied(base, 1);
    expect(even.findings.map((f) => f.fingerprint)).toEqual(["demo-lp3", "other"]);
    expect(odd.findings.map((f) => f.fingerprint)).toEqual(["other", "demo-p2-1"]);
    expect(odd.notes[0]).toMatch(/demo/i);
  });
});

describe("trust store", async () => {
  const { trustStore } = await import("./trust-store.svelte");
  it("loads the demo and finds an agent's alerting skills by the agents that load them", async () => {
    await trustStore.load();
    expect(trustStore.alerting.map((s) => s.subject.name).sort()).toEqual(["db-migrations", "release-check"]);
    expect(trustStore.forAgent("claude-code").map((s) => s.subject.name)).toEqual(["release-check"]);
    expect(trustStore.forAgent("cursor")).toEqual([]);
  });
});
