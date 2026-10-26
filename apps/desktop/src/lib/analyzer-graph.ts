// Pure builders for the analyzer graphs. The pipeline graph shows how one analyzer works, from
// the inventory to where its findings appear, with real data from its permissions preview and
// last run. The overview graph shows which analyzer covers which agent and what it found.
// Positions are computed here so the Svelte Flow components only draw.
import type { AnalyzerResult, AnalyzerView, Insights, Preview, Severity, SeverityCounts } from "./analyzers";
import { SANDBOX_LABEL, SEVERITY_ORDER, STATE_LABEL, countsText, emptyCounts, total, worst } from "./analyzers";
import type { AgentsReport } from "./types";
import { inventories } from "./agent-config";
import { productLabel } from "./run-view";

export type StageKind = "sources" | "projection" | "sandbox" | "analyzer" | "parser" | "findings" | "surface";
export type NodeStatus = "ok" | "warn" | "error" | "idle";
export interface GraphLine { label: string; value: string }
export interface GraphNode {
  id: string;
  kind: StageKind | "agent";
  title: string;
  subtitle: string;
  status: NodeStatus;
  /** Short chips on the card. */
  badges: string[];
  /** Everything the details panel shows. */
  lines: GraphLine[];
  position: { x: number; y: number };
  width: number;
  height: number;
  /** Severity counts drawn as a bar (findings and agents). */
  counts?: SeverityCounts;
}
export interface GraphEdge { id: string; from: string; to: string; label: string | null; active: boolean; severity: Severity | null }
export interface Graph { nodes: GraphNode[]; edges: GraphEdge[] }

export const NODE_W = 210;
export const NODE_H = 104;
const COL = NODE_W + 64;
const ROW = NODE_H + 28;

export const STAGE_LABEL: Record<StageKind, string> = {
  sources: "Inventory",
  projection: "What it gets",
  sandbox: "Sandbox",
  analyzer: "Analyzer",
  parser: "Parser",
  findings: "Findings",
  surface: "Shown in",
};

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

function openCounts(result: AnalyzerResult | null): SeverityCounts {
  const c = emptyCounts();
  for (const f of result?.findings ?? []) if (f.state === "open") c[f.severity]++;
  return c;
}

function duration(ms: number | null | undefined): string {
  if (ms == null) return "unknown";
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s`;
}

function pinState(view: AnalyzerView): { text: string; status: NodeStatus } {
  const m = view.mcp;
  if (!m) return { text: "", status: "ok" };
  if (!m.listing) return { text: "Tools not listed yet", status: "warn" };
  if (m.listing.approved) return { text: `Tools pinned (${m.listing.pin})`, status: "ok" };
  return { text: "Tool list changed: review again", status: "warn" };
}

const PARSER_LABEL: Record<string, string> = {
  sarif: "SARIF 2.1.0",
  "holdmap-json": "holdmap JSON",
  "json-map": "JSON field mapping",
  osv: "OSV responses",
  auto: "Auto: SARIF, holdmap JSON or field mapping; else a short excerpt",
};

export interface PipelineInput {
  view: AnalyzerView;
  /** The permissions preview (real inventory); null until loaded. */
  preview: Preview | null;
  /** Its last result, if any. */
  result: AnalyzerResult | null;
}

/** Inventory → What it gets → Sandbox → Analyzer → Parser → Findings → Home / Needs you / Insights. */
export function buildPipeline({ view, preview, result }: PipelineInput): Graph {
  const nodes: GraphNode[] = [];
  const node = (n: Omit<GraphNode, "position" | "width" | "height">, col: number, row = 0, rows = 1): GraphNode => {
    const g = { ...n, width: NODE_W, height: NODE_H, position: { x: col * COL, y: (row - (rows - 1) / 2) * ROW } };
    nodes.push(g);
    return g;
  };
  const subjects = preview?.subjects ?? [];
  const agents = [...new Set(subjects.flatMap((s) => s.subject.agents))].sort();
  const skills = subjects.filter((s) => s.subject.kind === "skill").length;
  const packages = subjects.filter((s) => s.subject.kind === "package").length;
  const what = [skills ? plural(skills, "skill folder") : "", packages ? plural(packages, "package") : ""].filter(Boolean).join(" · ");
  node({
    id: "sources", kind: "sources", title: STAGE_LABEL.sources,
    subtitle: preview ? (what || "Nothing found right now") : "Loading…",
    status: preview ? (subjects.length ? "ok" : "idle") : "idle",
    badges: agents.length ? [plural(agents.length, "agent")] : [],
    lines: [
      { label: "Agents", value: agents.join(", ") || "none" },
      ...subjects.slice(0, 12).map((s) => ({ label: s.subject.kind === "package" ? "Package" : "Skill", value: `${s.subject.name} (${s.subject.agents.join(", ") || "no agent"})` })),
      ...(subjects.length > 12 ? [{ label: "More", value: plural(subjects.length - 12, "more item") }] : []),
      ...(preview?.skipped ?? []).map((s) => ({ label: "Skipped", value: s })),
    ],
  }, 0);

  const projection = preview?.projection?.length ? preview.projection
    : view.kind === "cli" ? ["path of a private copy of the skill folder"] : view.kind === "builtin" ? ["package ecosystem, name and exact version"] : [];
  node({
    id: "projection", kind: "projection", title: STAGE_LABEL.projection,
    subtitle: projection.length ? projection[0] + (projection.length > 1 ? ` +${projection.length - 1}` : "") : "Nothing mapped yet",
    status: projection.length ? "ok" : "warn",
    badges: view.mcp?.remote ? ["metadata only"] : [],
    lines: [
      ...projection.map((p) => ({ label: "Field", value: p })),
      ...(preview?.payload ?? []).map((p) => ({ label: "Exact request", value: p })),
      ...(preview?.tool_calls ?? []).map((c) => ({ label: `${c.tool} · ${c.subject}`, value: JSON.stringify(c.arguments) })),
      { label: "Never", value: "Environment values, secrets other than its declared keys, or matched text" },
    ],
  }, 1);

  const http = view.mcp?.transport === "http";
  const hosts = view.permissions.network;
  const sandboxTitle = view.kind === "builtin" ? "Inside holdmap" : http ? (view.mcp?.remote ? "Another machine" : "Local HTTP server") : (SANDBOX_LABEL[view.sandbox] ?? view.sandbox);
  node({
    id: "sandbox", kind: "sandbox", title: STAGE_LABEL.sandbox,
    subtitle: sandboxTitle,
    status: http && view.mcp?.remote ? "warn" : view.sandbox === "basic" && view.kind !== "builtin" && !http ? "warn" : "ok",
    badges: [hosts.length ? `network: ${hosts.length === 1 ? hosts[0] : plural(hosts.length, "host")}` : "no network"],
    lines: [
      { label: "Containment", value: preview?.sandbox ?? sandboxTitle },
      { label: "Network", value: hosts.length ? hosts.join(", ") : "none declared" },
      ...(view.permissions.network_reason ? [{ label: "Why", value: view.permissions.network_reason }] : []),
      ...view.permissions.sends.map((s) => ({ label: "Sends", value: s })),
      ...(preview?.env.length ? [{ label: "Environment", value: `${preview.env.join(", ")} (set by holdmap)` }] : []),
    ],
  }, 2);

  const pin = pinState(view);
  const calls = result?.tool_calls ?? [];
  const tools = view.mcp?.tools ?? [];
  const consentStatus: NodeStatus = view.state === "approved" ? pin.status : view.state === "changed" ? "warn" : view.state === "not_installed" ? "error" : "idle";
  node({
    id: "analyzer", kind: "analyzer", title: view.name,
    subtitle: view.kind === "mcp" ? `MCP ${http ? "over HTTP" : "over stdio"} · ${tools.length ? plural(tools.length, "tool") : "no tools chosen"}` : view.kind === "cli" ? "Command-line program" : "Built into holdmap",
    status: consentStatus,
    badges: [STATE_LABEL[view.state], ...(pin.text ? [pin.text] : [])],
    lines: [
      { label: "Consent", value: view.state === "approved" && view.approved_at_ms ? `${STATE_LABEL[view.state]} since ${new Date(view.approved_at_ms).toLocaleDateString()}` : STATE_LABEL[view.state] },
      ...(view.changes.length ? [{ label: "Changed", value: view.changes.join("; ") }] : []),
      ...(view.mcp ? [{ label: "Endpoint", value: view.mcp.endpoint }] : preview?.command ? [{ label: "Command", value: preview.command.join(" ") }] : []),
      ...(view.binary ? [{ label: "Program", value: view.binary }] : []),
      ...(view.version ? [{ label: "Version", value: view.version }] : []),
      ...(pin.text ? [{ label: "Tool pin", value: pin.text }] : []),
      ...(view.mcp?.listing?.flagged ? [{ label: "Warnings", value: `${plural(view.mcp.listing.flagged, "tool")} with suspicious descriptions` }] : []),
      ...tools.map((t) => {
        const c = calls.find((x) => x.tool === t);
        return { label: `Tool ${t}`, value: c ? `${plural(c.calls, "call")}${c.failed ? `, ${c.failed} failed` : ""} last run` : "not called yet" };
      }),
    ],
  }, 3);

  const fmt = view.output_format ?? (view.kind === "builtin" ? "osv" : "auto");
  const excerpts = result?.details.filter((d) => d.label.startsWith("Output of ")).length ?? 0;
  node({
    id: "parser", kind: "parser", title: STAGE_LABEL.parser,
    subtitle: PARSER_LABEL[fmt]?.split(":")[0] ?? fmt,
    status: result ? (result.status === "failed" ? "error" : excerpts ? "warn" : "ok") : "idle",
    badges: excerpts ? [plural(excerpts, "excerpt")] : [],
    lines: [
      { label: "Reads", value: PARSER_LABEL[fmt] ?? fmt },
      { label: "Keeps", value: "Capped, normalised findings; never matched text" },
      ...(excerpts ? [{ label: "Unmapped output", value: `${plural(excerpts, "tool output")} kept as a short excerpt. Map its fields to get findings.` }] : []),
    ],
  }, 4);

  const open = openCounts(result);
  const nOpen = total(open);
  node({
    id: "findings", kind: "findings", title: STAGE_LABEL.findings,
    subtitle: !result ? "Not run yet" : nOpen ? `${nOpen} open · ${countsText(open)}` : "No open findings",
    status: !result ? "idle" : result.status === "failed" ? "error" : (worst(open) === "critical" || worst(open) === "high") ? "error" : nOpen || result.status === "partial" ? "warn" : "ok",
    badges: result ? [result.status, duration(result.duration_ms)] : [],
    counts: open,
    lines: result ? [
      { label: "Last run", value: new Date(result.run_at_ms).toLocaleString() },
      { label: "Status", value: result.status },
      { label: "Took", value: duration(result.duration_ms) },
      { label: "Items", value: String(result.subjects.length) },
      ...SEVERITY_ORDER.filter((s) => open[s]).map((s) => ({ label: s, value: String(open[s]) })),
      ...(result.findings.length - nOpen ? [{ label: "Reviewed or snoozed", value: String(result.findings.length - nOpen) }] : []),
      ...result.notes.map((n) => ({ label: "Note", value: n })),
    ] : [{ label: "Last run", value: "never" }],
  }, 5);

  // Where findings appear: agents with open findings (Home), medium or worse (Needs you), all (Insights).
  const byAgent = new Map<string, SeverityCounts>();
  const subjectsById = new Map((result?.subjects ?? []).map((s) => [s.id, s]));
  for (const f of result?.findings ?? []) {
    if (f.state !== "open") continue;
    for (const a of subjectsById.get(f.subject)?.agents ?? []) {
      const c = byAgent.get(a) ?? emptyCounts();
      c[f.severity]++;
      byAgent.set(a, c);
    }
  }
  const needs = [...byAgent.values()].filter((c) => c.critical + c.high + c.medium > 0).length;
  const surfaces: [string, string, string, number][] = [
    ["home", "Home", byAgent.size ? `${plural(byAgent.size, "agent card")} marked` : "No agent marked", byAgent.size],
    ["needs", "Needs you", needs ? `${plural(needs, "item")} (medium or worse)` : "Nothing needs you", needs],
    ["insights", "Security", nOpen ? `${plural(nOpen, "open finding")}` : "No open findings", nOpen],
  ];
  surfaces.forEach(([id, title, subtitle, n], i) => node({
    id: `surface:${id}`, kind: "surface", title, subtitle,
    status: !result ? "idle" : n ? (id === "needs" ? "warn" : "ok") : "idle",
    badges: [],
    lines: [{ label: title, value: subtitle }, ...[...byAgent.entries()].map(([a, c]) => ({ label: a, value: countsText(c, 5) }))],
  }, 6, i, 3));

  const ran = !!result;
  const edges: GraphEdge[] = [
    { id: "e:sources", from: "sources", to: "projection", label: subjects.length ? String(subjects.length) : null, active: subjects.length > 0, severity: null },
    { id: "e:projection", from: "projection", to: "sandbox", label: null, active: projection.length > 0, severity: null },
    { id: "e:sandbox", from: "sandbox", to: "analyzer", label: null, active: view.state === "approved", severity: null },
    { id: "e:analyzer", from: "analyzer", to: "parser", label: calls.length ? plural(calls.reduce((n, c) => n + c.calls, 0), "call") : null, active: ran, severity: null },
    { id: "e:parser", from: "parser", to: "findings", label: ran ? String(result!.findings.length) : null, active: ran, severity: null },
    ...surfaces.map(([id, , , n]) => ({ id: `e:surface:${id}`, from: "findings", to: `surface:${id}`, label: n ? String(n) : null, active: n > 0, severity: worst(open) })),
  ];
  return { nodes, edges };
}

/** Analyzers (left) × agents (right): an edge where the analyzer's last run covered the agent. */
export function buildOverview(views: AnalyzerView[], insights: Insights | null, report: AgentsReport | null): Graph {
  const results = new Map((insights?.results ?? []).map((r) => [r.analyzer, r]));
  const agentNames = new Map<string, string>();
  for (const i of inventories(report)) if (!agentNames.has(i.product)) agentNames.set(i.product, i.name);
  for (const r of results.values()) for (const s of r.subjects) for (const a of s.agents) if (!agentNames.has(a)) agentNames.set(a, productLabel(a));
  const products = [...agentNames.keys()].sort((a, b) => (agentNames.get(a) ?? a).localeCompare(agentNames.get(b) ?? b));
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const agentCounts = new Map(products.map((p) => [p, emptyCounts()]));
  const shown = views.filter((v) => v.state !== "not_installed" || results.has(v.id));
  shown.forEach((v, i) => {
    const r = results.get(v.id) ?? null;
    const open = openCounts(r);
    nodes.push({
      id: `analyzer:${v.id}`, kind: "analyzer", title: v.name,
      subtitle: r ? (total(open) ? `${total(open)} open · ${countsText(open)}` : "No open findings") : v.state === "approved" ? "Not run yet" : STATE_LABEL[v.state],
      status: v.state === "approved" ? (r?.status === "failed" ? "error" : total(open) ? "warn" : "ok") : v.state === "changed" ? "warn" : "idle",
      badges: [STATE_LABEL[v.state], v.kind === "mcp" ? "MCP" : v.kind === "cli" ? "CLI" : "built-in"],
      lines: [
        { label: "State", value: STATE_LABEL[v.state] },
        { label: "Kind", value: v.kind },
        ...(r ? [{ label: "Last run", value: new Date(r.run_at_ms).toLocaleString() }, { label: "Items", value: String(r.subjects.length) }] : []),
      ],
      counts: open,
      position: { x: 0, y: (i - (shown.length - 1) / 2) * ROW },
      width: NODE_W, height: NODE_H,
    });
    if (!r) return;
    for (const p of products) {
      const subjects = r.subjects.filter((s) => s.agents.includes(p));
      if (!subjects.length) continue;
      const ids = new Set(subjects.map((s) => s.id));
      const c = emptyCounts();
      for (const f of r.findings) if (f.state === "open" && ids.has(f.subject)) c[f.severity]++;
      const ac = agentCounts.get(p)!;
      for (const s of SEVERITY_ORDER) ac[s] += c[s];
      const kinds = subjects.some((s) => s.kind === "package") ? "item" : "skill";
      edges.push({
        id: `e:${v.id}:${p}`, from: `analyzer:${v.id}`, to: `agent:${p}`,
        label: `${plural(subjects.length, kinds)}${total(c) ? ` · ${countsText(c, 1)}` : ""}`,
        active: true, severity: worst(c),
      });
    }
  });
  products.forEach((p, i) => {
    const c = agentCounts.get(p)!;
    const covered = edges.filter((e) => e.to === `agent:${p}`).length;
    nodes.push({
      id: `agent:${p}`, kind: "agent", title: agentNames.get(p) ?? p,
      subtitle: covered ? `${plural(covered, "analyzer")} · ${total(c) ? countsText(c) : "no open findings"}` : "Not covered by an analyzer",
      status: !covered ? "idle" : (worst(c) === "critical" || worst(c) === "high") ? "error" : total(c) ? "warn" : "ok",
      badges: [],
      lines: [{ label: "Analyzers", value: edges.filter((e) => e.to === `agent:${p}`).map((e) => e.from.slice("analyzer:".length)).join(", ") || "none" }, ...SEVERITY_ORDER.filter((s) => c[s]).map((s) => ({ label: s, value: String(c[s]) }))],
      counts: c,
      position: { x: COL * 2, y: (i - (products.length - 1) / 2) * ROW },
      width: NODE_W, height: NODE_H,
    });
  });
  return { nodes, edges };
}

/** Plain-text reading order for screen readers and the narrow layout. */
export function readingOrder(g: Graph): GraphNode[] {
  return [...g.nodes].sort((a, b) => a.position.x - b.position.x || a.position.y - b.position.y);
}
