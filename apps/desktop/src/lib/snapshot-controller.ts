import type { SnapshotProvider, SnapshotResult, SnapshotSource } from "./screen-snapshot";

export type SnapshotState = {
  open: boolean;
  busy: boolean;
  phase: "choose" | "ready";
  result: SnapshotResult | null;
  error: string | null;
};

export type SnapshotAdapters = {
  app: (signal: AbortSignal) => Promise<SnapshotResult>;
  screen: (signal: AbortSignal) => Promise<SnapshotResult>;
  recording: () => SnapshotProvider | null;
  prepare: (signal: AbortSignal) => Promise<void>;
};

/** The app shell depends only on the capture UI actions it exposes. */
export interface SnapshotCaptureHandle {
  show(): void;
  cancel(): void;
  captureRecording(): Promise<void>;
}

export const initialSnapshotState = (): SnapshotState => ({
  open: false, busy: false, phase: "choose", result: null, error: null,
});

/** Owns one retained PNG and request cancellation. Providers own pixels; the view owns saving. */
export function createSnapshotController(adapters: SnapshotAdapters, changed: (state: SnapshotState) => void) {
  let state = initialSnapshotState();
  let generation = 0;
  let request: AbortController | null = null;
  let disposed = false;
  const update = (patch: Partial<SnapshotState>) => {
    state = { ...state, ...patch };
    if (!disposed) changed(state);
  };
  const cancel = () => {
    generation++;
    request?.abort();
    request = null;
    update({ busy: false, open: false, error: null, phase: state.result ? "ready" : "choose" });
  };
  return {
    show() {
      if (!disposed && !state.busy) update({ open: true, phase: state.result ? "ready" : "choose", error: null });
    },
    choose() { if (!disposed && !state.busy) update({ open: true, phase: "choose", error: null }); },
    review() { if (!disposed && !state.busy && state.result) update({ open: true, phase: "ready", error: null }); },
    close() { if (state.busy) cancel(); else update({ open: false }); },
    cancel,
    discard() { cancel(); update({ result: null, phase: "choose" }); },
    async capture(source: SnapshotSource) {
      if (disposed || state.busy) return;
      const provider = source === "recording" ? adapters.recording() : null;
      if (source === "recording" && !provider) {
        update({ open: true, phase: "choose", error: "The recording source is no longer available. Choose another source." });
        return;
      }
      const sequence = ++generation;
      const controller = new AbortController();
      request = controller;
      update({ busy: true, open: false, error: null });
      try {
        // Keep the screen picker in the click turn. App/borrowed sources need no picker,
        // so first let the snapshot chooser disappear from the pixels being captured.
        let pending: Promise<SnapshotResult>;
        if (source === "screen") pending = adapters.screen(controller.signal);
        else {
          await adapters.prepare(controller.signal);
          if (controller.signal.aborted) return;
          pending = source === "recording" ? provider!.captureSnapshot(controller.signal) : adapters.app(controller.signal);
        }
        const result = await pending;
        if (!disposed && sequence === generation && !controller.signal.aborted) {
          update({ result, phase: "ready", open: true, error: null });
        }
      } catch (cause) {
        if (!disposed && sequence === generation && !controller.signal.aborted) {
          update({ phase: "choose", open: true, error: cause instanceof Error ? cause.message : String(cause) });
        }
      } finally {
        if (!disposed && sequence === generation) {
          request = null;
          update({ busy: false });
        }
      }
    },
    dispose() {
      disposed = true;
      generation++;
      request?.abort();
      request = null;
      state = initialSnapshotState();
    },
  };
}
