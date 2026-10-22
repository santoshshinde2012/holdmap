// Security tab view-model: pure functions that shape analyzer state into what the Security
// components draw (summary, grouped findings, gallery cards, run progress). No Svelte, no I/O,
// so every rule here is unit-tested and the components stay small.
import { SEVERITY_ORDER, emptyCounts, findingWhere, total, type AnalyzerResult, type AnalyzerView, type Finding, type Insights, type Severity, type SeverityCounts, type Subject } from "./analyzers";
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
}
export type GroupBy = "severity" | "skill" | "analyzer";
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
      });
    }
  }
  return out.sort((a, b) => RANK[b.finding.severity] - RANK[a.finding.severity] || a.finding.title.localeCompare(b.finding.title));
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
    return [i.finding.title, i.finding.rule_id, i.subject?.name ?? i.finding.subject, i.analyzerName, i.where ?? "", i.finding.fix ?? ""].some((t) => t.toLowerCase().includes(q));
  });
}

/** Groups in a stable order: severity most severe first; skills and analyzers by worst finding. */
export function groupFindings(items: FindingItem[], by: GroupBy): FindingGroup[] {
  const groups = new Map<string, FindingGroup>();
  for (const i of items) {
    const key = by === "severity" ? i.finding.severity : by === "skill" ? i.finding.subject : i.analyzer;
    const label = by === "severity" ? cap(i.finding.severity) : by === "skill" ? (i.subject?.name ?? i.finding.subject) : i.analyzerName;
    const g = groups.get(key) ?? { key, label, severity: null, items: [] };
    g.items.push(i);
    if (!g.severity || RANK[i.finding.severity] > RANK[g.severity]) g.severity = i.finding.severity;
    groups.set(key, g);
  }
  const list = [...groups.values()];
  if (by === "severity") return SEVERITY_ORDER.map((s) => groups.get(s)).filter((g): g is FindingGroup => !!g);
  return list.sort((a, b) => RANK[b.severity!] - RANK[a.severity!] || b.items.length - a.items.length || a.label.localeCompare(b.label));
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

export type RunPhase = "idle" | "running" | "done" | "failed";
export interface RunStep { id: string; name: string; phase: RunPhase; note: string | null }
export interface RunProgress { steps: RunStep[]; done: number; total: number; phase: RunPhase }

export function runProgress(steps: RunStep[]): RunProgress {
  const done = steps.filter((s) => s.phase === "done" || s.phase === "failed").length;
  const phase: RunPhase = !steps.length ? "idle" : steps.some((s) => s.phase === "running" || s.phase === "idle") ? "running" : steps.some((s) => s.phase === "failed") ? "failed" : "done";
  return { steps, done, total: steps.length, phase };
}
