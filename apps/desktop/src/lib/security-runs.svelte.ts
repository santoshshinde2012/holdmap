// Runs analyzers one at a time and tracks progress for the Security tab. Only approved analyzers
// can run (the runner and Rust both check), and each run stays inside its sandbox.
import * as api from "./api";
import { insightsStore } from "./insights-store.svelte";
import { runProgress, type RunStep } from "./security-view";
import type { AnalyzerView } from "./analyzers";

export class SecurityRuns {
  steps = $state<RunStep[]>([]);
  progress = $derived(runProgress(this.steps));
  constructor(private readonly run: (id: string) => Promise<{ status: string }> = api.runAnalyzer, private readonly refresh: () => Promise<void> = () => insightsStore.refresh()) {}

  get busy(): boolean { return this.progress.phase === "running"; }

  async start(analyzers: Pick<AnalyzerView, "id" | "name" | "state">[]): Promise<void> {
    const ok = analyzers.filter((a) => a.state === "approved");
    if (this.busy || !ok.length) return;
    this.steps = ok.map((a) => ({ id: a.id, name: a.name, phase: "idle", note: null }));
    for (let i = 0; i < this.steps.length; i++) {
      this.steps[i].phase = "running";
      try {
        const r = await this.run(this.steps[i].id);
        this.steps[i].phase = r.status === "failed" ? "failed" : "done";
        this.steps[i].note = r.status === "complete" ? null : r.status;
      } catch (e) {
        this.steps[i].phase = "failed";
        this.steps[i].note = String(e);
      }
    }
    await this.refresh();
  }

  clear(): void { if (!this.busy) this.steps = []; }
}

export const securityRuns = new SecurityRuns();
