// Screen recording session state for the app shell: the confirm → recording → encoding → ready
// lifecycle, stale-request guards and focus return. Kept out of App.svelte so the shell only
// renders it; the recording backends themselves live in screen-record-backend.ts.
import { tick } from "svelte";
import { displayCaptureError, type CaptureResult, type RecordingLimit } from "./screen-record";
import { DEFAULT_PLAN, clampPlan, type RecordingPlan } from "./screen-record-plan";
import { initialRecordingSupport, resolveRecordingSupport, startRecording, type RecordingSession } from "./screen-record-backend";

export type CapturePhase = "confirm" | "encoding" | "ready";

export interface ScreenCaptureHost {
  /** Another capture surface (the snapshot picker) owns the screen right now. */
  blocked(): boolean;
  /** Any modal is open: focus must not jump behind it. */
  modalOpen(): boolean;
}

export class ScreenCaptureController {
  session = $state<RecordingSession | null>(null);
  support = $state(initialRecordingSupport());
  elapsed = $state(0);
  /** What the next (or running) recording captures; chosen in the dialog before the picker opens. */
  plan = $state<RecordingPlan>(DEFAULT_PLAN);
  /** The plan the running recording was started with, so changing the dialog cannot alter it. */
  activePlan = $state<RecordingPlan>(DEFAULT_PLAN);
  paused = $state(false);
  /** Why the last recording stopped by itself, or null when the user stopped it. */
  stoppedBy = $state<RecordingLimit | null>(null);
  dialog = $state<CapturePhase | null>(null);
  result = $state<CaptureResult | null>(null);
  error = $state<string | null>(null);
  errorTitle = $state("Couldn't start");
  busy = $state(false);
  starting = $state(false);

  #host: ScreenCaptureHost;
  #disposed = false;
  #finishing = false;
  #sequence = 0;
  #supportSequence = 0;
  #controller: AbortController | null = null;
  #encoding: RecordingSession | null = null;
  #trigger: HTMLElement | null = null;

  constructor(host: ScreenCaptureHost) {
    this.#host = host;
  }

  get canPause(): boolean { return typeof this.session?.pause === "function"; }

  setPlan = (plan: RecordingPlan) => { this.plan = clampPlan(plan); };

  /** Pause and resume leave the paused stretch out of the files; only backends that can do so offer it. */
  togglePause = () => {
    const session = this.session;
    if (!session?.pause || !session.resume || this.#finishing) return;
    if (this.paused) session.resume();
    else session.pause();
    this.paused = !this.paused;
  };

  /** Title-bar / palette / ⇧V entry point: stop a running recording, else open the dialog. */
  open = () => {
    if (this.session) { void this.finish(); return; }
    if (this.#host.blocked() || this.busy || this.#finishing || this.dialog) return;
    const active = document.activeElement;
    this.#trigger = active instanceof HTMLElement && active !== document.body ? active : null;
    this.error = null;
    this.errorTitle = "Couldn't start";
    this.dialog = this.result ? "ready" : "confirm";
    if (!this.result) this.refreshSupport();
  };

  refreshSupport = () => {
    const sequence = ++this.#supportSequence;
    this.support = initialRecordingSupport();
    if (this.support.pending) void resolveRecordingSupport().then((support) => {
      if (!this.#disposed && sequence === this.#supportSequence) this.support = support;
    });
  };

  begin = async () => {
    if (this.#disposed || this.busy || this.dialog !== "confirm" || !this.support.backend) return;
    const sequence = ++this.#sequence;
    const current = () => !this.#disposed && sequence === this.#sequence;
    this.error = null;
    this.errorTitle = "Couldn't start";
    this.elapsed = 0;
    this.paused = false;
    this.stoppedBy = null;
    const plan = clampPlan(this.plan);
    this.activePlan = plan;
    this.busy = true;
    this.starting = false;
    const controller = new AbortController();
    this.#controller = controller;
    try {
      let ended = false;
      const session = await startRecording(this.support.backend, {
        onTick: (ms) => { if (current()) this.elapsed = Math.min(plan.maxDurationMs, Math.max(0, ms)); },
        onLimit: (limit) => { if (current()) this.stoppedBy = limit; },
        onEnded: () => { ended = true; if (current()) void this.finish(); },
      }, controller.signal, () => { if (current()) this.starting = true; }, plan);
      if (!current()) { session.discard(); return; }
      this.session = session;
      this.dialog = null;
      this.busy = false;
      // Sharing can end while the preview or encoder is still starting.
      if (ended) void this.finish();
    } catch (cause) {
      if (!current()) return;
      this.busy = false;
      this.errorTitle = this.starting ? "Couldn't prepare recording" : "Couldn't start";
      this.error = this.starting ? (cause instanceof Error ? cause.message : String(cause)) : displayCaptureError(cause).message;
      this.dialog = "confirm";
    } finally {
      if (sequence === this.#sequence) { this.busy = false; this.starting = false; }
    }
  };

  finish = async () => {
    if (this.#finishing) return;
    const session = this.session;
    if (!session) return;
    const sequence = this.#sequence;
    this.#finishing = true;
    this.#encoding = session;
    this.session = null;
    this.paused = false;
    this.dialog = "encoding";
    try {
      const result = await session.stop();
      if (this.#disposed || sequence !== this.#sequence) { void result.release?.().catch(() => undefined); return; }
      this.result = result;
      this.dialog = "ready";
    } catch (cause) {
      if (this.#disposed || sequence !== this.#sequence) return;
      this.dialog = "confirm";
      this.errorTitle = "Couldn't finish";
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (sequence === this.#sequence) {
        this.#encoding = null;
        this.#finishing = false;
        this.busy = false;
      }
    }
  };

  /** Closing a preview keeps the one bounded result for this session; an explicit discard clears it. */
  close = () => {
    if (this.dialog === "ready" && this.result) this.dialog = null;
    else this.discard();
    this.#restoreFocus();
  };

  discardAndClose = () => {
    this.discard();
    this.#restoreFocus();
  };

  discard = () => {
    ++this.#sequence;
    this.#controller?.abort();
    this.#controller = null;
    this.session?.discard();
    this.#encoding?.discard();
    // A long desktop recording still lives in the native recorder's private folder.
    void this.result?.release?.().catch(() => undefined);
    this.session = null;
    this.#encoding = null;
    this.#finishing = false;
    this.dialog = null;
    this.result = null;
    this.error = null;
    this.busy = false;
    this.starting = false;
    this.elapsed = 0;
    this.paused = false;
    this.stoppedBy = null;
  };

  dispose = () => {
    this.#disposed = true;
    this.discard();
  };

  #restoreFocus() {
    // Let the outgoing dialog restore its own focus first, including a palette that opened this
    // preview, then choose the still-mounted recording trigger.
    void tick().then(() => {
      if (this.#disposed || this.#host.modalOpen()) return;
      const trigger = this.#trigger?.isConnected ? this.#trigger : document.querySelector<HTMLElement>("[data-screen-record]");
      trigger?.focus();
    });
  }
}
