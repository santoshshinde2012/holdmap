// Runs analyzers one at a time and tracks their progress for the whole app: the run dock, the
// status bar and the analyzer cards all read this one instance, so progress survives dialogs
// and tab switches. Only approved analyzers can run (this class and Rust both check), and each
// run stays inside its sandbox. Progress updates carry names only.
import * as api from "./api";
import { insightsStore } from "./insights-store.svelte";
import { runProgress, runSummary, type RunStep } from "./run-view";
import type { AnalyzerResult, AnalyzerView } from "./analyzers";

type Runner = (id: string) => Promise<Pick<AnalyzerResult, "status"> & Partial<Pick<AnalyzerResult, "findings" | "subjects" | "notes">>>;

export class SecurityRuns {
  steps = $state<RunStep[]>([]);
  startedAt = $state<number | null>(null);
  finishedAt = $state<number | null>(null);
  /** The dock is collapsed to one line (it stays visible while a run is going). */
  collapsed = $state(false);
  progress = $derived(runProgress(this.steps));
  summary = $derived(runSummary(this.progress));
  private cancelRequested = false;
  private unlisten: (() => void) | null = null;

  constructor(
    private readonly run: Runner = api.runAnalyzer,
    private readonly refresh: () => Promise<void> = () => insightsStore.refresh(),
    private readonly cancelRun: () => Promise<void> = api.cancelAnalyzerRun,
    private readonly subscribe: typeof api.onAnalyzerProgress = api.onAnalyzerProgress,
    private readonly now: () => number = Date.now,
  ) {}

  get busy(): boolean { return this.progress.phase === "running"; }
  get visible(): boolean { return this.steps.length > 0; }
  get current(): RunStep | null { return this.steps.find((s) => s.phase === "running") ?? null; }

  /** The step for one analyzer, for its card. */
  stepFor(id: string): RunStep | null { return this.steps.find((s) => s.id === id) ?? null; }

  async start(analyzers: Pick<AnalyzerView, "id" | "name" | "state">[]): Promise<void> {
    const ok = analyzers.filter((a) => a.state === "approved");
    if (this.busy || !ok.length) return;
    this.cancelRequested = false;
    this.collapsed = false;
    this.startedAt = this.now();
    this.finishedAt = null;
    this.steps = ok.map((a) => ({ id: a.id, name: a.name, phase: "idle", note: null, event: null, startedAt: null, finishedAt: null, findings: null, subjects: null, detail: null }));
    await this.listen();
    try {
      for (let i = 0; i < this.steps.length; i++) {
        const step = this.steps[i];
        if (this.cancelRequested) { step.phase = "cancelled"; continue; }
        step.phase = "running";
        step.startedAt = this.now();
        try {
          const r = await this.run(step.id);
          step.phase = r.status === "failed" ? "failed" : "done";
          step.note = r.status === "complete" ? null : r.status;
          step.findings = r.findings?.length ?? null;
          step.subjects = r.subjects?.length ?? null;
          step.detail = step.phase === "done" && step.subjects === 0 ? r.notes?.[0] ?? null : null;
        } catch (e) {
          step.phase = this.cancelRequested ? "cancelled" : "failed";
          step.note = this.cancelRequested ? null : String(e instanceof Error ? e.message : e);
        }
        step.finishedAt = this.now();
        step.event = null;
      }
    } finally {
      this.finishedAt = this.now();
      this.unlisten?.();
      this.unlisten = null;
    }
    await this.refresh();
  }

  /** Stop the analyzer that's running and skip the queued ones. */
  async cancel(): Promise<void> {
    if (!this.busy) return;
    this.cancelRequested = true;
    for (const s of this.steps) if (s.phase === "idle") s.phase = "cancelled";
    await this.cancelRun();
  }

  clear(): void { if (!this.busy) { this.steps = []; this.startedAt = null; this.finishedAt = null; } }

  private async listen(): Promise<void> {
    this.unlisten?.();
    try {
      this.unlisten = await this.subscribe(({ id, event }) => {
        const s = this.steps.find((x) => x.id === id && x.phase === "running");
        if (s) s.event = event;
      });
    } catch {
      this.unlisten = null; // progress is a nicety; the run itself still reports when it ends
    }
  }
}

export const securityRuns = new SecurityRuns();
