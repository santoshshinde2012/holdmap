// Small run-progress and scope helpers, kept apart so the status bar and agent details
// don't pull the whole Security view into the first screen.
import type { RunEvent } from "./analyzers";

/** Scope in plain words; "unknown" when holdmap couldn't tell. */
export function scopeLabel(scope: string | null | undefined): string {
  switch (scope) {
    case "user":
    case "global": return "Global — every project on this machine";
    case "project": return "Project — this repository only";
    case "local": return "Local — this repository, not committed";
    default: return "Unknown — holdmap couldn't confirm the scope";
  }
}

// ---- Runs ----

export type RunPhase = "idle" | "running" | "done" | "failed" | "cancelled";
export interface RunStep {
  id: string;
  name: string;
  /** "idle" means queued. */
  phase: RunPhase;
  note: string | null;
  /** The latest live update from the run (names only). */
  event?: RunEvent | null;
  startedAt?: number | null;
  finishedAt?: number | null;
  /** Findings and subjects the finished run reported. */
  findings?: number | null;
  subjects?: number | null;
}
export interface RunProgress { steps: RunStep[]; done: number; total: number; phase: RunPhase }

const finished = (p: RunPhase) => p === "done" || p === "failed" || p === "cancelled";

export function runProgress(steps: RunStep[]): RunProgress {
  const done = steps.filter((s) => finished(s.phase)).length;
  const phase: RunPhase = !steps.length ? "idle"
    : steps.some((s) => s.phase === "running" || s.phase === "idle") ? "running"
    : steps.some((s) => s.phase === "failed") ? "failed"
    : steps.some((s) => s.phase === "cancelled") ? "cancelled" : "done";
  return { steps, done, total: steps.length, phase };
}

/** What one analyzer is doing, in a few words: "Scanning release-check (2 of 4)". */
export function stepLabel(s: RunStep): string {
  switch (s.phase) {
    case "idle": return "Queued";
    case "cancelled": return "Cancelled";
    case "failed": return s.note ? `Failed: ${s.note}` : "Failed";
    case "done": {
      const found = s.findings == null ? "" : s.findings === 1 ? " · 1 finding" : ` · ${s.findings} findings`;
      return `${s.note ? `Done (${s.note})` : "Done"}${found}`;
    }
    case "running": {
      const e = s.event;
      if (!e) return "Starting";
      const of = e.total > 0 && e.index > 0 ? ` (${e.index} of ${e.total})` : "";
      const what = e.subject ?? (e.total ? `${e.total} ${e.total === 1 ? "item" : "items"}` : "");
      switch (e.step) {
        case "preparing": return "Preparing: checking approval and the program";
        case "copying": return `Preparing a private copy of ${what}${of}`;
        case "scanning": return `Scanning ${what}${of}`.trim();
        case "collecting": return "Collecting results";
      }
    }
  }
}

/** Share of the run done, 0..1, counting per-item steps when the analyzer reports them. */
export function runFraction(p: RunProgress): number {
  if (!p.total) return 0;
  let sum = 0;
  for (const s of p.steps) {
    if (finished(s.phase)) sum += 1;
    else if (s.phase === "running" && s.event) {
      const e = s.event;
      const inner = e.step === "preparing" ? 0.05 : e.step === "collecting" ? 0.95 : e.total ? (e.index - (e.step === "copying" ? 0.5 : 0.25)) / e.total : 0.5;
      sum += Math.max(0, Math.min(0.95, inner));
    }
  }
  return sum / p.total;
}

/** "1:05" or "12s". */
export function elapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** One-line wrap-up once every analyzer finished. */
export function runSummary(p: RunProgress): string {
  const ok = p.steps.filter((s) => s.phase === "done");
  const failed = p.steps.filter((s) => s.phase === "failed").length;
  const cancelled = p.steps.filter((s) => s.phase === "cancelled").length;
  const findings = ok.reduce((n, s) => n + (s.findings ?? 0), 0);
  const parts = [`${ok.length} of ${p.total} ${p.total === 1 ? "analyzer" : "analyzers"} finished`, `${findings} ${findings === 1 ? "finding" : "findings"}`];
  if (failed) parts.push(`${failed} failed`);
  if (cancelled) parts.push(`${cancelled} cancelled`);
  return parts.join(" · ");
}
