import type { ShutdownPreview } from "./power";
import type { ScheduledAction } from "./schedules";

/** What the shutdown flow needs from the backend; injected so it can be tested without Tauri. */
export interface ShutdownPort {
  shutdownPreview(): Promise<ShutdownPreview>;
  shutdownMachine(confirmationId: string): Promise<{ requested: boolean }>;
  scheduleShutdown(confirmationId: string, delaySecs: number): Promise<ScheduledAction>;
}

export type ShutdownIntent = "now" | "schedule";
export type ShutdownSource = "settings" | "schedules";

/** State and steps of the "shut down this computer" review: preview, confirm or schedule. */
export class ShutdownFlow {
  visible = $state(false);
  intent = $state<ShutdownIntent>("now");
  source = $state<ShutdownSource>("settings");
  preview = $state.raw<ShutdownPreview | null>(null);
  loading = $state(false);
  busy = $state(false);
  requested = $state(false);
  error = $state<string | null>(null);
  scheduled = $state.raw<ScheduledAction | null>(null);
  #sequence = 0;
  #port: ShutdownPort;
  #onScheduled: () => Promise<unknown>;

  constructor(port: ShutdownPort, onScheduled: () => Promise<unknown> = async () => {}) {
    this.#port = port;
    this.#onScheduled = onScheduled;
  }

  get pending(): boolean {
    return this.busy || this.loading;
  }

  /** Open the review and fetch a fresh preview; a later open or close discards a stale one. */
  async open(intent: ShutdownIntent, source: ShutdownSource): Promise<void> {
    if (this.pending) return;
    Object.assign(this, { intent, source, visible: true, preview: null, error: null, requested: false, scheduled: null, loading: true });
    const sequence = ++this.#sequence;
    try {
      const preview = await this.#port.shutdownPreview();
      if (this.visible && sequence === this.#sequence) this.preview = preview;
    } catch (e) {
      if (this.visible && sequence === this.#sequence) this.error = String(e);
    } finally {
      if (sequence === this.#sequence) this.loading = false;
    }
  }

  /** Drop any in-flight preview without changing visibility. */
  discard(): void {
    this.#sequence++;
    this.loading = false;
    this.preview = null;
  }

  /** Close and return where the user came from, or null while a request is running. */
  close(): ShutdownSource | null {
    if (this.busy) return null;
    this.visible = false;
    this.discard();
    const back = this.source;
    this.source = "settings";
    return back;
  }

  #ready(intent: ShutdownIntent): this is { preview: ShutdownPreview } {
    return this.intent === intent && !!this.preview && !this.pending && !this.requested && !this.scheduled;
  }

  async confirm(): Promise<void> {
    if (!this.#ready("now")) return;
    await this.#run(async (id) => {
      const result = await this.#port.shutdownMachine(id);
      if (!result.requested) throw new Error("The operating system did not accept the shutdown request.");
      this.requested = true;
    });
  }

  async schedule(delaySecs: number): Promise<void> {
    if (!this.#ready("schedule")) return;
    await this.#run(async (id) => {
      this.scheduled = await this.#port.scheduleShutdown(id, delaySecs);
      await this.#onScheduled();
    });
  }

  async #run(step: (confirmationId: string) => Promise<void>): Promise<void> {
    this.busy = true;
    this.error = null;
    try {
      await step(this.preview!.confirmation_id);
    } catch (e) {
      this.error = String(e);
      this.preview = null; // Native handles are consumed on every attempt; re-review to retry.
    } finally {
      this.busy = false;
    }
  }
}
