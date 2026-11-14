// The tray popup panel's data (`PanelData` in src-tauri/src/tray/model.rs) and the pure helpers
// that turn it into what the panel draws. Rows carry the tray action id they run; the panel can
// call nothing else (`local_window::guard`).
import { humanBytes } from "./format";

export type Urgency = "high" | "medium";

export interface PanelNeed { urgency: Urgency; title: string; action: string }
export interface PanelDev { port: number; label: string; exposed: boolean; action: string }
export interface PanelAgent { name: string; place: string | null; cpu: number | null; memory: number; action: string }
export interface PanelToggle { name: string; on: boolean; action: string }

/** The quick settings (`PanelSettings`): each switch carries the action id that flips it. */
export interface PanelSettings {
  /** What holds the readings: "Menu bar" (macOS) or "Panel" (Linux); null on Windows. */
  bar: string | null;
  items: PanelToggle[];
  /** The items as one menu-bar item; null where there's no such choice. */
  combined: boolean | null;
  /** The compact menu bar (the icon and at most two readings); null off macOS. */
  compact: boolean | null;
  /** holdmap's icon in the Dock; null off macOS. */
  dock: boolean | null;
  notify: boolean;
}

export interface PanelData {
  /** False until the first scan. */
  ready: boolean;
  ports: number;
  exposed: number;
  /** AI agents, null until the agents report is read. Developer tools are not counted. */
  agents: number | null;
  /** The agents' CPU together (percent of one core), once measured between two scans. */
  cpu: number | null;
  /** The AI agents' RAM together, as the Agents view counts it. */
  memory: number | null;
  /** The agents' CPU per scan, oldest first. */
  cpuHistory: number[];
  /** The most urgent item per scan, oldest first: 0 none, 1 worth a look, 2 look now. */
  attentionHistory: number[];
  needs: PanelNeed[] | null;
  dev: PanelDev[];
  stopDev: boolean;
  /** The AI agents using the most RAM first (a steady order). */
  topAgents: PanelAgent[];
  moreAgents: number;
  /** Developer tools running beside the agents (Docker Desktop): listed apart, not counted. */
  tools: PanelAgent[];
  /** Listeners opened and closed per scan, oldest first: [opened, closed]. */
  churn: [number, number][];
  /** The section to bring into view (the menu-bar item clicked), changing `focusSeq` per click. */
  focus: string | null;
  focusSeq: number;
  /** The panel stays open when it loses focus. */
  pinned: boolean;
  settings: PanelSettings;
}

/** Each quick setting's state by the action id that flips it. */
export function settingValues(s: PanelSettings): Record<string, boolean> {
  const values: Record<string, boolean> = Object.fromEntries(s.items.map((i) => [i.action, i.on]));
  if (s.combined !== null) values.combined = s.combined;
  if (s.compact !== null) values.compact = s.compact;
  if (s.dock !== null) values.dock = s.dock;
  values.notify = s.notify;
  return values;
}

export type StatusTone = "pending" | "ok" | "medium" | "high";

/** The status line at the top: how much needs you, and the most urgent item. */
export function needsStatus(needs: readonly PanelNeed[] | null): { tone: StatusTone; title: string; detail: string } {
  if (!needs) return { tone: "pending", title: "Checking…", detail: "Reading agents and ports" };
  if (!needs.length) return { tone: "ok", title: "All clear", detail: "Nothing needs you right now" };
  const first = needs.find((n) => n.urgency === "high") ?? needs[0];
  return { tone: first.urgency, title: `${needs.length} need${needs.length === 1 ? "s" : ""} you`, detail: first.title };
}

/** A byte count split into its number and unit, so the unit can be set smaller ("2.5", "GB"). */
export function bytesParts(bytes: number | null): { value: string; unit: string } {
  if (bytes === null) return { value: "…", unit: "" };
  const [value, unit = ""] = humanBytes(bytes).split(" ");
  return { value, unit };
}

/** A row's name with what tells it apart: "Cursor · typeglass". */
export function agentLabel(a: Pick<PanelAgent, "name" | "place">): string {
  return a.place ? `${a.name} · ${a.place}` : a.name;
}

/** The full row, for its tooltip and screen readers: "Cursor · typeglass: 2.4 GB RAM, CPU 3.1%". */
export function agentSummary(a: PanelAgent): string {
  const cpu = a.cpu === null ? "CPU not measured yet" : `CPU ${cpuText(a.cpu)}`;
  return `${agentLabel(a)}: ${humanBytes(a.memory)} RAM, ${cpu}`;
}

/** A small CPU sparkline for the headline tile: one polyline across `width` × `height`. */
export function sparkPath(samples: readonly number[], width: number, height: number, slots = 30): string {
  const recent = samples.slice(-slots);
  if (recent.length < 2) return "";
  const top = chartTop(recent);
  const step = width / (slots - 1);
  const x0 = width - step * (recent.length - 1);
  const fmt = (n: number) => Number(n.toFixed(2));
  return recent.map((v, i) => `${i ? "L" : "M"}${fmt(x0 + i * step)},${fmt(height - (Math.max(0, v) / top) * (height - 1) - 0.5)}`).join("");
}

/** The ring's two arcs, as stroke dash arrays on a circle of `circumference`. */
export function ringArcs(local: number, network: number, circumference: number): { local: string; network: string } {
  const total = Math.max(local + network, 1);
  const len = (n: number) => Number(((n / total) * circumference).toFixed(2));
  return { local: `${len(local)} ${circumference}`, network: `${len(network)} ${circumference}` };
}

/** Bars for ports opened (up) and closed (down) per scan, right-aligned like the CPU chart. */
export function churnBars(churn: readonly [number, number][], width: number, half: number, slots = HISTORY_CELLS): { x: number; w: number; up: number; down: number }[] {
  const recent = churn.slice(-slots);
  const top = Math.max(1, ...recent.flat());
  const step = width / slots;
  const x0 = width - step * recent.length;
  return recent.map(([opened, closed], i) => ({
    x: Number((x0 + i * step).toFixed(2)),
    w: Number(Math.max(1, step - 1).toFixed(2)),
    up: Number(((opened / top) * half).toFixed(2)),
    down: Number(((closed / top) * half).toFixed(2)),
  }));
}

/** What the port changes chart says, for people who can't see it. */
export function churnSummary(churn: readonly [number, number][]): string {
  if (!churn.length) return "Port changes appear after a few scans.";
  const opened = churn.reduce((n, [o]) => n + o, 0);
  const closed = churn.reduce((n, [, c]) => n + c, 0);
  const scans = `${churn.length} scan${churn.length === 1 ? "" : "s"}`;
  return `In the last ${scans}: ${opened} port${opened === 1 ? "" : "s"} opened, ${closed} closed.`;
}

/** Cells in the attention history grid: one per scan the panel keeps. */
export const HISTORY_CELLS = 60;
/** A chart's full height is at least this much CPU, so idle stays low. */
export const CPU_FLOOR = 10;

/** "7.4%" under 10, "12%" from 10. */
export function cpuText(cpu: number): string {
  const v = Math.max(0, cpu);
  return v < 9.95 ? `${v.toFixed(1)}%` : `${Math.round(v)}%`;
}

/** The headline number and its unit, split so the unit can be set smaller. */
export function cpuParts(cpu: number | null): { value: string; unit: string } {
  if (cpu === null) return { value: "…", unit: "" };
  const text = cpuText(cpu);
  return { value: text.slice(0, -1), unit: "%" };
}

/** The chart's top: the peak, at least the floor, rounded up to a readable step. */
export function chartTop(samples: readonly number[]): number {
  const peak = Math.max(CPU_FLOOR, ...samples.map((v) => Math.max(0, v)));
  const step = peak <= 20 ? 5 : peak <= 100 ? 10 : 50;
  return Math.ceil(peak / step) * step;
}

/**
 * SVG paths for the CPU history: the filled area and its top line, right-aligned so the newest
 * scan sits at the right edge and the chart fills in from the right as history grows.
 */
export function chartPaths(samples: readonly number[], width: number, height: number, slots = HISTORY_CELLS): { area: string; line: string; end: { x: number; y: number } | null } {
  if (samples.length < 2) return { area: "", line: "", end: null };
  const top = chartTop(samples);
  const step = width / (slots - 1);
  const x0 = width - step * (samples.length - 1);
  const points = samples.map((v, i) => [x0 + i * step, height - (Math.max(0, v) / top) * height] as const);
  const fmt = (n: number) => Number(n.toFixed(2));
  const line = points.map(([x, y], i) => `${i ? "L" : "M"}${fmt(x)},${fmt(y)}`).join("");
  const [ex, ey] = points[points.length - 1];
  const area = `${line}L${fmt(ex)},${height}L${fmt(x0)},${height}Z`;
  // The newest scan, as fractions of the chart, for the marker drawn over it.
  return { area, line, end: { x: fmt(ex / width), y: fmt(ey / height) } };
}

export type CellTone = "none" | "ok" | "medium" | "high";

/** The attention grid: the oldest scans first, empty cells where there's no history yet. */
export function attentionCells(history: readonly number[], cells = HISTORY_CELLS): CellTone[] {
  const recent = history.slice(-cells);
  const tones = recent.map((level): CellTone => (level >= 2 ? "high" : level === 1 ? "medium" : "ok"));
  return [...Array<CellTone>(cells - tones.length).fill("none"), ...tones];
}

/** What the attention grid says, for people who can't see it. */
export function attentionSummary(history: readonly number[]): string {
  if (!history.length) return "No scans yet.";
  const flagged = history.filter((level) => level > 0).length;
  const scans = `${history.length} scan${history.length === 1 ? "" : "s"}`;
  if (!flagged) return `Nothing needed you in the last ${scans}.`;
  return `Something needed you in ${flagged} of the last ${scans}.`;
}

/** What the CPU chart says, for people who can't see it. */
export function cpuSummary(samples: readonly number[]): string {
  if (samples.length < 2) return "Agent CPU appears after a few scans.";
  const peak = Math.max(...samples);
  return `Agent CPU over the last ${samples.length} scans: now ${cpuText(samples[samples.length - 1])}, peak ${cpuText(peak)}.`;
}

export const URGENCY_LABEL: Record<Urgency, string> = { high: "Look now", medium: "Worth a look" };

/** Sample data for the browser preview, screenshots and tests. */
export function demoPanel(): PanelData {
  const wave = (i: number) => 4 + 3 * Math.sin(i / 4) + (i % 9 === 0 ? 9 : 0) + (i % 13 === 5 ? 5 : 0);
  return {
    ready: true,
    ports: 37,
    exposed: 4,
    agents: 4,
    cpu: 7.4,
    memory: 1_771_464_294,
    cpuHistory: Array.from({ length: 48 }, (_, i) => Number(wave(i).toFixed(1))),
    attentionHistory: Array.from({ length: 48 }, (_, i) => (i > 30 && i < 36 ? 1 : i === 41 ? 2 : i > 41 ? 1 : 0)),
    needs: [
      { urgency: "high", title: "Claude Code listens on all interfaces", action: "need:0:agent:agent:4242" },
      { urgency: "medium", title: ":5173 is open to the network", action: "need:1:entry:tcp:5173" },
    ],
    dev: [
      { port: 3000, label: "Next.js · storefront", exposed: false, action: "focus:3000" },
      { port: 5173, label: "Vite · dashboard", exposed: true, action: "focus:5173" },
      { port: 8000, label: "FastAPI · api", exposed: false, action: "focus:8000" },
    ],
    stopDev: true,
    topAgents: [
      { name: "Cursor", place: "storefront-monorepo-with-a-long-name", cpu: 2.1, memory: 1_181_116_006, action: "agent:agent:811" },
      { name: "Claude Code", place: "storefront", cpu: 4.2, memory: 301_989_888, action: "agent:agent:4242" },
      { name: "Codex CLI", place: "api", cpu: 0.9, memory: 189_792_256, action: "agent:agent:977" },
      { name: "Gemini CLI", place: "dashboard", cpu: 0.2, memory: 98_566_144, action: "agent:agent:1203" },
    ],
    moreAgents: 0,
    tools: [{ name: "Docker Desktop", place: null, cpu: 1.4, memory: 575_668_224, action: "agent:agent:3301" }],
    churn: Array.from({ length: 48 }, (_, i): [number, number] => [i % 11 === 3 ? 2 : i % 7 === 0 ? 1 : 0, i % 13 === 6 ? 1 : i % 17 === 9 ? 2 : 0]),
    focus: null,
    focusSeq: 0,
    pinned: false,
    settings: {
      bar: "Menu bar",
      items: [
        ["holdmap icon", "icon", false], ["Agents", "agents", true], ["Alerts", "needs", true], ["Agent CPU", "cpu", false],
        ["Agent RAM", "ram", false], ["Dev servers", "dev", false], ["CPU history", "cpu_history", false], ["Agent bars", "agent_bars", false],
      ].map(([name, id, on]) => ({ name: name as string, on: on as boolean, action: `item:${id}` })),
      combined: false,
      compact: true,
      dock: true,
      notify: true,
    },
  };
}

/** The browser preview's `tray_action`: the panel's own controls change the sample data. */
export function demoAction(data: PanelData, id: string): PanelData {
  const s = data.settings;
  if (id === "pin-panel") return { ...data, pinned: !data.pinned };
  if (id === "notify") return { ...data, settings: { ...s, notify: !s.notify } };
  if (id === "dock" && s.dock !== null) return { ...data, settings: { ...s, dock: !s.dock } };
  if (id === "combined" && s.combined !== null) return { ...data, settings: { ...s, combined: !s.combined } };
  if (id === "compact" && s.compact !== null) return { ...data, settings: { ...s, compact: !s.compact } };
  if (id.startsWith("item:")) return { ...data, settings: { ...s, items: s.items.map((i) => (i.action === id ? { ...i, on: !i.on } : i)) } };
  return data;
}
