// Browser demo for analyzers: labelled sample results over the mock agents. Approval and runs
// are simulated in memory; nothing is installed, executed or sent anywhere.
import type { AnalyzerResult, AnalyzerView, CatalogDiff, CatalogStatus, FindingState, Insights, Preview, Subject } from "./analyzers";
import { emptyCounts } from "./analyzers";
import { inventories } from "./agent-config";
import { readManifest, type DemoManifest } from "./analyzer-manifest";
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
    changes: [], sandbox: "macos_seatbelt", binary: "/Users/dev/.local/bin/skillspector",
    permissions: { read: ["{subject.dir}"], network: ["api.osv.dev"], network_reason: "SkillSpector looks up dependency versions it finds in the skill on OSV and has no switch to turn that off.", sends: ["Package names and versions found in the skill go to api.osv.dev."], spawns_mcp_servers: false },
  },
};

const skill = (id: string, name: string, agents: string[]): Subject => ({ id, kind: "skill", name, agents });

function skillspectorResult(): AnalyzerResult {
  const releaseCheck = skill("claude:project:release-check", "release-check", ["claude-code"]);
  const frontend = skill("shared:project:frontend-checks", "frontend-checks", ["cursor"]);
  const review = skill("shared:user:code-review", "code-review", ["cursor", "codex"]);
  const migrations = skill("codex:user:db-migrations", "db-migrations", ["codex"]);
  const f = (fingerprint: string, rule_id: string, category: string, severity: AnalyzerResult["findings"][number]["severity"], title: string, subject: string, file: string, line: number, fix: string) =>
    ({ fingerprint, analyzer: SKILLSPECTOR, rule_id, category, severity, title, subject, location: { file, line }, fix, refs: [], state: "open" as FindingState });
  return {
    analyzer: SKILLSPECTOR, analyzer_version: "2.13.0", run_at_ms: RUN_AT, status: "complete", coverage: 1,
    subjects: [review, migrations, frontend, releaseCheck],
    findings: [
      f("demo-p1", "P1", "Prompt Injection", "high", "Instruction Override", releaseCheck.id, "SKILL.md", 12, "Remove text that tells the agent to ignore prompts or safety rules."),
      f("demo-e1", "E1", "Data Exfiltration", "medium", "External Transmission", releaseCheck.id, "scripts/publish.sh", 8, "Verify the destination is trusted and that no secrets or tokens are sent."),
      f("demo-d2", "D2", "Dangerous Code", "medium", "Destructive Shell Command", migrations.id, "scripts/reset.sh", 4, "Ask before running commands that drop or reset a database."),
      f("demo-lp3", "LP3", "MCP Least Privilege", "low", "MCP Least Privilege", frontend.id, "SKILL.md", 1, "List the tools the skill may use in the allowed-tools frontmatter field."),
    ],
    details: [
      { subject: releaseCheck.id, label: "Risk score", value: "64 / 100" },
      { subject: releaseCheck.id, label: "Recommendation", value: "CAUTION" },
      { subject: migrations.id, label: "Risk score", value: "31 / 100" },
      { subject: migrations.id, label: "Recommendation", value: "CAUTION" },
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
  for (const a of inventories(report)) {
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
  private background = new Set<string>();
  private user = new Map<string, DemoManifest>();
  private catalogState: CatalogStatus = { version: 1, published: "2026-10-18", accepted: false, pending: null };

  catalog(): CatalogStatus {
    return structuredClone(this.catalogState);
  }

  /** Sample update for the browser demo; nothing is downloaded. */
  fetchCatalog(): CatalogDiff {
    const diff: CatalogDiff = {
      from_version: this.catalogState.version, to_version: this.catalogState.version + 1, published: "2026-11-02",
      added: [],
      removed: [],
      changed: [{ id: SKILLSPECTOR, fields: [{ field: "runner.timeout_secs", before: "120", after: "300" }] }],
    };
    this.catalogState.pending = diff;
    return diff;
  }

  acceptCatalog(): CatalogStatus {
    const p = this.catalogState.pending;
    if (!p) throw new Error("the catalog file is malformed: no catalog update is waiting");
    this.catalogState = { version: p.to_version, published: p.published, accepted: true, pending: null };
    return this.catalog();
  }

  rejectCatalog(): void {
    this.catalogState.pending = null;
  }

  list(): AnalyzerView[] {
    return [SKILLSPECTOR, OSV, ...this.user.keys()].map((id) => this.view(id));
  }

  /** Simulated "Add your own analyzer": checks the manifest like holdmap; nothing is installed. */
  add(text: string): AnalyzerView {
    const m = readManifest(text, Object.keys(BASE));
    if (this.user.has(m.id)) throw new Error(`invalid manifest: ${m.id} is already registered; remove it first`);
    this.user.set(m.id, m);
    return this.view(m.id);
  }

  /** Remove a registered analyzer with its approval and results. */
  remove(id: string): boolean {
    if (BASE[id]) throw new Error(`${BASE[id].name} is in holdmap's catalog and can't be removed; turn it off instead`);
    this.revoke(id);
    return this.user.delete(id);
  }

  private base(id: string): Omit<AnalyzerView, "state" | "approved_at_ms" | "last_run" | "version"> {
    const m = this.user.get(id);
    if (!m) return BASE[id];
    return {
      id, name: m.name, publisher: m.publisher, license: m.license, homepage: m.homepage, install: null, kind: m.kind,
      summary: m.summary, changes: [], sandbox: "macos_seatbelt", binary: `/Users/dev/.local/bin/${m.command.split("/").pop()}`, source: "user",
      permissions: { read: ["{subject.dir}"], network: m.network, network_reason: m.networkReason, sends: m.sends, spawns_mcp_servers: m.kind === "mcp" },
    };
  }

  private view(id: string): AnalyzerView {
    const approvedAt = this.approved.get(id) ?? null;
    const result = this.results.get(id);
    const open = emptyCounts();
    for (const f of this.withReviews(result)?.findings ?? []) if (f.state === "open") open[f.severity]++;
    return {
      source: "catalog",
      ...this.base(id),
      state: approvedAt ? "approved" : "not_approved",
      background: this.background.has(id),
      approved_at_ms: approvedAt,
      version: approvedAt && id === SKILLSPECTOR ? "2.13.0" : null,
      last_run: result ? { run_at_ms: result.run_at_ms, status: result.status, subjects: result.subjects.length, open } : null,
    };
  }

  preview(id: string, report: AgentsReport): Preview {
    const user = this.user.get(id);
    if (user) return this.userPreview(user, report);
    const builtin = id === OSV;
    const packages = osvPackages(report);
    const skills = new Map<string, { subject: Subject; input: string }>();
    for (const a of inventories(report)) {
      for (const s of a.capabilities?.skills ?? []) {
        const dir = s.source_path.replace(/\/SKILL\.md$/, "");
        const entry = skills.get(dir) ?? { subject: skill(s.id, s.name, []), input: dir };
        if (!entry.subject.agents.includes(a.product)) entry.subject.agents.push(a.product);
        skills.set(dir, entry);
      }
    }
    const unpinned = inventories(report).flatMap((a) => (a.capabilities?.mcp_servers ?? []).filter((s) => s.launch && s.launch.pinned === false && s.launch.runner === "npx").map((s) => `${s.name} (${s.launch?.package}) is not pinned to a version, so it was not checked.`));
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
        : "macOS sandbox (best effort): it gets a copy of the folder, a cleared environment and a private temporary home, and is stopped at the time limit. It can only write inside its private folder and can't read common credential folders (.ssh, .aws, Keychains and similar). It may use the network, as its permissions say. It can still read your other files. Apple has deprecated the sandbox-exec tool this uses.",
      schedule: this.background.has(id)
        ? "When you click Run now, and while holdmap is open when the analysed items change (you allowed automatic runs that use the network)."
        : "Only when you click Run now. It uses the network, so it doesn't run on its own unless you allow automatic runs.",
    };
  }

  private skills(report: AgentsReport) {
    const skills = new Map<string, { subject: Subject; input: string }>();
    for (const a of inventories(report)) {
      for (const s of a.capabilities?.skills ?? []) {
        const dir = s.source_path.replace(/\/SKILL\.md$/, "");
        const entry = skills.get(dir) ?? { subject: skill(s.id, s.name, []), input: dir };
        if (!entry.subject.agents.includes(a.product)) entry.subject.agents.push(a.product);
        skills.set(dir, entry);
      }
    }
    return [...skills.values()].sort((a, b) => a.input.localeCompare(b.input));
  }

  private userPreview(m: DemoManifest, report: AgentsReport): Preview {
    const program = m.command.split("/").pop() ?? m.command;
    const notices: string[] = [];
    if (m.kind === "mcp") {
      notices.push("This starts a local MCP server. holdmap starts it only when you run this analyzer, with a cleared environment and the same sandbox as other analyzers, and stops it when the run ends. It never runs on its own.");
      for (const a of inventories(report)) {
        for (const s of a.capabilities?.mcp_servers ?? []) {
          if (s.name === program || (s.launch?.package ?? "").split("@")[0].endsWith(program)) {
            notices.push(`Warning: this looks like the MCP server ${a.product} is configured to use (${s.name}). holdmap starts its own copy: it gets none of your agent's settings or keys, and anything the server does when it starts (signing in, syncing, writing files it is allowed to) happens again.`);
          }
        }
      }
    }
    const args = Object.entries(m.toolArgs).map(([k, v]) => `${k}=${v === "{subject.dir}" ? "<copy of the skill folder>" : v}`).join(", ");
    return {
      analyzer: this.view(m.id),
      command: m.kind === "mcp" ? [m.command, ...m.args, `then calls tool ${m.tool}(${args}) once per skill`] : [m.command, ...m.args],
      subjects: this.skills(report),
      skipped: [],
      payload: [],
      env: ["PATH", "HOME", "TMPDIR", "TMP", "TEMP", "LANG", "NO_COLOR"],
      sandbox: "macOS sandbox (best effort): it gets a copy of the folder, a cleared environment and a private temporary home, and is stopped at the time limit. It can only write inside its private folder and can't read common credential folders (.ssh, .aws, Keychains and similar). " + (m.network.length ? "It may use the network, as its permissions say." : "The operating system blocks its network access.") + " It can still read your other files. Apple has deprecated the sandbox-exec tool this uses.",
      schedule: m.kind === "mcp" ? "Only when you click Run now. MCP analyzers never run on their own." : "When you click Run now, and while holdmap is open when the analysed items change.",
      notices,
    };
  }

  approve(id: string): AnalyzerView {
    this.approved.set(id, Date.now());
    this.background.delete(id);
    return this.view(id);
  }

  setBackground(id: string, allow: boolean): AnalyzerView {
    const m = this.user.get(id);
    if (m?.kind === "mcp") throw new Error(`${m.name} starts an MCP server, so it runs only when you click Run now.`);
    if (!this.approved.has(id)) throw new Error(`${m?.name ?? BASE[id]?.name ?? id} is off. Turn it on first.`);
    if (allow) this.background.add(id);
    else this.background.delete(id);
    return this.view(id);
  }

  revoke(id: string): boolean {
    this.background.delete(id);
    this.results.delete(id);
    return this.approved.delete(id);
  }

  run(id: string, report: AgentsReport): AnalyzerResult {
    if (!this.approved.has(id)) throw new Error(`${BASE[id]?.name ?? id} is off. Review its permissions and turn it on first.`);
    const user = this.user.get(id);
    const result = user
      ? { analyzer: id, analyzer_version: null, run_at_ms: Date.now(), status: "complete" as const, coverage: null, subjects: this.skills(report).map((s) => s.subject), findings: [], details: [], notes: [`Browser demo: ${user.kind === "mcp" ? "no MCP server was started" : "nothing was run"}; this is a sample result.`] }
      : id === OSV ? osvResult(report) : { ...skillspectorResult(), run_at_ms: Date.now() };
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
