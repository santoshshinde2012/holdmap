import { invoke } from "@tauri-apps/api/core";
import { recordingFilename, type CaptureFile, type CaptureResult, type RecordingListener, type RecordingSession } from "./screen-record";
import { DISK_MP4_LIMIT_BYTES, LEGACY_PLAN, planWantsGif, planWantsMp4, type RecordingPlan } from "./screen-record-plan";
import { snapshotSignal, type SnapshotResult } from "./screen-snapshot";
import { captureNativeSnapshot } from "./screen-snapshot-native";

export type NativeRecordingStatus = {
  sessionId: string;
  phase: "choosing" | "capturing" | "finalizing" | "ready" | "cancelled" | "error";
  elapsedMs: number;
  width: number;
  height: number;
  mp4Bytes: number;
  gifBytes: number;
  warnings: string[];
  error: string | null;
};

const POLL_MS = 250;
/** Files up to this size are copied into the window (preview and save); larger MP4s stay on disk. */
const MAX_READ_BYTES = 64 * 1024 * 1024;
const MAX_GIF_BYTES = 128 * 1024 * 1024;
const IPC_TIMEOUT_MS = 15_000;
const FINALIZE_TIMEOUT_MS = 120_000;
/** Turning a long MP4 into a GIF samples hundreds of frames, and may retry smaller. */
const LONG_GIF_FINALIZE_TIMEOUT_MS = 600_000;
const cancelled = () => new DOMException("Recording cancelled.", "AbortError");

function friendlyMessage(message: string): string {
  const messages: Record<string, string> = {
    unsupported_os: "Native screen recording requires macOS 14 or newer. Update macOS, or use a browser that supports screen sharing.",
    window_unavailable: "Holdmap can only record its own window, and that window isn't on screen. Bring it forward (not minimized or hidden), then record again. Nothing else is ever recorded.",
    permission_denied: "Screen recording permission was denied. Allow Holdmap in System Settings, then record again.",
    capture_start_failed: "Couldn't start capturing the holdmap window. Check screen recording permission in System Settings, then retry.",
    invalid_dimensions: "The holdmap window has an unsupported size. Resize it and try again.",
    capture_setup_failed: "Couldn't prepare the holdmap window for recording. Try recording again.",
    missing_writer: "The video encoder couldn't start. Try recording again.",
    mp4_failed: "Couldn't finish the recording. Try recording again.",
    gif_failed: "Couldn't finish the GIF. The MP4 may still be available.",
    missing_directory: "The temporary recording files are unavailable. Try recording again.",
  };
  return messages[message] || (/^[a-z][a-z0-9_]*$/.test(message) ? "Native screen recording failed. Try recording again." : message);
}

/** The native recorder re-validates every limit; the plan only chooses within them. */
function nativeOptions(plan: RecordingPlan) {
  return {
    maxDurationMs: plan.maxDurationMs,
    mp4: planWantsMp4(plan),
    gif: planWantsGif(plan),
    mp4Bitrate: plan.mp4Bitrate,
    gifFps: plan.gifFps,
    gifMaxEdge: plan.gifMaxEdge,
    gifMaxBytes: plan.gifMaxBytes,
  };
}

function nativeCall<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args).catch((cause) => {
    throw new Error(friendlyMessage(cause instanceof Error ? cause.message : String(cause)));
  });
}

function checkedStatus(value: NativeRecordingStatus, sessionId?: string): NativeRecordingStatus {
  if (!value || typeof value.sessionId !== "string" || !value.sessionId || value.sessionId.length > 256
    || (sessionId !== undefined && value.sessionId !== sessionId)
    || !["choosing", "capturing", "finalizing", "ready", "cancelled", "error"].includes(value.phase)
    || ![value.elapsedMs, value.width, value.height, value.mp4Bytes, value.gifBytes].every((number) => Number.isFinite(number) && number >= 0)
    || !Array.isArray(value.warnings) || !value.warnings.every((warning) => typeof warning === "string")
    || (value.error !== null && typeof value.error !== "string")) {
    throw new Error("Couldn't read the native recording status.");
  }
  return value;
}

function bounded<T>(promise: Promise<T>, signal: AbortSignal, timeout = IPC_TIMEOUT_MS): Promise<T> {
  if (signal.aborted) return Promise.reject(cancelled());
  return new Promise((resolve, reject) => {
    const clean = () => { clearTimeout(timer); signal.removeEventListener("abort", abort); };
    const abort = () => { clean(); reject(cancelled()); };
    const timer = setTimeout(() => { clean(); reject(new Error("Native recording took too long to respond. Try recording again.")); }, timeout);
    signal.addEventListener("abort", abort, { once: true });
    promise.then((value) => { clean(); resolve(value); }, (cause) => { clean(); reject(cause); });
  });
}

function pause(signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) { reject(cancelled()); return; }
    const abort = () => { clearTimeout(timer); reject(cancelled()); };
    const timer = setTimeout(() => { signal.removeEventListener("abort", abort); resolve(); }, POLL_MS);
    signal.addEventListener("abort", abort, { once: true });
  });
}

/** Owns native capture until its bounded exports have been copied into renderer memory. */
export class NativeRecordingSession implements RecordingSession {
  private readonly controller = new AbortController();
  private readonly stamp = new Date();
  private poll: ReturnType<typeof setTimeout> | null = null;
  private polling: Promise<void> | null = null;
  private finishing = false;
  private discarded = false;
  private ended = false;
  private failure: unknown = null;
  private stopped: Promise<CaptureResult> | null = null;
  private cleanup: Promise<void> | null = null;
  private snapshotting: Promise<SnapshotResult> | null = null;
  private snapshotAbort: AbortController | null = null;
  private readonly onAbort = () => this.discard();

  private constructor(private status: NativeRecordingStatus, private readonly listener: RecordingListener, private readonly signal: AbortSignal, private readonly plan: RecordingPlan) {
    signal.addEventListener("abort", this.onAbort, { once: true });
  }

  static async start(listener: RecordingListener, signal: AbortSignal, onPreparing: () => void, plan: RecordingPlan = LEGACY_PLAN): Promise<NativeRecordingSession> {
    if (signal.aborted) throw cancelled();
    let abandoned = false;
    let session: NativeRecordingSession | null = null;
    const request = nativeCall<NativeRecordingStatus>("start_screen_recording", { options: nativeOptions(plan) }).then((value) => {
      // Own the acknowledged session before another promise turn can cancel
      // startup. Its abort listener and bounded cleanup must cover the handoff.
      session = new NativeRecordingSession(checkedStatus(value), listener, signal, plan);
      if (abandoned || signal.aborted) {
        session.discard();
        throw cancelled();
      }
      return session;
    });
    try {
      session = await bounded(request, signal);
      while (session.status.phase === "choosing") {
        await pause(session.controller.signal);
        await session.refresh();
      }
      session.assertUsable();
      onPreparing();
      listener.onTick(session.elapsedMs());
      if (session.status.phase === "capturing") session.schedulePoll();
      else session.notifyEnded();
      return session;
    } catch (cause) {
      abandoned = true;
      session?.discard();
      throw cause;
    }
  }

  elapsedMs(): number { return Math.min(this.plan.maxDurationMs, this.status.elapsedMs); }

  captureSnapshot(signal: AbortSignal): Promise<SnapshotResult> {
    if (this.finishing || this.discarded || this.status.phase !== "capturing") return Promise.reject(new Error("The recording is no longer capturing."));
    if (this.snapshotting) return Promise.reject(new Error("Wait for the current snapshot to finish."));
    this.clearPoll();
    const controller = new AbortController();
    this.snapshotAbort = controller;
    const linked = snapshotSignal(signal, controller.signal, this.controller.signal);
    const operation = (async () => {
      try {
        await this.polling;
        this.assertUsable();
        if (this.finishing || this.status.phase !== "capturing") throw new Error("The recording is no longer capturing.");
        return await captureNativeSnapshot(linked.signal, this.status.sessionId, this.elapsedMs());
      } finally {
        linked.dispose();
        this.snapshotAbort = null;
        this.snapshotting = null;
        if (!this.finishing && !this.discarded && this.status.phase === "capturing") this.schedulePoll();
      }
    })();
    this.snapshotting = operation;
    return operation;
  }

  stop(): Promise<CaptureResult> {
    this.stopped ??= this.finish();
    return this.stopped;
  }

  discard(): void {
    this.discarded = true;
    this.finishing = true;
    this.clearPoll();
    this.snapshotAbort?.abort();
    this.controller.abort();
    this.signal.removeEventListener("abort", this.onAbort);
    void this.release().catch(() => undefined);
  }

  private async refresh() {
    this.status = await this.readStatus();
  }

  private async readStatus(): Promise<NativeRecordingStatus> {
    const value = await bounded(nativeCall<NativeRecordingStatus>("screen_recording_status", { sessionId: this.status.sessionId }), this.controller.signal);
    return checkedStatus(value, this.status.sessionId);
  }

  private assertUsable() {
    if (this.discarded || this.controller.signal.aborted) throw cancelled();
    if (this.status.phase === "cancelled") throw new Error("Recording was cancelled. Start again to retry.");
    if (this.status.phase === "error") throw new Error(this.status.error ? friendlyMessage(this.status.error) : "Native screen recording failed. Try recording again.");
  }

  private notifyEnded() {
    if (!this.ended && !this.discarded && !this.finishing) { this.ended = true; this.listener.onEnded(); }
  }

  private schedulePoll() {
    this.poll = setTimeout(() => {
      this.poll = null;
      this.polling = this.readStatus().then((status) => {
        if (this.finishing || this.discarded) return;
        this.status = status;
        this.listener.onTick(this.elapsedMs());
        if (this.status.elapsedMs >= this.plan.maxDurationMs - 250) this.listener.onLimit?.("duration");
        if (this.status.phase !== "capturing" || this.elapsedMs() >= this.plan.maxDurationMs) this.notifyEnded();
        else if (!this.snapshotting) this.schedulePoll();
      }).catch((cause) => {
        if (this.finishing || this.discarded) return;
        this.failure = cause;
        this.notifyEnded();
      }).finally(() => {
        this.polling = null;
      });
    }, POLL_MS);
  }

  private clearPoll() {
    if (this.poll !== null) clearTimeout(this.poll);
    this.poll = null;
  }

  private release(): Promise<void> {
    this.cleanup ??= bounded(nativeCall<void>("discard_screen_recording", { sessionId: this.status.sessionId }), new AbortController().signal).catch((cause) => {
      this.cleanup = null;
      throw cause;
    });
    return this.cleanup;
  }

  private async finish(): Promise<CaptureResult> {
    this.finishing = true;
    // A long MP4 is never copied into the window: its session stays open until the result is released.
    let keepSession = false;
    this.clearPoll();
    this.snapshotAbort?.abort();
    const warnings = this.status.warnings.map(friendlyMessage);
    try {
      // The native boundary permits one operation at a time. A status request
      // already in flight must finish before Stop or any export read is sent.
      await this.snapshotting?.catch(() => undefined);
      await this.polling;
      this.assertUsable();
      if (this.failure) throw this.failure;
      if (this.status.phase !== "ready" && this.status.phase !== "finalizing") {
        this.status = checkedStatus(await bounded(nativeCall<NativeRecordingStatus>("stop_screen_recording", { sessionId: this.status.sessionId }), this.controller.signal), this.status.sessionId);
      }
      const deadline = performance.now() + (planWantsGif(this.plan) && this.plan.maxDurationMs > 300_000 ? LONG_GIF_FINALIZE_TIMEOUT_MS : FINALIZE_TIMEOUT_MS);
      while (this.status.phase !== "ready") {
        this.assertUsable();
        if (performance.now() >= deadline) throw new Error("Preparing this recording took too long. Try a shorter recording.");
        await pause(this.controller.signal);
        await this.refresh();
      }
      this.assertUsable();
      warnings.push(...this.status.warnings.map(friendlyMessage).filter((warning) => !warnings.includes(warning)));
      const read = async (format: "mp4" | "gif"): Promise<CaptureFile | null> => {
        const expected = format === "mp4" ? this.status.mp4Bytes : this.status.gifBytes;
        if (!expected) return null;
        const limit = format === "mp4" ? DISK_MP4_LIMIT_BYTES : MAX_GIF_BYTES;
        if (!Number.isSafeInteger(expected) || expected > limit) throw new Error("Recording is too large to save.");
        if (format === "mp4" && expected > MAX_READ_BYTES) {
          keepSession = true;
          return { blob: new Blob([]), bytes: expected, filename: recordingFilename(format, this.stamp), native: { sessionId: this.status.sessionId, format } };
        }
        const bytes = await bounded(nativeCall<ArrayBuffer>("read_screen_recording", { sessionId: this.status.sessionId, format }), this.controller.signal);
        this.assertUsable();
        if (!(bytes instanceof ArrayBuffer) || bytes.byteLength !== expected || bytes.byteLength > MAX_READ_BYTES) throw new Error("Couldn't read the complete native recording.");
        const blob = new Blob([bytes], { type: format === "mp4" ? "video/mp4" : "image/gif" });
        return { blob, bytes: blob.size, filename: recordingFilename(format, this.stamp) };
      };
      const files: { mp4: CaptureFile | null; gif: CaptureFile | null } = { mp4: null, gif: null };
      for (const format of ["mp4", "gif"] as const) {
        try { files[format] = await read(format); }
        catch (cause) {
          this.assertUsable();
          warnings.push(cause instanceof Error ? cause.message : `Couldn't read the ${format.toUpperCase()}.`);
        }
      }
      if (!files.mp4 && !files.gif) throw new Error(warnings.at(-1) || "Nothing was captured. Try another screen or window.");
      return {
        durationMs: this.elapsedMs(), width: this.status.width, height: this.status.height, ...files, warnings,
        ...(keepSession ? { release: () => this.release() } : {}),
      };
    } finally {
      this.controller.abort();
      this.signal.removeEventListener("abort", this.onAbort);
      if (!keepSession) await this.release().catch(() => { warnings.push("Couldn't remove temporary recording files."); });
    }
  }
}
