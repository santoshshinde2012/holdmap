// Skill trust and risk: types mirroring `holdmap_analyzers::{trust, risk}`, the risk formula
// (a TypeScript mirror for the browser demo; the app uses the Rust one) and CSV/Markdown export.
// File contents never appear anywhere: only names, sizes, hashes and times.
import type { AnalyzerResult, Severity, SeverityCounts, Subject } from "./analyzers";

export type TrustState = "changed" | "new" | "unreadable" | "trusted";
export interface FileChange { path: string; size_before?: number | null; size_after?: number | null }
export interface ChangeSummary { added: FileChange[]; removed: FileChange[]; modified: FileChange[]; added_total: number; removed_total: number; modified_total: number }
export interface SkillTrustView {
  subject: Subject;
  state: TrustState;
  sha256?: string | null;
  file_count: number;
  total_bytes: number;
  modified_ms?: number | null;
  partial: boolean;
  first_seen_ms: number;
  seen_ms: number;
  changed_since_last_scan: boolean;
  trusted_ms?: number | null;
  reason?: string | null;
  changes?: ChangeSummary | null;
  error?: string | null;
}
export interface TrustReport { skills: SkillTrustView[]; changed: number; untrusted: number; scanned_ms?: number | null }

export const TRUST_LABEL: Record<TrustState, string> = { changed: "Changed since trusted", new: "Not trusted yet", unreadable: "Couldn't read", trusted: "Trusted" };
export const alerts = (s: TrustState) => s === "changed" || s === "new";

export type RiskBand = "none" | "low" | "elevated" | "high" | "critical";
export type RiskScope = "agent" | "folder";
export interface RiskContributor { fingerprint: string; severity: Severity; title: string; subject: string }
export interface RiskEntry { scope: RiskScope; key: string; band: RiskBand; points: number; open: SeverityCounts; changed_skills: number; top: RiskContributor[] }

/** Points per open finding and per skill changed since trusted (same as Rust `risk`). */
export const SEVERITY_POINTS: Record<Severity, number> = { critical: 40, high: 15, medium: 5, low: 1, info: 0 };
export const CHANGED_SKILL_POINTS = 10;
export const MAX_CONTRIBUTORS = 5;
const BAND_RANK: Record<RiskBand, number> = { none: 0, low: 1, elevated: 2, high: 3, critical: 4 };
const SEV_RANK: Record<Severity, number> = { info: 0, low: 1, medium: 2, high: 3, critical: 4 };
export const BAND_LABEL: Record<RiskBand, string> = { none: "None", low: "Low", elevated: "Elevated", high: "High", critical: "Critical" };

export function bandOf(points: number, anyCritical: boolean): RiskBand {
  if (anyCritical || points >= 40) return "critical";
  if (points >= 25) return "high";
  if (points >= 10) return "elevated";
  return points >= 1 ? "low" : "none";
}

/** Risks for every agent and folder, riskiest first (mirror of Rust `risk::risks`). */
export function risks(results: AnalyzerResult[], trust: TrustReport | null): RiskEntry[] {
  const subjects = new Map<string, Subject>();
  for (const r of results) for (const s of r.subjects) if (!subjects.has(s.id)) subjects.set(s.id, s);
  const keys = (s: Subject): [RiskScope, string][] => [...s.agents.map((a) => ["agent", a] as [RiskScope, string]), ...(s.dir ? [["folder", s.dir] as [RiskScope, string]] : [])];
  const by = new Map<string, { scope: RiskScope; key: string; findings: Map<string, AnalyzerResult["findings"][number]>; changed: Set<string> }>();
  const acc = (scope: RiskScope, key: string) => {
    const id = `${scope}\u0000${key}`;
    let a = by.get(id);
    if (!a) by.set(id, (a = { scope, key, findings: new Map(), changed: new Set() }));
    return a;
  };
  for (const f of results.flatMap((r) => r.findings)) {
    if (f.state !== "open" || f.severity === "info") continue;
    const s = subjects.get(f.subject);
    if (!s) continue;
    for (const [scope, key] of keys(s)) acc(scope, key).findings.set(f.fingerprint, f);
  }
  for (const s of trust?.skills ?? []) if (s.state === "changed") for (const [scope, key] of keys(s.subject)) acc(scope, key).changed.add(s.subject.id);
  const out: RiskEntry[] = [...by.values()].map((a) => {
    const found = [...a.findings.values()].sort((x, y) => SEV_RANK[y.severity] - SEV_RANK[x.severity] || (x.fingerprint < y.fingerprint ? -1 : x.fingerprint > y.fingerprint ? 1 : 0));
    const points = found.reduce((n, f) => n + SEVERITY_POINTS[f.severity], 0) + CHANGED_SKILL_POINTS * a.changed.size;
    const open: SeverityCounts = { critical: 0, high: 0, medium: 0, low: 0, info: 0 };
    for (const f of found) open[f.severity]++;
    return { scope: a.scope, key: a.key, band: bandOf(points, open.critical > 0), points, open, changed_skills: a.changed.size, top: found.slice(0, MAX_CONTRIBUTORS).map((f) => ({ fingerprint: f.fingerprint, severity: f.severity, title: f.title, subject: f.subject })) };
  });
  return out.sort((x, y) => BAND_RANK[y.band] - BAND_RANK[x.band] || y.points - x.points || (x.scope === y.scope ? 0 : x.scope === "agent" ? -1 : 1) || (x.key < y.key ? -1 : x.key > y.key ? 1 : 0));
}

/** "1 high, 2 medium, 1 changed skill" (same words as Rust `risk::explain`). */
export function explain(e: RiskEntry): string {
  const parts = (["critical", "high", "medium", "low"] as Severity[]).filter((s) => e.open[s]).map((s) => `${e.open[s]} ${s}`);
  if (e.changed_skills) parts.push(`${e.changed_skills} changed skill${e.changed_skills === 1 ? "" : "s"}`);
  return parts.length ? parts.join(", ") : "nothing open";
}

export type RiskSortKey = "band" | "key" | "scope";
export function sortRisks(list: RiskEntry[], key: RiskSortKey, dir: "asc" | "desc"): RiskEntry[] {
  const k = (e: RiskEntry) => (key === "band" ? BAND_RANK[e.band] * 1e6 + e.points : key === "scope" ? e.scope : e.key);
  const sign = dir === "asc" ? 1 : -1;
  return [...list].sort((a, b) => { const x = k(a), y = k(b); return (x < y ? -1 : x > y ? 1 : 0) * sign; });
}

/** A CSV cell that a spreadsheet won't run as a formula. */
const cell = (v: string | number) => {
  let s = String(v);
  if (/^[=+\-@\t\r]/.test(s)) s = `'${s}`;
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};
const mdCell = (v: string | number) => String(v).replace(/[\r\n]+/g, " ").replace(/\\/g, "\\\\").replace(/\|/g, "\\|").replace(/</g, "&lt;");
const RISK_COLUMNS = ["scope", "key", "band", "points", "critical", "high", "medium", "low", "changed_skills", "why"];
const riskRow = (e: RiskEntry, label: (e: RiskEntry) => string) => [e.scope, label(e), BAND_LABEL[e.band], e.points, e.open.critical, e.open.high, e.open.medium, e.open.low, e.changed_skills, explain(e)];

export function risksCsv(list: RiskEntry[], label: (e: RiskEntry) => string = (e) => e.key): string {
  return [RISK_COLUMNS.join(","), ...list.map((e) => riskRow(e, label).map(cell).join(","))].join("\r\n") + "\r\n";
}
export function risksMarkdown(list: RiskEntry[], now: number, label: (e: RiskEntry) => string = (e) => e.key): string {
  const out = [`# Holdmap top risks · ${new Date(now).toISOString().slice(0, 10)}`, "", "> Bands: critical 40, high 15, medium 5, low 1 point per open finding; 10 per skill changed since trusted. Any open critical is Critical. Findings are analyzer output holdmap hasn't verified.", ""];
  if (!list.length) return out.concat("No risks.", "").join("\n");
  out.push(`| ${RISK_COLUMNS.join(" | ")} |`, `|${" --- |".repeat(RISK_COLUMNS.length)}`);
  for (const e of list) out.push(`| ${riskRow(e, label).map(mdCell).join(" | ")} |`);
  return out.join("\n") + "\n";
}

/** "3 KB" etc. for change summaries. */
export function bytes(n: number | null | undefined): string {
  if (n == null) return "";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
