// The tray popup panel's data (`PanelData` in src-tauri/src/tray/model.rs) and the pure helpers
// that turn it into what the panel draws. Rows carry the tray action id they run; the panel can
// call nothing else (`local_window::guard`).

export type Urgency = "high" | "medium";

export interface PanelNeed { urgency: Urgency; title: string; action: string }
export interface PanelDev { port: number; label: string; exposed: boolean; action: string }
export interface PanelAgent { name: string; place: string | null; cpu: number | null; memory: number; action: string }

export interface PanelData {
  /** False until the first scan. */
  ready: boolean;
  ports: number;
  exposed: number;
  /** Null until the agents report is read. */
  agents: number | null;
  /** The agents' CPU together (percent of one core), once measured between two scans. */
  cpu: number | null;
  memory: number | null;
  /** The agents' CPU per scan, oldest first. */
  cpuHistory: number[];
  /** The most urgent item per scan, oldest first: 0 none, 1 worth a look, 2 look now. */
  attentionHistory: number[];
  needs: PanelNeed[] | null;
  dev: PanelDev[];
  stopDev: boolean;
  /** The busiest agents first. */
  topAgents: PanelAgent[];
  moreAgents: number;
  style: "icon" | "count" | "trend";
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
export function chartPaths(samples: readonly number[], width: number, height: number, slots = HISTORY_CELLS): { area: string; line: string } {
  if (samples.length < 2) return { area: "", line: "" };
  const top = chartTop(samples);
  const step = width / (slots - 1);
  const x0 = width - step * (samples.length - 1);
  const points = samples.map((v, i) => [x0 + i * step, height - (Math.max(0, v) / top) * height] as const);
  const fmt = (n: number) => Number(n.toFixed(2));
  const line = points.map(([x, y], i) => `${i ? "L" : "M"}${fmt(x)},${fmt(y)}`).join("");
  const area = `${line}L${fmt(points[points.length - 1][0])},${height}L${fmt(x0)},${height}Z`;
  return { area, line };
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
    memory: 2_684_354_560,
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
      { name: "Claude Code", place: "storefront", cpu: 4.2, memory: 301_989_888, action: "agent:agent:4242" },
      { name: "Cursor", place: null, cpu: 2.1, memory: 1_181_116_006, action: "agent:agent:811" },
      { name: "Codex CLI", place: "api", cpu: 0.9, memory: 189_792_256, action: "agent:agent:977" },
      { name: "Gemini CLI", place: "dashboard", cpu: 0.2, memory: 98_566_144, action: "agent:agent:1203" },
    ],
    moreAgents: 0,
    style: "count",
  };
}
