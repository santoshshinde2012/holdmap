// Shareable reports built from what the app already shows: a text summary to paste into a
// ticket or chat, JSON for scripts, and SARIF 2.1.0 for code-scanning tools. Pure functions, no
// I/O. Privacy: nothing here reads a command line, an environment value, a prompt, a chat or a
// finding's raw evidence; home folders in paths become "~". Everything is built from
// already-fetched, already-displayed fields, so exporting never starts a scan or a process.
import { SEVERITY_ORDER, type Severity } from "./analyzers";
import type { AttentionItem, Overview } from "./home";
import type { FindingItem, SecuritySummary } from "./security-view";
import type { Agent, AgentsReport } from "./types";
import { saveRecording } from "./screen-record-save";

export const REPORT_SCHEMA = "holdmap-report/1";
const INFO_URI = "https://github.com/santoshshinde2012/holdmap";

/** `/Users/ana/x`, `/home/ana/x` and `C:\Users\ana\x` become `~/x`. */
export function redactHome(path: string): string {
  return path.replace(/^(?:\/(?:Users|home)\/[^/]+|[A-Za-z]:[\\/]Users[\\/][^\\/]+)(?=[\\/]|$)/, "~").replace(/\\/g, "/");
}

const day = (ms: number) => new Date(ms).toISOString().slice(0, 10);
/** A local calendar date, for dates the user picked (snooze ends). */
const localDay = (ms: number) => { const d = new Date(ms); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`; };
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const num = (n: number | null) => (n === null ? "unknown" : String(n));
const sevWord = (s: string) => s[0].toUpperCase() + s.slice(1);
/** One line, no markdown or control characters that could break a pasted list. */
const line = (s: string) => s.replace(/[\r\n\t]+/g, " ").trim();

function agentRows(report: AgentsReport | null) {
  return (report?.agents ?? []).map((a: Agent) => ({
    name: a.name,
    vendor: a.vendor,
    kind: a.kind,
    pid: a.pid,
    processes: a.process_ids.length,
    memory_bytes: a.memory_bytes,
    ports: a.ports.map((p) => ({ port: p.port, exposure: p.exposure })),
  }));
}

/** What a scan found, for the Home tab. `needs` are the "Needs you" items as shown. */
export function homeReport(o: Overview, needs: AttentionItem[], report: AgentsReport | null, now: number) {
  return {
    schema: REPORT_SCHEMA,
    kind: "home" as const,
    generated_at: new Date(now).toISOString(),
    platform: report?.platform ?? null,
    overview: { agents: o.agents, listening: o.ports, dev_servers: o.devServers, exposed: o.exposed, mcp_configured: o.mcpConfigured, mcp_running: o.mcpRunning },
    // The "settings" rule quotes a config value; the title already says what is risky.
    needs_you: needs.map((n) => ({ rule: n.rule, severity: n.severity, title: line(n.title), ...(n.rule === "settings" ? {} : { detail: line(n.detail) }) })),
    agents: agentRows(report),
  };
}

export function homeSummaryText(o: Overview, needs: AttentionItem[], now: number): string {
  const lines = [
    `Holdmap summary · ${day(now)}`,
    `${plural(o.agents ?? 0, "agent")}, ${num(o.ports)} listening, ${num(o.devServers)} dev ${o.devServers === 1 ? "server" : "servers"}, ${num(o.exposed)} exposed to the network.`,
  ];
  if (o.mcpConfigured !== null) lines.push(`MCP servers: ${o.mcpRunning ?? 0} running of ${o.mcpConfigured} configured.`);
  lines.push("");
  if (!needs.length) lines.push("Needs you: nothing right now.");
  else {
    lines.push(`Needs you (${needs.length}):`);
    for (const n of needs) lines.push(`- [${sevWord(n.severity)}] ${line(n.title)}${n.rule === "settings" ? "" : ` — ${line(n.detail)}`}`);
  }
  return lines.join("\n") + "\n";
}

const LEVEL: Record<Severity, "error" | "warning" | "note"> = { critical: "error", high: "error", medium: "warning", low: "note", info: "note" };

function counts(items: FindingItem[]): Record<Severity, number> {
  const c = { critical: 0, high: 0, medium: 0, low: 0, info: 0 } as Record<Severity, number>;
  for (const i of items) c[i.finding.severity]++;
  return c;
}

/** Where a finding sits, home folder hidden; "unknown" when holdmap couldn't confirm it. */
function place(i: FindingItem) {
  return {
    folder: i.folder ? redactHome(i.folder) : null,
    scope: i.scope,
    path: i.path ? redactHome(i.path) : null,
    line: i.finding.location.line,
  };
}

/** Findings as the Security tab lists them. `included` says which ones (the on-screen filter). */
export function securityReport(items: FindingItem[], summary: SecuritySummary, now: number, included = "Open findings") {
  return {
    schema: REPORT_SCHEMA,
    kind: "security" as const,
    generated_at: new Date(now).toISOString(),
    included: line(included),
    note: "Findings are what each analyzer reported; holdmap hasn't verified them.",
    open: summary.open,
    last_scan: summary.lastRun ? new Date(summary.lastRun).toISOString() : null,
    coverage: { agents_scanned: summary.agents.scanned, agents_total: summary.agents.total, skills_scanned: summary.skills.scanned, skills_total: summary.skills.total },
    counts: counts(items),
    findings: items.map((i) => ({
      id: i.finding.rule_id,
      fingerprint: i.finding.fingerprint,
      severity: i.finding.severity,
      state: i.finding.state,
      new_since_previous_run: i.isNew,
      review_reason: i.finding.note ? line(i.finding.note) : null,
      title: line(i.finding.title),
      analyzer: i.analyzerName,
      subject: i.subject ? { kind: i.subject.kind, name: i.subject.name } : null,
      agents: i.agents,
      location: i.finding.location.file ? { file: redactHome(i.finding.location.file), line: i.finding.location.line } : null,
      ...place(i),
      fix: i.finding.fix ? line(i.finding.fix) : null,
      references: i.finding.refs,
    })),
  };
}

export function securitySummaryText(items: FindingItem[], summary: SecuritySummary, now: number): string {
  const c = counts(items);
  const lines = [
    `Holdmap security summary · ${day(now)}`,
    `${plural(summary.open, "open finding")}${summary.open ? ` (${SEVERITY_ORDER.filter((s) => c[s]).map((s) => `${c[s]} ${s}`).join(", ")})` : ""}.`,
    `Scanned ${summary.agents.scanned} of ${plural(summary.agents.total, "agent")} and ${summary.skills.scanned} of ${plural(summary.skills.total, "skill")}${summary.lastRun ? `, last on ${day(summary.lastRun)}` : ", no scan yet"}.`,
    "",
  ];
  if (!items.length) lines.push("No findings.");
  for (const i of items) {
    const who = [i.subject?.name, ...i.agents].filter(Boolean).join(" · ");
    lines.push(`- [${sevWord(i.finding.severity)}] ${line(i.finding.title)}${who ? ` (${line(who)})` : ""} — ${line(i.analyzerName)}${i.finding.fix ? `. Fix: ${line(i.finding.fix)}` : ""}`);
  }
  return lines.join("\n") + "\n";
}

/** SARIF 2.1.0: one run, one rule per rule id, one result per finding. */
export function securitySarif(items: FindingItem[], version?: string, included = "Open findings") {
  const rules = new Map<string, { id: string; name: string; shortDescription: { text: string }; defaultConfiguration: { level: string }; properties: { tags: string[] } }>();
  for (const i of items) {
    const f = i.finding;
    if (!rules.has(f.rule_id)) {
      rules.set(f.rule_id, { id: f.rule_id, name: f.rule_id, shortDescription: { text: line(f.title) }, defaultConfiguration: { level: LEVEL[f.severity] }, properties: { tags: f.category ? [f.category] : [] } });
    }
  }
  return {
    $schema: "https://json.schemastore.org/sarif-2.1.0.json",
    version: "2.1.0",
    runs: [{
      tool: { driver: { name: "Holdmap", ...(version ? { version } : {}), informationUri: INFO_URI, rules: [...rules.values()] } },
      results: items.map((i) => {
        const f = i.finding;
        const logical = i.subject ? { logicalLocations: [{ name: i.subject.name, kind: i.subject.kind }] } : {};
        const file = i.path ?? f.location.file;
        const physical = file
          ? { physicalLocation: { artifactLocation: { uri: redactHome(file) }, ...(f.location.line ? { region: { startLine: f.location.line } } : {}) } }
          : {};
        const where = { ...physical, ...logical };
        return {
          ruleId: f.rule_id,
          level: LEVEL[f.severity],
          message: { text: line(f.fix ? `${f.title}. Fix: ${f.fix}` : f.title) },
          partialFingerprints: { holdmap: f.fingerprint },
          locations: Object.keys(where).length ? [where] : [],
          properties: { analyzer: i.analyzerName, state: f.state, agents: i.agents, skill: i.subject?.name ?? null, folder: i.folder ? redactHome(i.folder) : null, scope: i.scope },
        };
      }),
      properties: { included: line(included), unverified: true },
    }],
  };
}

export const CSV_COLUMNS = ["severity", "rule", "message", "analyzer", "agents", "skill", "scope", "folder", "path", "line", "state", "snoozed_until", "fix", "new", "reason"] as const;

/** A CSV cell: quoted when needed, and never starting with = + - @ so a spreadsheet can't run it. */
export function csvCell(v: string | number | null): string {
  let s = v === null ? "" : line(String(v));
  if (/^[=+\-@]/.test(s)) s = `'${s}`;
  return /[",]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** RFC 4180 CSV, one row per finding shown. */
export function securityCsv(items: FindingItem[]): string {
  const rows = items.map((i) => {
    const p = place(i);
    return [i.finding.severity, i.finding.rule_id, i.finding.title, i.analyzerName, i.agents.join("; "), i.subject?.name ?? i.finding.subject, p.scope, p.folder ?? "unknown", p.path, p.line, i.finding.state, i.finding.until_ms ? localDay(i.finding.until_ms) : null, i.finding.fix, i.isNew ? "yes" : "no", i.finding.note ?? null].map(csvCell).join(",");
  });
  return [CSV_COLUMNS.join(","), ...rows].join("\r\n") + "\r\n";
}

/** A Markdown cell: one line, backslashes and pipes escaped. */
const md = (s: string | number | null) => (s === null || s === "" ? "—" : line(String(s)).replace(/\\/g, "\\\\").replace(/\|/g, "\\|").replace(/</g, "&lt;"));

/** Markdown for a ticket or PR: summary, inclusion note, then a table. */
export function securityMarkdown(items: FindingItem[], summary: SecuritySummary, now: number, included = "Open findings"): string {
  const c = counts(items);
  const out = [
    `# Holdmap security findings · ${day(now)}`,
    "",
    `**Included:** ${md(included)} (${plural(items.length, "finding")}).  `,
    `**Coverage:** ${summary.agents.scanned} of ${plural(summary.agents.total, "agent")}, ${summary.skills.scanned} of ${plural(summary.skills.total, "skill")}${summary.lastRun ? `, last scan ${day(summary.lastRun)}` : ", no scan yet"}.  `,
    `**By severity:** ${SEVERITY_ORDER.map((s) => `${sevWord(s)} ${c[s]}`).join(" · ")}`,
    "",
    "> Findings are what each analyzer reported; holdmap hasn't verified them. Home folders are shown as `~`.",
    "",
  ];
  if (!items.length) return out.concat("No findings.", "").join("\n");
  out.push("| Severity | Rule | Message | Agents | Skill | Scope | Folder | Path | State | Reason |", "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
  for (const i of items) {
    const p = place(i);
    const f = i.finding;
    const state = f.state === "snoozed" && f.until_ms ? `snoozed until ${localDay(f.until_ms)}` : f.state;
    out.push(`| ${sevWord(f.severity)} | ${md(f.rule_id)} | ${md(f.title)} | ${md(i.agents.join(", "))} | ${md(i.subject?.name ?? f.subject)} | ${md(p.scope)} | ${md(p.folder ?? "unknown")} | ${md(p.path ? `${p.path}${p.line ? `:${p.line}` : ""}` : null)} | ${md(state)} | ${md(f.state === "open" ? null : f.note ?? null)} |`);
  }
  return out.join("\n") + "\n";
}

export type ReportExt = "json" | "sarif" | "csv" | "md";

/** A file name safe for the native save dialog: `holdmap-<what>-<date>.<ext>`. */
export function reportFilename(what: string, now: number, ext: ReportExt): string {
  return `holdmap-${what}-${day(now)}.${ext}`;
}

const MIME: Record<ReportExt, string> = { json: "application/json", sarif: "application/sarif+json", csv: "text/csv", md: "text/markdown" };

/** Save through the native chooser in the app, or a download in the browser. Strings are saved
 *  as they are (CSV, Markdown); anything else as pretty JSON. */
export async function saveReport(filename: string, data: unknown): Promise<{ saved: boolean }> {
  const ext = (filename.split(".").pop() ?? "json") as ReportExt;
  const text = typeof data === "string" ? data : JSON.stringify(data, null, 2) + "\n";
  const blob = new Blob([text], { type: MIME[ext] ?? "application/json" });
  const r = await saveRecording({ blob, bytes: blob.size, filename });
  return { saved: r.saved };
}
