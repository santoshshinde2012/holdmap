// The one install dialog for the app. Any "Install…" button opens it; it lives at the app root
// so an install keeps going (and stays visible) when other views or dialogs change.
import * as api from "./api";
import { insightsStore } from "./insights-store.svelte";
import { beginInstall, initialInstall, reduceInstall, type InstallEvent, type InstallPlan, type InstallRecord, type InstallState } from "./analyzer-install";

export class InstallDialog {
  id = $state<string | null>(null);
  plan = $state<InstallPlan | null>(null);
  loadError = $state<string | null>(null);
  state = $state<InstallState>(initialInstall());
  /** The user ticked "I understand…" (consent is per attempt, never remembered). */
  agreed = $state(false);
  /** Bumped after every attempt ends, so lists of presets can refetch. */
  attempts = $state(0);

  constructor(
    private readonly loadPlan: (id: string) => Promise<InstallPlan> = (id) => api.analyzerInstallPlan(id),
    private readonly runInstall: (id: string, on: (e: InstallEvent) => void) => Promise<InstallRecord> = (id, on) => api.installAnalyzer(id, on),
    private readonly cancelInstall: () => Promise<void> = () => api.cancelAnalyzerInstall(),
    private readonly refresh: () => Promise<void> = () => insightsStore.refresh(),
  ) {}

  get open(): boolean { return this.id !== null; }
  get busy(): boolean { return this.state.phase === "installing" || this.state.phase === "verifying"; }

  async show(id: string): Promise<void> {
    if (this.busy) return; // one install at a time; the open dialog shows it
    this.id = id;
    this.plan = null;
    this.loadError = null;
    this.agreed = false;
    this.state = initialInstall();
    try { this.plan = await this.loadPlan(id); }
    catch (e) { this.loadError = e instanceof Error ? e.message : String(e); }
  }

  /** Runs only after consent; Retry calls this again. */
  async install(): Promise<void> {
    const id = this.id;
    if (!id || !this.plan?.can_install || !this.agreed || this.busy) return;
    this.state = beginInstall();
    try {
      await this.runInstall(id, (e) => { if (this.id === id) this.state = reduceInstall(this.state, e); });
      // The command's own result is final even if an event went missing.
      if (this.state.phase !== "done" && this.id === id) this.state = { ...this.state, phase: "done" };
      this.plan = await this.loadPlan(id).catch(() => this.plan);
    } catch (e) {
      if (this.id === id && this.state.phase !== "cancelled" && this.state.phase !== "failed") {
        this.state = { ...this.state, phase: "failed", reason: e instanceof Error ? e.message : String(e) };
      }
    }
    await this.refresh().catch(() => {});
    this.attempts++;
  }

  async cancel(): Promise<void> {
    if (this.busy) await this.cancelInstall();
  }

  close(): void {
    if (this.busy) return;
    this.id = null;
    this.plan = null;
  }
}

export const installDialog = new InstallDialog();
