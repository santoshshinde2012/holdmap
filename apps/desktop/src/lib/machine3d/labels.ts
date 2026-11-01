// Which 3D labels to show: greedy by priority, never two overlapping, never one off screen.
// Pure over projected screen points, so the renderer stays a thin projection step.
import type { Part } from "./scene";

export interface LabelCandidate {
  id: string;
  /** Screen anchor in CSS pixels: the label sits centred above it. */
  x: number;
  y: number;
  /** Estimated box size in CSS pixels. */
  w: number;
  h: number;
  priority: number;
  /** Behind the camera or clipped by the near plane. */
  behind?: boolean;
  /** Fallback vertical offsets (CSS pixels) to try when the first place is taken: a nameplate
   *  steps down a line rather than disappear. */
  alts?: readonly number[];
}

export const LABEL_CHAR = { label: 6.9, sub: 6 } as const;
/** Box estimate for a two-line label, close to the rendered 12 px / 11 px text plus padding. */
export function labelSize(label: string, sub: string | null): { w: number; h: number } {
  return { w: Math.max(label.length * LABEL_CHAR.label, (sub?.length ?? 0) * LABEL_CHAR.sub) + 14, h: sub ? 34 : 20 };
}

/** Screen area free for labels: the canvas less the overlays along its edges. */
export interface LabelArea { width: number; height: number; top?: number; bottom?: number; right?: number }

export function pickLabels(cands: readonly LabelCandidate[], area: LabelArea, gap = 4, edge = 6): Set<string> {
  return new Set(placeLabels(cands, area, gap, edge).keys());
}

/** The labels to show and the vertical offset each one took (0, or one of its fallbacks). */
export function placeLabels(cands: readonly LabelCandidate[], { width, height, top = 0, bottom = 0, right = 0 }: LabelArea, gap = 4, edge = 6): Map<string, number> {
  const shown: { x: number; y: number; w: number; h: number }[] = [];
  const out = new Map<string, number>();
  // Stable order: priority, then nearer the bottom of the screen (usually nearer the camera), then x, then id.
  const sorted = [...cands].sort((a, b) => b.priority - a.priority || b.y - a.y || a.x - b.x || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  for (const c of sorted) {
    if (c.behind) continue;
    for (const dy of [0, ...(c.alts ?? [])]) {
      const box = { x: c.x - c.w / 2, y: c.y + dy - c.h, w: c.w, h: c.h };
      if (box.x < edge || box.y < top + edge || box.x + box.w > width - right - edge || box.y + box.h > height - bottom - edge) continue;
      if (shown.some((s) => box.x < s.x + s.w + gap && s.x < box.x + box.w + gap && box.y < s.y + s.h + gap && s.y < box.y + box.h + gap)) continue;
      shown.push(box);
      out.set(c.id, dy);
      break;
    }
  }
  return out;
}

export interface LabelState {
  focus: string | null;
  selected: string | null;
  hovered: string | null;
  hiddenLayers: ReadonlySet<string>;
  /** Parts a clicked reading lights up; the rest stay quiet. */
  highlight?: ReadonlySet<string> | null;
}

/** How much a part's label matters right now; null hides it. Agents always try; a focused agent shows its parts. */
export function labelPriority(p: Part, state: LabelState): number | null {
  if (state.hiddenLayers.has(p.layer) && p.kind !== "agent" && p.kind !== "dormant") return null;
  if (p.id === state.selected) return 100;
  if (p.id === state.hovered) return 95;
  if (state.highlight) return state.highlight.has(p.id) ? 75 : p.kind === "agent" || p.kind === "dormant" ? 40 : null;
  // While one agent is in focus the others fade, and so do their names.
  if (p.kind === "agent" || p.kind === "dormant") return state.focus && state.focus !== p.id ? null : 90;
  const mine = state.focus === p.owner;
  if (state.focus && !mine) return null;
  if (p.warn) return mine ? 80 : 60;
  if (mine) return p.kind === "more" ? 55 : 70;
  // Overview: the parts that tell the story without crowding it.
  if (p.kind === "call") return null;
  if (p.kind === "mcp" && p.live) return 45;
  if (p.kind === "remote" || p.kind === "port") return 30;
  return null;
}
