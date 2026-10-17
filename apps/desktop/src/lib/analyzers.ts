// Opt-in analyzers: wire types mirroring holdmap-analyzers, plus pure helpers that join findings
// to agents for Home, the map, Needs you and the agent Insights section.
import type { Agent, AgentsReport } from "./types";

export type Severity = "info" | "low" | "medium" | "high" | "critical";
export type FindingState = "open" | "reviewed" | "snoozed";
export type RunStatus = "complete" | "partial" | "failed";
export type AnalyzerState = "not_installed" | "not_approved" | "approved" | "changed";

export interface Subject { id: string; kind: "skill" | "package" | "agent"; name: string; agents: string[] }
export interface Finding {
  fingerprint: string;
  analyzer: string;
  rule_id: string;
  category: string | null;
  severity: Severity;
  title: string;
  subject: string;
  location: { file: string | null; line: number | null };
  fix: string | null;
  refs: string[];
  state: FindingState;
}
export interface Detail { subject: string; label: string; value: string }
export interface AnalyzerResult {
  analyzer: string;
  analyzer_version: string | null;
  run_at_ms: number;
  status: RunStatus;
  coverage: number | null;
  subjects: Subject[];
  findings: Finding[];
  details: Detail[];
  notes: string[];
  input_signature?: string | null;
}
export interface SeverityCounts { critical: number; high: number; medium: number; low: number; info: number }
export interface Permissions { read: string[]; network: string[]; network_reason: string | null; sends: string[]; spawns_mcp_servers: boolean }
export interface AnalyzerView {
  id: string;
  name: string;
  publisher: string;
  summary: string;
  homepage: string;
  license: string;
  install: string | null;
  kind: "builtin" | "cli" | "mcp";
  state: AnalyzerState;
  changes: string[];
  permissions: Permissions;
  sandbox: "basic";
  binary: string | null;
  version: string | null;
  approved_at_ms: number | null;
  last_run: { run_at_ms: number; status: RunStatus; subjects: number; open: SeverityCounts } | null;
}
export interface Preview {
  analyzer: AnalyzerView;
  command: string[] | null;
  subjects: { subject: Subject; input: string }[];
  skipped: string[];
  payload: string[];
  env: string[];
  sandbox: string;
  schedule: string;
}
export interface Insights { results: AnalyzerResult[]; open: SeverityCounts }

export const SEVERITY_ORDER: Severity[] = ["critical", "high", "medium", "low", "info"];
const RANK: Record<Severity, number> = { info: 0, low: 1, medium: 2, high: 3, critical: 4 };

export const STATE_LABEL: Record<AnalyzerState, string> = {
  not_installed: "Not installed",
  not_approved: "Off",
  approved: "On",
  changed: "Changed: review again",
};

export function emptyCounts(): SeverityCounts {
  return { critical: 0, high: 0, medium: 0, low: 0, info: 0 };
}

export function total(c: SeverityCounts): number {
  return c.critical + c.high + c.medium + c.low + c.info;
}

/** The worst severity with a non-zero count, or null. */
export function worst(c: SeverityCounts): Severity | null {
  return SEVERITY_ORDER.find((s) => c[s] > 0) ?? null;
}

/** "1 critical, 2 medium" (most severe first; zero counts omitted). */
export function countsText(c: SeverityCounts, max = 2): string {
  const parts = SEVERITY_ORDER.filter((s) => c[s] > 0).map((s) => `${c[s]} ${s}`);
  return parts.slice(0, max).join(", ") + (parts.length > max ? ", …" : "");
}

function analyzerName(id: string): string {
  return id === "nvidia.skillspector" ? "SkillSpector" : id === "holdmap.osv" ? "OSV lookup" : id;
}

/** Open findings about subjects that belong to `product`, most severe first. */
export function agentFindings(insights: Insights | null, product: string, includeClosed = false): { finding: Finding; subject: Subject | null; result: AnalyzerResult }[] {
  if (!insights) return [];
  const out: { finding: Finding; subject: Subject | null; result: AnalyzerResult }[] = [];
  for (const result of insights.results) {
    const subjects = new Map(result.subjects.map((s) => [s.id, s]));
    for (const finding of result.findings) {
      const subject = subjects.get(finding.subject) ?? null;
      if (!subject?.agents.includes(product)) continue;
      if (!includeClosed && finding.state !== "open") continue;
      out.push({ finding, subject, result });
    }
  }
  return out.sort((a, b) => RANK[b.finding.severity] - RANK[a.finding.severity] || a.finding.title.localeCompare(b.finding.title));
}

/** Open finding counts for one agent. */
export function agentCounts(insights: Insights | null, product: string): SeverityCounts {
  const c = emptyCounts();
  for (const { finding } of agentFindings(insights, product)) c[finding.severity]++;
  return c;
}

/** Details (risk score, recommendation) for a subject. */
export function subjectDetails(result: AnalyzerResult, subject: string): Detail[] {
  return result.details.filter((d) => d.subject === subject);
}

export interface InsightNeed {
  id: string;
  agentId: string;
  severity: "high" | "medium";
  title: string;
  detail: string;
}

/** One "Needs you" item per agent with open medium-or-worse findings. */
export function insightNeeds(insights: Insights | null, report: AgentsReport | null): InsightNeed[] {
  if (!insights || !report) return [];
  const seen = new Set<string>();
  const items: InsightNeed[] = [];
  for (const agent of report.agents) {
    if (seen.has(agent.product)) continue;
    seen.add(agent.product);
    const found = agentFindings(insights, agent.product).filter(({ finding }) => RANK[finding.severity] >= RANK.medium);
    if (!found.length) continue;
    const top = found[0];
    const c = agentCounts(insights, agent.product);
    const severity = RANK[top.finding.severity] >= RANK.high ? "high" : "medium";
    const where = top.subject ? ` in ${top.subject.name}` : "";
    items.push({
      id: `analyzer:${agent.product}`,
      agentId: agent.id,
      severity,
      title: `${agent.name}: ${analyzerName(top.finding.analyzer)} found ${countsText(c)}${where}`,
      detail: `${top.finding.title}${top.finding.fix ? `. ${top.finding.fix}` : ""}`,
    });
  }
  return items;
}

/** Agents with open high or critical findings (their map node is ringed). */
export function flaggedAgents(insights: Insights | null, agents: Agent[]): Set<string> {
  return new Set(agents.filter((a) => agentFindings(insights, a.product).some(({ finding }) => RANK[finding.severity] >= RANK.high)).map((a) => a.id));
}

/** "file:line" for a finding, or null. */
export function findingWhere(f: Finding): string | null {
  if (!f.location.file) return null;
  return f.location.line ? `${f.location.file}:${f.location.line}` : f.location.file;
}

/** Whether an analyzer can be run now. */
export function canRun(a: AnalyzerView): boolean {
  return a.state === "approved";
}

/** "just now", "5 min ago", "3 h ago", "2 days ago". */
export function ago(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86_400) return `${Math.floor(s / 3600)} h ago`;
  const d = Math.floor(s / 86_400);
  return `${d} ${d === 1 ? "day" : "days"} ago`;
}
