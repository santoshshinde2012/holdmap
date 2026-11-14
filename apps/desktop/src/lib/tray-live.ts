// Dev only: the tray panel page (tray.html) under `npm run dev` reads this machine through the
// Vite live collector, the same CLI routes the app's browser preview uses, so the panel can be
// checked with real agents, long names and an unreachable scanner. The desktop app builds
// PanelData in Rust (src-tauri/src/tray/model.rs); this mirrors its rows, not its history.
import { liveJson } from "./browser-live";
import { agentPlace } from "./agents";
import { attention } from "./home";
import type { PanelAgent, PanelData } from "./tray-panel";
import type { AgentsReport, PortEntry, Snapshot } from "./types";

const TOP = 5;

const devLabel = (e: PortEntry) => e.framework && e.project ? `${e.framework.name} · ${e.project.name}` : e.framework?.name ?? e.project?.name ?? e.label;

/** Panel data from one scan and agents report, keeping `prev`'s settings and CPU history. */
export function panelFrom(snapshot: Snapshot, report: AgentsReport, prev: PanelData): PanelData {
  const rows = report.agents.map((a): PanelAgent & { tool: boolean } => ({
    name: a.name, place: agentPlace(a, report.agents), cpu: a.cpu_unknown ? null : a.cpu_percent, memory: a.memory_bytes, action: `agent:${a.id}`, tool: a.kind === "tool",
  })).sort((a, b) => b.memory - a.memory || a.name.localeCompare(b.name));
  const agents = rows.filter((r) => !r.tool);
  const strip = ({ tool: _tool, ...r }: PanelAgent & { tool: boolean }): PanelAgent => r;
  const needs = attention(snapshot.entries, report).map((n, i) => ({ urgency: n.severity, title: n.title, action: `need:${i}` }));
  const cpu = agents.reduce((n, a) => n + (a.cpu ?? 0), 0);
  const dev = snapshot.entries.filter((e) => e.is_dev && e.is_mine);
  return {
    ...prev,
    ready: true,
    ports: snapshot.entries.length,
    exposed: snapshot.entries.filter((e) => e.exposure !== "loopback").length,
    agents: agents.length,
    cpu,
    memory: agents.reduce((n, a) => n + a.memory, 0),
    cpuHistory: [...prev.cpuHistory, Number(cpu.toFixed(1))].slice(-48),
    attentionHistory: [...prev.attentionHistory, needs.some((n) => n.urgency === "high") ? 2 : needs.length ? 1 : 0].slice(-48),
    needs,
    dev: dev.map((e) => ({ port: e.port, label: devLabel(e), exposed: e.exposure !== "loopback", action: `focus:${e.port}` })),
    stopDev: dev.some((e) => !e.protected),
    topAgents: agents.slice(0, TOP).map(strip),
    moreAgents: Math.max(0, agents.length - TOP),
    tools: rows.filter((r) => r.tool).map(strip),
  };
}

/** A loader for the panel: each call reads the live collector once. */
export function liveTrayLoader(initial: PanelData): () => Promise<PanelData> {
  let prev: PanelData = { ...initial, cpuHistory: [], attentionHistory: [], churn: [] };
  return async () => {
    const [snapshot, report] = await Promise.all([liveJson<Snapshot>("scan"), liveJson<AgentsReport>("agents")]);
    prev = panelFrom(snapshot, report, prev);
    return prev;
  };
}
