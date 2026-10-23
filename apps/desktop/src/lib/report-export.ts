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

/** Findings as the Security tab lists them (open ones unless the caller passes more). */
export function securityReport(items: FindingItem[], summary: SecuritySummary, now: number) {
  return {
    schema: REPORT_SCHEMA,
    kind: "security" as const,
    generated_at: new Date(now).toISOString(),
    open: summary.open,
    last_scan: summary.lastRun ? new Date(summary.lastRun).toISOString() : null,
    coverage: { agents_scanned: summary.agents.scanned, agents_total: summary.agents.total, skills_scanned: summary.skills.scanned, skills_total: summary.skills.total },
    counts: counts(items),
    findings: items.map((i) => ({
      id: i.finding.rule_id,
      fingerprint: i.finding.fingerprint,
      severity: i.finding.severity,
      state: i.finding.state,
      title: line(i.finding.title),
      analyzer: i.analyzerName,
      subject: i.subject ? { kind: i.subject.kind, name: i.subject.name } : null,
      agents: i.agents,
      location: i.finding.location.file ? { file: redactHome(i.finding.location.file), line: i.finding.location.line } : null,
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
export function securitySarif(items: FindingItem[], version?: string) {
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
        const physical = f.location.file
          ? { physicalLocation: { artifactLocation: { uri: redactHome(f.location.file) }, ...(f.location.line ? { region: { startLine: f.location.line } } : {}) } }
          : {};
        const where = { ...physical, ...logical };
        return {
          ruleId: f.rule_id,
          level: LEVEL[f.severity],
          message: { text: line(f.fix ? `${f.title}. Fix: ${f.fix}` : f.title) },
          partialFingerprints: { holdmap: f.fingerprint },
          locations: Object.keys(where).length ? [where] : [],
          properties: { analyzer: i.analyzerName, state: f.state, agents: i.agents },
        };
      }),
    }],
  };
}

/** A file name safe for the native save dialog: `holdmap-<what>-<date>.<ext>`. */
export function reportFilename(what: string, now: number, ext: "json" | "sarif"): string {
  return `holdmap-${what}-${day(now)}.${ext}`;
}

/** Save through the native chooser in the app, or a download in the browser. */
export async function saveReport(filename: string, data: unknown): Promise<{ saved: boolean }> {
  const text = JSON.stringify(data, null, 2) + "\n";
  const blob = new Blob([text], { type: "application/json" });
  const r = await saveRecording({ blob, bytes: blob.size, filename });
  return { saved: r.saved };
}
