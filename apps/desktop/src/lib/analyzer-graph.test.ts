import { describe, expect, it } from "vitest";
import { buildOverview, buildPipeline, readingOrder } from "./analyzer-graph";
import type { AnalyzerResult, AnalyzerView, Insights, Preview } from "./analyzers";
import type { AgentsReport } from "./types";

const view = (over: Partial<AnalyzerView> = {}): AnalyzerView => ({
  id: "local.scan", name: "Scan", publisher: "unknown", summary: "", homepage: "", license: "unknown", install: null, kind: "mcp",
  state: "approved", changes: [], sandbox: "linux_landlock", binary: "/opt/scan", version: null, approved_at_ms: 1, last_run: null,
  permissions: { read: ["{subject.dir}"], network: [], network_reason: null, sends: [], spawns_mcp_servers: true },
  output_format: "auto",
  mcp: { transport: "stdio", endpoint: "/opt/scan --stdio", remote: false, remote_opt_in: false, tools: ["list_files", "scan_skill"], listing: { count: 3, server: null, listed_at_ms: 1, pin: "abcdef123456", flagged: 1, approved: true }, needs_tools: false },
  ...over,
});
const subj = (id: string, agents: string[]) => ({ id, kind: "skill" as const, name: id, agents });
const preview: Preview = {
  analyzer: view(), command: ["/opt/scan --stdio", "then calls list_files, scan_skill once per skill"],
  subjects: [{ subject: subj("deploy", ["claude-code", "cursor"]), input: "/s/deploy" }, { subject: subj("lint", ["cursor"]), input: "/s/lint" }],
  skipped: [], payload: [], env: ["PATH", "HOME"], sandbox: "Linux Landlock: …", schedule: "Only when you click Run now.",
  projection: ["file names in the skill folder", "skill name"],
  tool_calls: [{ tool: "list_files", subject: "deploy", arguments: { files: ["SKILL.md"] }, problems: [] }],
};
const finding = (subject: string, severity: "high" | "low", state: "open" | "reviewed" = "open") => ({ fingerprint: `${subject}-${severity}-${state}`, analyzer: "local.scan", rule_id: "r", category: null, severity, title: "t", subject, location: { file: null, line: null }, fix: null, refs: [], state });
const result: AnalyzerResult = {
  analyzer: "local.scan", analyzer_version: null, run_at_ms: 1_700_000_000_000, status: "complete", coverage: null,
  subjects: [subj("deploy", ["claude-code", "cursor"]), subj("lint", ["cursor"])],
  findings: [finding("deploy", "high"), finding("lint", "low"), finding("lint", "low", "reviewed")],
  details: [{ subject: "lint", label: "Output of word_count", value: "SKILL.md has 3 words." }], notes: [],
  duration_ms: 1840, tool_calls: [{ tool: "list_files", calls: 2, failed: 0 }, { tool: "scan_skill", calls: 2, failed: 1 }],
};

describe("analyzer pipeline graph", () => {
  it("has every stage in order, with real numbers from the preview and the last run", () => {
    const g = buildPipeline({ view: view(), preview, result });
    expect(readingOrder(g).map((n) => n.id)).toEqual(["sources", "projection", "sandbox", "analyzer", "parser", "findings", "surface:home", "surface:needs", "surface:insights"]);
    const by = Object.fromEntries(g.nodes.map((n) => [n.id, n]));
    expect(by.sources.subtitle).toBe("2 skill folders");
    expect(by.sources.badges).toEqual(["2 agents"]);
    expect(by.projection.lines.map((l) => l.value)).toContain("file names in the skill folder");
    expect(by.projection.lines.some((l) => l.label === "list_files · deploy")).toBe(true);
    expect(by.sandbox.subtitle).toBe("Linux Landlock");
    expect(by.sandbox.badges).toEqual(["no network"]);
    expect(by.analyzer.badges).toEqual(["On", "Tools pinned (abcdef123456)"]);
    expect(by.analyzer.lines.find((l) => l.label === "Tool scan_skill")?.value).toBe("2 calls, 1 failed last run");
    expect(by.parser.status).toBe("warn");
    expect(by.parser.badges).toEqual(["1 excerpt"]);
    expect(by.findings.subtitle).toBe("2 open · 1 high, 1 low");
    expect(by.findings.status).toBe("error");
    expect(by.findings.badges).toEqual(["complete", "1.8 s"]);
    expect(by["surface:home"].subtitle).toBe("2 agent cards marked");
    expect(by["surface:needs"].subtitle).toBe("2 items (medium or worse)");
    expect(by["surface:insights"].subtitle).toBe("2 open findings");
    expect(g.edges.find((e) => e.id === "e:analyzer")?.label).toBe("4 calls");
    expect(g.edges.every((e) => g.nodes.some((n) => n.id === e.from) && g.nodes.some((n) => n.id === e.to))).toBe(true);
  });

  it("shows consent, pin and not-run states honestly", () => {
    const g = buildPipeline({ view: view({ state: "changed", changes: ["tool list_files changed (description)"], mcp: { ...view().mcp!, listing: { ...view().mcp!.listing!, approved: false } } }), preview: null, result: null });
    const by = Object.fromEntries(g.nodes.map((n) => [n.id, n]));
    expect(by.sources.subtitle).toBe("Loading…");
    expect(by.analyzer.status).toBe("warn");
    expect(by.analyzer.badges).toContain("Tool list changed: review again");
    expect(by.findings.subtitle).toBe("Not run yet");
    expect(g.edges.filter((e) => e.active)).toEqual([]);
  });

  it("labels remote HTTP servers as metadata only and unsandboxed", () => {
    const g = buildPipeline({ view: view({ permissions: { ...view().permissions, network: ["scan.example.com"] }, mcp: { ...view().mcp!, transport: "http", remote: true } }), preview, result: null });
    const by = Object.fromEntries(g.nodes.map((n) => [n.id, n]));
    expect(by.sandbox.subtitle).toBe("Another machine");
    expect(by.sandbox.status).toBe("warn");
    expect(by.projection.badges).toEqual(["metadata only"]);
  });
});

describe("analyzer overview graph", () => {
  it("links each analyzer to the agents its last run covered, with findings", () => {
    const report = { agents: [{ id: "a1", product: "cursor", name: "Cursor" }, { id: "a2", product: "claude-code", name: "Claude Code" }, { id: "a3", product: "codex", name: "Codex" }] } as unknown as AgentsReport;
    const insights: Insights = { results: [result], open: { critical: 0, high: 1, medium: 0, low: 1, info: 0 } };
    const g = buildOverview([view(), view({ id: "holdmap.osv", name: "OSV", kind: "builtin", state: "not_approved", mcp: null })], insights, report);
    expect(g.edges.map((e) => [e.to, e.label])).toEqual([["agent:claude-code", "1 skill · 1 high"], ["agent:cursor", "2 skills · 1 high, …"]]);
    const codex = g.nodes.find((n) => n.id === "agent:codex")!;
    expect(codex.subtitle).toBe("Not covered by an analyzer");
    const cursor = g.nodes.find((n) => n.id === "agent:cursor")!;
    expect(cursor.status).toBe("error");
    expect(g.nodes.find((n) => n.id === "analyzer:holdmap.osv")?.subtitle).toBe("Off");
  });
});
