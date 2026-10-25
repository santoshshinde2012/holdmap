// Security tab view-model: pure functions that shape analyzer state into what the Security
// components draw (summary, grouped findings, gallery cards, run progress). No Svelte, no I/O,
// so every rule here is unit-tested and the components stay small.
import { SEVERITY_ORDER, emptyCounts, findingWhere, total, type AnalyzerResult, type AnalyzerView, type RunEvent, type Finding, type Insights, type Severity, type SeverityCounts, type Subject } from "./analyzers";
import type { PresetView } from "./analyzer-presets";

export interface Scope { product: string; name: string }

export interface SecuritySummary {
  counts: SeverityCounts;
  open: number;
  lastRun: number | null;
  analyzersOn: number;
  agents: { scanned: number; total: number };
  skills: { scanned: number; total: number };
}

/** Header numbers: open counts, last run, and how much of the inventory a scan has covered. */
export function securitySummary(insights: Insights | null, analyzers: AnalyzerView[], agents: Scope[], skillsTotal: number): SecuritySummary {
  const results = insights?.results ?? [];
  const counts = insights ? { ...insights.open } : emptyCounts();
  const runs = results.map((r) => r.run_at_ms);
  const scannedAgents = new Set<string>();
  const scannedSkills = new Set<string>();
  for (const r of results) for (const s of r.subjects) {
    for (const a of s.agents) scannedAgents.add(a);
    if (s.kind === "skill") scannedSkills.add(s.id);
  }
  const products = new Set(agents.map((a) => a.product));
  return {
    counts,
    open: total(counts),
    lastRun: runs.length ? Math.max(...runs) : null,
    analyzersOn: analyzers.filter((a) => a.state === "approved").length,
    agents: { scanned: [...products].filter((p) => scannedAgents.has(p)).length, total: products.size },
    skills: { scanned: scannedSkills.size, total: Math.max(skillsTotal, scannedSkills.size) },
  };
}

export interface FindingItem {
  finding: Finding;
  subject: Subject | null;
  analyzer: string;
  analyzerName: string;
  agents: string[];
  where: string | null;
  evidence: { label: string; value: string }[];
  /** The scanned folder, or null when holdmap can't confirm it (show "unknown"). */
  folder: string | null;
  /** "global", "project", "local", or "unknown". */
  scope: string;
  /** The file the finding points at, inside `folder` when both are known. */
  path: string | null;
}
export type GroupBy = "severity" | "skill" | "analyzer" | "agent" | "folder";
export type SortKey = "severity" | "rule" | "message" | "agent" | "skill" | "folder" | "path";
export interface SortSpec { key: SortKey; dir: "asc" | "desc" }
export interface FindingFilter { severities: Severity[]; query: string; agent: string | null; showClosed: boolean }
export interface FindingGroup { key: string; label: string; severity: Severity | null; items: FindingItem[] }

const RANK: Record<Severity, number> = { info: 0, low: 1, medium: 2, high: 3, critical: 4 };

export function emptyFilter(): FindingFilter {
  return { severities: [], query: "", agent: null, showClosed: false };
}

/** Every finding from every analyzer's last run, joined to its subject and evidence. */
export function findingItems(insights: Insights | null, analyzers: AnalyzerView[], agentNames: Map<string, string> = new Map()): FindingItem[] {
  const names = new Map(analyzers.map((a) => [a.id, a.name]));
  const out: FindingItem[] = [];
  for (const r of insights?.results ?? []) {
    const subjects = new Map(r.subjects.map((s) => [s.id, s]));
    for (const f of r.findings) {
      const subject = subjects.get(f.subject) ?? null;
      out.push({
        finding: f,
        subject,
        analyzer: r.analyzer,
        analyzerName: names.get(r.analyzer) ?? r.analyzer,
        agents: (subject?.agents ?? []).map((p) => agentNames.get(p) ?? p),
        where: findingWhere(f),
        evidence: evidence(r, f),
        folder: subject?.dir ?? null,
        scope: subject?.scope ?? "unknown",
        path: joinPath(subject?.dir ?? null, f.location.file),
      });
    }
  }
  return out.sort((a, b) => RANK[b.finding.severity] - RANK[a.finding.severity] || a.finding.title.localeCompare(b.finding.title));
}

/** `dir/file` for a relative file inside a known folder; an absolute file as is. */
export function joinPath(dir: string | null, file: string | null): string | null {
  if (!file) return null;
  if (/^(\/|[A-Za-z]:[\\/])/.test(file) || !dir) return file;
  return `${dir.replace(/[\\/]+$/, "")}/${file.replace(/^\.\//, "")}`;
}

function evidence(r: AnalyzerResult, f: Finding): { label: string; value: string }[] {
  const rows = r.details.filter((d) => d.subject === f.subject).map((d) => ({ label: d.label, value: d.value }));
  rows.push({ label: "Rule", value: f.rule_id });
  if (f.category) rows.push({ label: "Category", value: f.category });
  return rows;
}

export function applyFilter(items: FindingItem[], filter: FindingFilter): FindingItem[] {
  const q = filter.query.trim().toLowerCase();
  return items.filter((i) => {
    if (!filter.showClosed && i.finding.state !== "open") return false;
    if (filter.severities.length && !filter.severities.includes(i.finding.severity)) return false;
    if (filter.agent && !i.subject?.agents.includes(filter.agent)) return false;
    if (!q) return true;
    return [i.finding.title, i.finding.rule_id, i.subject?.name ?? i.finding.subject, i.analyzerName, i.where ?? "", i.finding.fix ?? "", i.folder ?? "", i.agents.join(" ")].some((t) => t.toLowerCase().includes(q));
  });
}

/** Groups in a stable order: severity most severe first; skills and analyzers by worst finding. */
export function groupFindings(items: FindingItem[], by: GroupBy): FindingGroup[] {
  const groups = new Map<string, FindingGroup>();
  for (const i of items) {
    for (const [key, label] of groupKeys(i, by)) {
      const g = groups.get(key) ?? { key, label, severity: null, items: [] };
      g.items.push(i);
      if (!g.severity || RANK[i.finding.severity] > RANK[g.severity]) g.severity = i.finding.severity;
      groups.set(key, g);
    }
  }
  const list = [...groups.values()];
  if (by === "severity") return SEVERITY_ORDER.map((s) => groups.get(s)).filter((g): g is FindingGroup => !!g);
  return list.sort((a, b) => RANK[b.severity!] - RANK[a.severity!] || b.items.length - a.items.length || a.label.localeCompare(b.label));
}

/** A finding loaded by two agents shows under both when grouping by agent. */
function groupKeys(i: FindingItem, by: GroupBy): [string, string][] {
  switch (by) {
    case "severity": return [[i.finding.severity, cap(i.finding.severity)]];
    case "skill": return [[i.finding.subject, i.subject?.name ?? i.finding.subject]];
    case "analyzer": return [[i.analyzer, i.analyzerName]];
    case "folder": return [[i.folder ?? "unknown", i.folder ?? "Folder unknown"]];
    case "agent": return i.agents.length ? i.agents.map((a) => [a, a] as [string, string]) : [["unknown", "Agent unknown"]];
  }
}

const sortValue = (i: FindingItem, key: SortKey): string | number => {
  switch (key) {
    case "severity": return RANK[i.finding.severity];
    case "rule": return i.finding.rule_id;
    case "message": return i.finding.title;
    case "agent": return i.agents.join(", ");
    case "skill": return i.subject?.name ?? i.finding.subject;
    case "folder": return i.folder ?? "";
    case "path": return i.where ?? "";
  }
};

/** Table order; ties fall back to severity (worst first), then title, so it stays stable. */
export function sortFindings(items: FindingItem[], sort: SortSpec): FindingItem[] {
  const sign = sort.dir === "asc" ? 1 : -1;
  return [...items].sort((a, b) => {
    const x = sortValue(a, sort.key), y = sortValue(b, sort.key);
    const c = typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y));
    return sign * c || RANK[b.finding.severity] - RANK[a.finding.severity] || a.finding.title.localeCompare(b.finding.title);
  });
}

export type ActionKind = "open_folder" | "review_file" | "fix" | "ask_agent" | "snooze";
export interface SuggestedAction { kind: ActionKind; label: string }

/** What a person can do about a finding, in the order worth trying. Never claims holdmap
 *  verified anything: findings are the analyzer's word, unverified. */
export function suggestedActions(i: FindingItem): SuggestedAction[] {
  const out: SuggestedAction[] = [];
  if (i.path) out.push({ kind: "review_file", label: `Open ${i.where ?? i.path} and read the flagged lines` });
  if (i.finding.fix) out.push({ kind: "fix", label: "Apply the fix above, or remove the flagged text" });
  if (i.folder) out.push({ kind: "open_folder", label: "Show the skill's folder to edit or remove it" });
  if (i.agents.length) out.push({ kind: "ask_agent", label: `Until it's fixed, turn the skill off in ${i.agents.join(", ")}` });
  out.push({ kind: "snooze", label: "If it's expected, mark it accepted so it stops counting" });
  return out;
}

/** Scope in plain words; "unknown" when holdmap couldn't tell. */
export function scopeLabel(scope: string | null | undefined): string {
  switch (scope) {
    case "user":
    case "global": return "Global — every project on this machine";
    case "project": return "Project — this repository only";
    case "local": return "Local — this repository, not committed";
    default: return "Unknown — holdmap couldn't confirm the scope";
  }
}

/** Open findings for one agent (catalog product id), e.g. for its card or detail view. */
export function agentFindingCounts(insights: Insights | null, product: string): SeverityCounts {
  const c = emptyCounts();
  for (const r of insights?.results ?? []) {
    const subjects = new Map(r.subjects.map((s) => [s.id, s]));
    for (const f of r.findings) if (f.state === "open" && subjects.get(f.subject)?.agents.includes(product)) c[f.severity]++;
  }
  return c;
}

/** Counts of the filtered-but-unsevered set, for the severity chips. */
export function severityCounts(items: FindingItem[]): SeverityCounts {
  const c = emptyCounts();
  for (const i of items) c[i.finding.severity]++;
  return c;
}

export const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

// ---- Analyzer gallery ----

export type CardStatus = "on" | "off" | "changed" | "not_installed" | "available";
export type CardAction = "run" | "enable" | "review" | "install" | "add";
export interface AnalyzerCard {
  id: string;
  name: string;
  initial: string;
  publisher: string;
  checks: string;
  target: "Skills" | "MCP packages";
  status: CardStatus;
  statusLabel: string;
  action: CardAction;
  actionLabel: string;
  network: string[];
  lastRun: number | null;
  open: number;
}

const STATUS_LABEL: Record<CardStatus, string> = { on: "On", off: "Off", changed: "Changed", not_installed: "Not installed", available: "Available" };
const ACTION_LABEL: Record<CardAction, string> = { run: "Run", enable: "Enable…", review: "Review…", install: "Install…", add: "Add…" };

/** One card per configured analyzer, then presets not yet added. Exactly one primary action each.
 *  Enable, Review, Install and Add open the consent sheet in Settings; only Run acts here. */
export function analyzerCards(analyzers: AnalyzerView[], presets: PresetView[]): AnalyzerCard[] {
  const cards: AnalyzerCard[] = analyzers.map((a) => {
    const status: CardStatus = a.state === "approved" ? "on" : a.state === "changed" ? "changed" : a.state === "not_installed" ? "not_installed" : "off";
    const action: CardAction = status === "on" ? "run" : status === "changed" ? "review" : status === "not_installed" ? "install" : "enable";
    return card(a.id, a.name, a.publisher, a.summary, a.kind === "builtin" ? "MCP packages" : "Skills", status, action, a.permissions.network, a.last_run?.run_at_ms ?? null, a.last_run ? total(a.last_run.open) : 0);
  });
  for (const p of presets) {
    if (p.added || analyzers.some((a) => a.id === p.id)) continue;
    cards.push(card(p.id, p.name, p.publisher, p.summary, "Skills", p.installed ? "available" : "not_installed", p.installed ? "add" : "install", p.network, null, 0));
  }
  return cards;
}

function card(id: string, name: string, publisher: string, checks: string, target: AnalyzerCard["target"], status: CardStatus, action: CardAction, network: string[], lastRun: number | null, open: number): AnalyzerCard {
  return { id, name, initial: (name.match(/[A-Za-z0-9]/)?.[0] ?? "?").toUpperCase(), publisher, checks: firstSentence(checks), target, status, statusLabel: STATUS_LABEL[status], action, actionLabel: ACTION_LABEL[action], network, lastRun, open };
}

/** The first sentence, so cards stay one idea long. */
export function firstSentence(s: string): string {
  const m = s.match(/^(.+?[.!?])(\s|$)/);
  return (m ? m[1] : s).replace(/[.]$/, "");
}

// ---- Runs ----

export type RunPhase = "idle" | "running" | "done" | "failed" | "cancelled";
export interface RunStep {
  id: string;
  name: string;
  /** "idle" means queued. */
  phase: RunPhase;
  note: string | null;
  /** The latest live update from the run (names only). */
  event?: RunEvent | null;
  startedAt?: number | null;
  finishedAt?: number | null;
  /** Findings and subjects the finished run reported. */
  findings?: number | null;
  subjects?: number | null;
}
export interface RunProgress { steps: RunStep[]; done: number; total: number; phase: RunPhase }

const finished = (p: RunPhase) => p === "done" || p === "failed" || p === "cancelled";

export function runProgress(steps: RunStep[]): RunProgress {
  const done = steps.filter((s) => finished(s.phase)).length;
  const phase: RunPhase = !steps.length ? "idle"
    : steps.some((s) => s.phase === "running" || s.phase === "idle") ? "running"
    : steps.some((s) => s.phase === "failed") ? "failed"
    : steps.some((s) => s.phase === "cancelled") ? "cancelled" : "done";
  return { steps, done, total: steps.length, phase };
}

/** What one analyzer is doing, in a few words: "Scanning release-check (2 of 4)". */
export function stepLabel(s: RunStep): string {
  switch (s.phase) {
    case "idle": return "Queued";
    case "cancelled": return "Cancelled";
    case "failed": return s.note ? `Failed: ${s.note}` : "Failed";
    case "done": {
      const found = s.findings == null ? "" : s.findings === 1 ? " · 1 finding" : ` · ${s.findings} findings`;
      return `${s.note ? `Done (${s.note})` : "Done"}${found}`;
    }
    case "running": {
      const e = s.event;
      if (!e) return "Starting";
      const of = e.total > 0 && e.index > 0 ? ` (${e.index} of ${e.total})` : "";
      const what = e.subject ?? (e.total ? `${e.total} ${e.total === 1 ? "item" : "items"}` : "");
      switch (e.step) {
        case "preparing": return "Preparing: checking approval and the program";
        case "copying": return `Preparing a private copy of ${what}${of}`;
        case "scanning": return `Scanning ${what}${of}`.trim();
        case "collecting": return "Collecting results";
      }
    }
  }
}

/** Share of the run done, 0..1, counting per-item steps when the analyzer reports them. */
export function runFraction(p: RunProgress): number {
  if (!p.total) return 0;
  let sum = 0;
  for (const s of p.steps) {
    if (finished(s.phase)) sum += 1;
    else if (s.phase === "running" && s.event) {
      const e = s.event;
      const inner = e.step === "preparing" ? 0.05 : e.step === "collecting" ? 0.95 : e.total ? (e.index - (e.step === "copying" ? 0.5 : 0.25)) / e.total : 0.5;
      sum += Math.max(0, Math.min(0.95, inner));
    }
  }
  return sum / p.total;
}

/** "1:05" or "12s". */
export function elapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** One-line wrap-up once every analyzer finished. */
export function runSummary(p: RunProgress): string {
  const ok = p.steps.filter((s) => s.phase === "done");
  const failed = p.steps.filter((s) => s.phase === "failed").length;
  const cancelled = p.steps.filter((s) => s.phase === "cancelled").length;
  const findings = ok.reduce((n, s) => n + (s.findings ?? 0), 0);
  const parts = [`${ok.length} of ${p.total} ${p.total === 1 ? "analyzer" : "analyzers"} finished`, `${findings} ${findings === 1 ? "finding" : "findings"}`];
  if (failed) parts.push(`${failed} failed`);
  if (cancelled) parts.push(`${cancelled} cancelled`);
  return parts.join(" · ");
}
