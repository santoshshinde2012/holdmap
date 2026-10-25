// Guided installs, run progress, the results table's shaping and every export format.
import { describe, expect, it, vi } from "vitest";
import installRs from "../../../../crates/holdmap-analyzers/src/install.rs?raw";
import { DemoAnalyzers } from "./analyzers-demo";
import { DEMO_RECIPES, beginInstall, demoInstall, demoPlan, initialInstall, reduceInstall, MAX_LOG, type InstallEvent, type InstallPlan, type InstallRecord } from "./analyzer-install";
import { InstallDialog } from "./install-dialog.svelte";
import { SecurityRuns } from "./security-runs.svelte";
import { applyFilter, emptyFilter, findingItems, groupFindings, runSummary, runProgress, scopeLabel, securitySummary, sortFindings, stepLabel, suggestedActions, type FindingItem } from "./security-view";
import { csvCell, securityCsv, securityMarkdown, securityReport, securitySarif, CSV_COLUMNS } from "./report-export";

const demo = new DemoAnalyzers();
const insights = demo.insights();
const analyzers = demo.list();
const names = new Map([["claude-code", "Claude Code"], ["cursor", "Cursor"]]);
const items = findingItems(insights, analyzers, names);
const summary = securitySummary(insights, analyzers, [{ product: "claude-code", name: "Claude Code" }], 5);

describe("install recipes", () => {
  it("the browser demo mirrors every recipe compiled into install.rs", () => {
    const ids = [...installRs.matchAll(/id: "([^"]+)",\n\s+method: InstallMethod::(\w+),\n\s+package: "([^"]+)",\n\s+version: "([^"]+)"/g)];
    expect(ids.length).toBeGreaterThan(0);
    expect(Object.keys(DEMO_RECIPES).sort()).toEqual(ids.map((m) => m[1]).sort());
    const method = { UvTool: "uv_tool", GoInstall: "go_install", Manual: "manual" } as Record<string, string>;
    for (const [, id, m, pkg, version] of ids) {
      expect(DEMO_RECIPES[id]).toMatchObject({ method: method[m], package: pkg, version });
    }
  });
  it("every installable recipe pins an exact version", () => {
    for (const r of Object.values(DEMO_RECIPES)) {
      if (r.method === "manual") continue;
      expect(r.package).toMatch(/==\d|@v\d/);
      expect(r.hosts.length).toBeGreaterThan(0);
    }
  });
});

describe("install plan (demo)", () => {
  it("shows the package, source, holdmap folder, hosts and command before anything runs", () => {
    const p = demoPlan("cisco.skill-scanner", "Cisco", "uv tool install x", ["skill folders"], [], null, "");
    expect(p).toMatchObject({ can_install: true, blocked: null, version: "2.3.0", network: ["pypi.org", "files.pythonhosted.org"] });
    expect(p.target_dir).toContain("holdmap/analyzers/tools/cisco.skill-scanner");
    expect(p.command).toEqual(["/opt/homebrew/bin/uv", "tool", "install", "--force", "--no-config", "cisco-ai-skill-scanner==2.3.0"]);
    expect(p.command.join(" ")).not.toMatch(/sudo/);
  });
  it("blocks with a reason when the package manager is missing", () => {
    const p = demoPlan("google.osv-scanner", "OSV", null, [], [], null, "no-go");
    expect(p.can_install).toBe(false);
    expect(p.toolchain?.path).toBeNull();
    expect(p.blocked).toMatch(/needs go, which wasn't found/);
    expect(demoPlan("cisco.skill-scanner", "C", null, [], [], null, "no-uv").blocked).toMatch(/needs uv/);
  });
  it("manual recipes and unknown analyzers can't install and say the safest way", () => {
    expect(demoPlan("aquasecurity.trivy-secrets", "Trivy", null, [], [], null, "").blocked).toMatch(/package manager/);
    expect(demoPlan("someone.else", "X", "do it", [], [], null, "").blocked).toMatch(/no install recipe/);
  });
});

describe("install state", () => {
  const record: InstallRecord = { analyzer: "a", version: "1", binary: "/b", sha256: "0".repeat(64), installed_at_ms: 1, method: "uv_tool", package: "p" };
  it("moves review → installing → verifying → done and keeps the log", () => {
    let s = initialInstall();
    s = reduceInstall(s, { kind: "started", command: ["uv", "tool", "install", "p"] }, 5);
    expect(s).toMatchObject({ phase: "installing", startedAt: 5, log: ["$ uv tool install p"] });
    s = reduceInstall(s, { kind: "log", line: "ok" });
    s = reduceInstall(s, { kind: "verifying" });
    expect(s.phase).toBe("verifying");
    s = reduceInstall(s, { kind: "done", record });
    expect(s).toMatchObject({ phase: "done", record, log: ["$ uv tool install p", "ok"] });
    expect(reduceInstall(s, { kind: "failed", reason: "late" })).toBe(s);
  });
  it("records failure reasons and cancellation, and caps the log", () => {
    expect(reduceInstall(beginInstall(), { kind: "failed", reason: "exit 2" })).toMatchObject({ phase: "failed", reason: "exit 2" });
    expect(reduceInstall(beginInstall(), { kind: "cancelled" }).reason).toMatch(/cancelled/);
    let s = beginInstall();
    for (let i = 0; i < MAX_LOG + 10; i++) s = reduceInstall(s, { kind: "log", line: String(i) });
    expect(s.log).toHaveLength(MAX_LOG);
  });
});

describe("demo install", () => {
  const plan = demoPlan("pycqa.bandit", "Bandit", null, [], [], null, "");
  it("emits the same updates as the app and returns the verified version", async () => {
    const seen: InstallEvent["kind"][] = [];
    const r = await demoInstall(plan, (e) => seen.push(e.kind), () => false, "", 0);
    expect(r.version).toBe("1.9.4");
    expect(seen[0]).toBe("started");
    expect(seen.slice(-2)).toEqual(["verifying", "done"]);
  });
  it("fails with a reason", async () => {
    const seen: InstallEvent[] = [];
    await expect(demoInstall(plan, (e) => seen.push(e), () => false, "fail", 0)).rejects.toThrow(/exit code 2/);
    expect(seen.at(-1)).toMatchObject({ kind: "failed" });
  });
  it("cancels", async () => {
    const seen: InstallEvent[] = [];
    await expect(demoInstall(plan, (e) => seen.push(e), () => true, "", 0)).rejects.toThrow();
    expect(seen.at(-1)).toEqual({ kind: "cancelled" });
  });
});

describe("install dialog", () => {
  const plan = (over: Partial<InstallPlan> = {}): InstallPlan => ({ ...demoPlan("pycqa.bandit", "Bandit", null, [], [], null, ""), ...over });
  it("installs nothing until the user agrees", async () => {
    const run = vi.fn(async () => ({}) as InstallRecord);
    const d = new InstallDialog(async () => plan(), run, async () => {}, async () => {});
    await d.show("pycqa.bandit");
    await d.install();
    expect(run).not.toHaveBeenCalled();
    d.agreed = true;
    await d.install();
    expect(run).toHaveBeenCalledOnce();
    expect(d.state.phase).toBe("done");
    expect(d.attempts).toBe(1);
  });
  it("never installs a blocked plan, even after agreeing", async () => {
    const run = vi.fn(async () => ({}) as InstallRecord);
    const d = new InstallDialog(async () => plan({ can_install: false, blocked: "no uv" }), run, async () => {}, async () => {});
    await d.show("x"); d.agreed = true; await d.install();
    expect(run).not.toHaveBeenCalled();
  });
  it("shows the failure reason, allows retry and asks again each time it opens", async () => {
    let fail = true;
    const d = new InstallDialog(async () => plan(), async (_id, on) => { if (fail) { on({ kind: "failed", reason: "network down" }); throw new Error("network down"); } on({ kind: "done", record: {} as InstallRecord }); return {} as InstallRecord; }, async () => {}, async () => {});
    await d.show("pycqa.bandit"); d.agreed = true; await d.install();
    expect(d.state).toMatchObject({ phase: "failed", reason: "network down" });
    fail = false; await d.install();
    expect(d.state.phase).toBe("done");
    await d.show("pycqa.bandit");
    expect(d.agreed).toBe(false);
  });
  it("cancel reaches the installer only while busy, and close waits for it", async () => {
    const cancel = vi.fn(async () => {});
    let release!: () => void;
    const d = new InstallDialog(async () => plan(), (_id, on) => new Promise((_res, rej) => { on({ kind: "started", command: [] }); release = () => { on({ kind: "cancelled" }); rej(new Error("cancelled")); }; }), cancel, async () => {});
    await d.cancel();
    expect(cancel).not.toHaveBeenCalled();
    await d.show("pycqa.bandit"); d.agreed = true;
    const p = d.install();
    await Promise.resolve();
    expect(d.busy).toBe(true);
    d.close();
    expect(d.open).toBe(true);
    await d.cancel();
    expect(cancel).toHaveBeenCalledOnce();
    release(); await p;
    expect(d.state.phase).toBe("cancelled");
    d.close();
    expect(d.open).toBe(false);
  });
});

describe("run progress", () => {
  const a = [{ id: "a", name: "A", state: "approved" as const }, { id: "b", name: "B", state: "approved" as const }];
  it("tracks live steps, timings and a summary", async () => {
    let t = 0;
    let push!: (p: { id: string; event: { step: "scanning"; index: number; total: number; subject: string | null; agents: string[] } }) => void;
    const subscribe = vi.fn(async (f: typeof push) => { push = f; return () => {}; });
    let mid = "";
    const r = new SecurityRuns(async (id) => { push({ id, event: { step: "scanning", index: 1, total: 3, subject: "deploy", agents: [] } }); mid = stepLabel(r.stepFor(id)!); return { status: "complete", findings: [], subjects: [] }; }, async () => {}, async () => {}, subscribe as never, () => (t += 1000));
    await r.start(a);
    expect(mid).toMatch(/deploy/);
    expect(r.progress).toMatchObject({ done: 2, total: 2, phase: "done" });
    expect(r.finishedAt! - r.startedAt!).toBeGreaterThan(0);
    expect(runSummary(r.progress)).toMatch(/2/);
  });
  it("cancel stops the running analyzer and skips the queued ones", async () => {
    const cancelRun = vi.fn(async () => {});
    let reject!: (e: Error) => void;
    const r = new SecurityRuns((id) => (id === "a" ? new Promise((_, rej) => (reject = rej)) : Promise.resolve({ status: "complete" as const })), async () => {}, cancelRun, async () => () => {});
    const p = r.start(a);
    await Promise.resolve(); await Promise.resolve();
    expect(r.busy).toBe(true);
    await r.cancel();
    reject(new Error("The run was cancelled."));
    await p;
    expect(cancelRun).toHaveBeenCalledOnce();
    expect(r.steps.map((s) => s.phase)).toEqual(["cancelled", "cancelled"]);
    expect(runProgress(r.steps).phase).toBe("cancelled");
  });
});

describe("results shaping", () => {
  it("each finding knows its agents, folder, scope and source path; unknowns say so", () => {
    const withDir = items.find((i) => i.folder)!;
    expect(withDir.path?.startsWith(withDir.folder!)).toBe(true);
    const unknown = items.find((i) => !i.folder);
    if (unknown) expect(scopeLabel(unknown.scope)).toMatch(/^Unknown/);
    expect(scopeLabel("user")).toMatch(/^Global/);
    expect(scopeLabel("project")).toMatch(/^Project/);
    expect(scopeLabel(null)).toMatch(/^Unknown/);
  });
  it("sorts by any column both ways", () => {
    const asc = sortFindings(items, { key: "rule", dir: "asc" }).map((i) => i.finding.rule_id);
    expect(asc).toEqual([...asc].sort());
    expect(sortFindings(items, { key: "rule", dir: "desc" }).map((i) => i.finding.rule_id)).toEqual([...asc].reverse());
    expect(sortFindings(items, { key: "severity", dir: "desc" })[0].finding.severity).toBe("high");
  });
  it("groups by agent (a finding under each agent) and by folder", () => {
    const byAgent = groupFindings(items, "agent");
    const placements = items.reduce((n, i) => n + Math.max(1, i.agents.length), 0);
    expect(byAgent.reduce((n, g) => n + g.items.length, 0)).toBe(placements);
    expect(groupFindings(items, "folder").length).toBeGreaterThan(0);
  });
  it("search matches folder and agent names", () => {
    const i = items.find((x) => x.folder)!;
    const part = i.folder!.split("/").filter(Boolean).at(-1)!;
    expect(applyFilter(items, { ...emptyFilter(), query: part })).toContain(i);
    expect(applyFilter(items, { ...emptyFilter(), query: "claude code" }).length).toBeGreaterThan(0);
  });
  it("suggests next steps without claiming holdmap verified anything", () => {
    const acts = suggestedActions(items[0]);
    expect(acts.at(-1)?.kind).toBe("snooze");
    expect(acts.map((a) => a.label).join(" ")).not.toMatch(/verified|confirmed|safe/i);
  });
});

describe("exports carry only what's shown", () => {
  const shown: FindingItem[] = applyFilter(items, { ...emptyFilter(), severities: ["medium"] });
  const note = "Only what's on screen: open findings, severity medium";
  it("JSON", () => {
    const r = securityReport(shown, summary, 0, note);
    expect(r.findings).toHaveLength(shown.length);
    expect(r.included).toBe(note);
    expect(r.note).toMatch(/hasn't verified/);
    expect(r.findings.every((f) => f.severity === "medium")).toBe(true);
    expect(r.findings[0]).toHaveProperty("folder");
    expect(r.findings[0]).toHaveProperty("scope");
  });
  it("SARIF 2.1.0", () => {
    const s = securitySarif(shown, "1.0.0", note);
    expect(s.version).toBe("2.1.0");
    expect(s.runs[0].results).toHaveLength(shown.length);
    expect(s.runs[0].properties).toEqual({ included: note, unverified: true });
    expect(s.runs[0].results.every((r) => r.level === "warning")).toBe(true);
    expect(JSON.stringify(s)).not.toMatch(/\/Users\/|\/home\//);
  });
  it("CSV", () => {
    const csv = securityCsv(shown);
    const rows = csv.trimEnd().split("\r\n");
    expect(rows[0]).toBe(CSV_COLUMNS.join(","));
    expect(rows).toHaveLength(shown.length + 1);
    expect(csv.endsWith("\r\n")).toBe(true);
    expect(csvCell("=HYPERLINK(1)")).toBe("'=HYPERLINK(1)");
    expect(csvCell('a,"b"')).toBe('"a,""b"""');
    expect(csvCell(null)).toBe("");
  });
  it("Markdown", () => {
    const m = securityMarkdown(shown, summary, 0, note);
    expect(m).toContain(`**Included:** ${note.replace(/'/g, "'")}`);
    expect(m).toContain("hasn't verified");
    expect(m.split("\n").filter((l) => l.startsWith("| ") && !l.startsWith("| Severity") && !l.startsWith("| ---"))).toHaveLength(shown.length);
    expect(securityMarkdown([], summary, 0)).toContain("No findings.");
  });
});
