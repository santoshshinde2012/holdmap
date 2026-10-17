// Browser demo for analyzers: labelled sample results over the mock agents. Approval and runs
// are simulated in memory; nothing is installed, executed or sent anywhere.
import type { AnalyzerResult, AnalyzerView, FindingState, Insights, Preview, Subject } from "./analyzers";
import { emptyCounts } from "./analyzers";
import type { AgentsReport } from "./types";

const SKILLSPECTOR = "nvidia.skillspector";
const OSV = "holdmap.osv";
const RUN_AT = Date.UTC(2026, 9, 17, 9, 30);

const BASE: Record<string, Omit<AnalyzerView, "state" | "approved_at_ms" | "last_run" | "version">> = {
  [OSV]: {
    id: OSV, name: "OSV vulnerability lookup", publisher: "holdmap", license: "Apache-2.0", homepage: "https://osv.dev", install: null, kind: "builtin",
    summary: "Checks pinned npm and PyPI packages that your MCP servers launch against the OSV.dev vulnerability database.",
    changes: [], sandbox: "basic", binary: null,
    permissions: { read: [], network: ["api.osv.dev"], network_reason: "OSV.dev is a public vulnerability database run by Google; the lookup happens there.", sends: ["The ecosystem, name and exact version of each pinned package. Nothing else: no paths, file contents, environment or machine details."], spawns_mcp_servers: false },
  },
  [SKILLSPECTOR]: {
    id: SKILLSPECTOR, name: "SkillSpector", publisher: "NVIDIA", license: "Apache-2.0", homepage: "https://github.com/NVIDIA/SkillSpector", kind: "cli",
    install: "uv tool install git+https://github.com/NVIDIA/skillspector.git@v2.13.0",
    summary: "Static scan of agent skills for prompt injection, data exfiltration, dangerous code and vulnerable dependencies.",
    changes: [], sandbox: "basic", binary: "/Users/dev/.local/bin/skillspector",
    permissions: { read: ["{subject.dir}"], network: ["api.osv.dev"], network_reason: "SkillSpector looks up dependency versions it finds in the skill on OSV and has no switch to turn that off.", sends: ["Package names and versions found in the skill go to api.osv.dev."], spawns_mcp_servers: false },
  },
};

const skill = (id: string, name: string, agents: string[]): Subject => ({ id, kind: "skill", name, agents });

function skillspectorResult(): AnalyzerResult {
  const releaseCheck = skill("claude:project:release-check", "release-check", ["claude-code"]);
  const frontend = skill("shared:project:frontend-checks", "frontend-checks", ["cursor"]);
  const review = skill("shared:user:code-review", "code-review", ["cursor"]);
  const f = (fingerprint: string, rule_id: string, category: string, severity: AnalyzerResult["findings"][number]["severity"], title: string, subject: string, file: string, line: number, fix: string) =>
    ({ fingerprint, analyzer: SKILLSPECTOR, rule_id, category, severity, title, subject, location: { file, line }, fix, refs: [], state: "open" as FindingState });
  return {
    analyzer: SKILLSPECTOR, analyzer_version: "2.13.0", run_at_ms: RUN_AT, status: "complete", coverage: 1,
    subjects: [review, frontend, releaseCheck],
    findings: [
      f("demo-p1", "P1", "Prompt Injection", "high", "Instruction Override", releaseCheck.id, "SKILL.md", 12, "Remove text that tells the agent to ignore prompts or safety rules."),
      f("demo-e1", "E1", "Data Exfiltration", "medium", "External Transmission", releaseCheck.id, "scripts/publish.sh", 8, "Verify the destination is trusted and that no secrets or tokens are sent."),
      f("demo-lp3", "LP3", "MCP Least Privilege", "low", "MCP Least Privilege", frontend.id, "SKILL.md", 1, "List the tools the skill may use in the allowed-tools frontmatter field."),
    ],
    details: [
      { subject: releaseCheck.id, label: "Risk score", value: "64 / 100" },
      { subject: releaseCheck.id, label: "Recommendation", value: "CAUTION" },
      { subject: frontend.id, label: "Risk score", value: "8 / 100" },
      { subject: frontend.id, label: "Recommendation", value: "SAFE" },
      { subject: review.id, label: "Risk score", value: "0 / 100" },
      { subject: review.id, label: "Recommendation", value: "SAFE" },
    ],
    notes: [],
  };
}

function osvResult(report: AgentsReport): AnalyzerResult {
  const packages = osvPackages(report);
  return {
    analyzer: OSV, analyzer_version: null, run_at_ms: Date.now(), status: "complete", coverage: null,
    subjects: packages.map((p) => ({ id: `package:${p.ecosystem}/${p.name}@${p.version}`, kind: "package", name: `${p.name}@${p.version}`, agents: p.agents })),
    findings: [],
    details: packages.map((p) => ({ subject: `package:${p.ecosystem}/${p.name}@${p.version}`, label: "Known advisories", value: "none (sample data)" })),
    notes: ["Browser demo: no request was sent; results are sample data."],
  };
}

function osvPackages(report: AgentsReport) {
  const out = new Map<string, { ecosystem: string; name: string; version: string; agents: string[] }>();
  for (const a of report.agents) {
    for (const s of a.capabilities?.mcp_servers ?? []) {
      const spec = s.launch?.package;
      if (!spec || !s.launch?.pinned || s.launch.runner !== "npx") continue;
      const at = spec.lastIndexOf("@");
      if (at <= 0) continue;
      const key = spec;
      const entry = out.get(key) ?? { ecosystem: "npm", name: spec.slice(0, at), version: spec.slice(at + 1), agents: [] };
      if (!entry.agents.includes(a.product)) entry.agents.push(a.product);
      out.set(key, entry);
    }
  }
  return [...out.values()].sort((a, b) => a.name.localeCompare(b.name));
}

/** In-memory demo state. */
export class DemoAnalyzers {
  private approved = new Map<string, number>([[SKILLSPECTOR, RUN_AT - 3_600_000]]);
  private results = new Map<string, AnalyzerResult>([[SKILLSPECTOR, skillspectorResult()]]);
  private reviews = new Map<string, FindingState>();

  list(): AnalyzerView[] {
    return [OSV, SKILLSPECTOR].map((id) => this.view(id));
  }

  private view(id: string): AnalyzerView {
    const approvedAt = this.approved.get(id) ?? null;
    const result = this.results.get(id);
    const open = emptyCounts();
    for (const f of this.withReviews(result)?.findings ?? []) if (f.state === "open") open[f.severity]++;
    return {
      ...BASE[id],
      state: approvedAt ? "approved" : "not_approved",
      approved_at_ms: approvedAt,
      version: approvedAt && id === SKILLSPECTOR ? "2.13.0" : null,
      last_run: result ? { run_at_ms: result.run_at_ms, status: result.status, subjects: result.subjects.length, open } : null,
    };
  }

  preview(id: string, report: AgentsReport): Preview {
    const builtin = id === OSV;
    const packages = osvPackages(report);
    const skills = new Map<string, { subject: Subject; input: string }>();
    for (const a of report.agents) {
      for (const s of a.capabilities?.skills ?? []) {
        const dir = s.source_path.replace(/\/SKILL\.md$/, "");
        const entry = skills.get(dir) ?? { subject: skill(s.id, s.name, []), input: dir };
        if (!entry.subject.agents.includes(a.product)) entry.subject.agents.push(a.product);
        skills.set(dir, entry);
      }
    }
    const unpinned = report.agents.flatMap((a) => (a.capabilities?.mcp_servers ?? []).filter((s) => s.launch && s.launch.pinned === false && s.launch.runner === "npx").map((s) => `${s.name} (${s.launch?.package}) is not pinned to a version, so it was not checked.`));
    return {
      analyzer: this.view(id),
      command: builtin ? null : ["skillspector", "scan", "<copy of the skill folder>", "--format", "json", "--no-llm", "--output", "<private report file>"],
      subjects: builtin
        ? packages.map((p) => ({ subject: { id: `package:npm/${p.name}@${p.version}`, kind: "package", name: `${p.name}@${p.version}`, agents: p.agents }, input: `npm ${p.name} ${p.version}` }))
        : [...skills.values()].sort((a, b) => a.input.localeCompare(b.input)),
      skipped: builtin ? [...new Set(unpinned)] : [],
      payload: builtin ? [JSON.stringify({ queries: packages.map((p) => ({ package: { ecosystem: p.ecosystem, name: p.name }, version: p.version })) })] : [],
      env: builtin ? [] : ["PATH", "HOME", "TMPDIR", "TMP", "TEMP", "LANG", "NO_COLOR"],
      sandbox: builtin
        ? "Runs inside holdmap. Only the exact request shown here is sent."
        : "Basic: it gets a copy of the folder, a cleared environment and a private temporary home, and is stopped at the time limit. The operating system does not block it from reading other files or using the network.",
      schedule: "When you click Run now, and while holdmap is open when the analysed items change.",
    };
  }

  approve(id: string): AnalyzerView {
    this.approved.set(id, Date.now());
    return this.view(id);
  }

  revoke(id: string): boolean {
    this.results.delete(id);
    return this.approved.delete(id);
  }

  run(id: string, report: AgentsReport): AnalyzerResult {
    if (!this.approved.has(id)) throw new Error(`${BASE[id]?.name ?? id} is off. Review its permissions and turn it on first.`);
    const result = id === OSV ? osvResult(report) : { ...skillspectorResult(), run_at_ms: Date.now() };
    this.results.set(id, result);
    return this.withReviews(result)!;
  }

  insights(): Insights {
    const results = [...this.results.entries()].filter(([id]) => this.approved.has(id)).map(([, r]) => this.withReviews(r)!);
    const open = emptyCounts();
    for (const r of results) for (const f of r.findings) if (f.state === "open") open[f.severity]++;
    return { results, open };
  }

  review(fingerprint: string, state: FindingState) {
    if (state === "open") this.reviews.delete(fingerprint);
    else this.reviews.set(fingerprint, state);
  }

  private withReviews(r: AnalyzerResult | undefined): AnalyzerResult | undefined {
    return r && { ...r, findings: r.findings.map((f) => ({ ...f, state: this.reviews.get(f.fingerprint) ?? "open" })) };
  }
}
