// Runs due scheduled scans while the window is open: every minute it asks the backend whether
// a scan is due, runs it through the shared run dock, then reports back so the backend can
// re-fingerprint skills and decide on one throttled notification. No daemon: closing holdmap
// stops it. Everything is injected so tests drive it with a fake clock.
import * as api from "./api";
import type { AnalyzerView } from "./analyzers";
import type { Due, ScheduleStatus } from "./schedule";

export const TICK_MS = 60_000;

export interface DriverDeps {
  due: () => Promise<Due>;
  finished: (startedMs: number) => Promise<unknown>;
  status: () => Promise<ScheduleStatus>;
  analyzers: () => Pick<AnalyzerView, "id" | "name" | "state">[];
  run: (list: Pick<AnalyzerView, "id" | "name" | "state">[]) => Promise<void>;
  busy: () => boolean;
  now: () => number;
  every: (fn: () => void, ms: number) => () => void;
}

export class ScheduleDriver {
  status = $state<ScheduleStatus | null>(null);
  /** Why the last due scan was skipped (battery, nothing eligible), if it was. */
  skipped = $state<string | null>(null);
  lastRunAt = $state<number | null>(null);
  private stop: (() => void) | null = null;
  private ticking = false;

  constructor(private readonly deps: DriverDeps) {}

  get nextMs(): number | null { return this.status?.next_ms ?? null; }
  get enabled(): boolean { return !!this.status?.schedule.enabled; }

  start(): void {
    if (this.stop) return;
    void this.refresh();
    this.stop = this.deps.every(() => void this.tick(), TICK_MS);
  }
  dispose(): void { this.stop?.(); this.stop = null; }

  async refresh(): Promise<void> {
    try { this.status = await this.deps.status(); } catch { /* keep the last known status */ }
  }

  /** One check. Never overlaps itself or a run the user started. */
  async tick(): Promise<void> {
    if (this.ticking || this.deps.busy()) return;
    this.ticking = true;
    try {
      const d = await this.deps.due();
      if (d.kind === "skip") { this.skipped = d.reason; return; }
      this.skipped = null;
      if (d.kind !== "run") return;
      const list = this.deps.analyzers().filter((a) => d.analyzers.includes(a.id) && a.state === "approved");
      const started = this.deps.now();
      if (list.length) await this.deps.run(list);
      await this.deps.finished(started);
      this.lastRunAt = started;
      await this.refresh();
    } catch (e) {
      console.error("Scheduled scan check failed", e);
    } finally {
      this.ticking = false;
    }
  }
}

let shared: ScheduleDriver | null = null;
/** The app's one driver, wired to the real API and run dock. */
export async function scheduleDriver(): Promise<ScheduleDriver> {
  if (shared) return shared;
  const [{ securityRuns }, { insightsStore }] = await Promise.all([import("./security-runs.svelte"), import("./insights-store.svelte")]);
  shared = new ScheduleDriver({
    due: api.scheduleDue,
    finished: api.scheduleFinished,
    status: api.scheduleStatus,
    analyzers: () => insightsStore.analyzers,
    run: (list) => securityRuns.start(list),
    busy: () => securityRuns.busy,
    now: Date.now,
    every: (fn, ms) => { const t = setInterval(fn, ms); return () => clearInterval(t); },
  });
  return shared;
}
