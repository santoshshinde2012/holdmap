import { invoke } from "@tauri-apps/api/core";
import { MAX_DURATION_MS, recordingFilename, type CaptureFile, type CaptureResult, type RecordingListener, type RecordingSession } from "./screen-record";

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
const MAX_EXPORT_BYTES = 128 * 1024 * 1024;
const IPC_TIMEOUT_MS = 15_000;
const FINALIZE_TIMEOUT_MS = 120_000;
const cancelled = () => new DOMException("Recording cancelled.", "AbortError");

function friendlyMessage(message: string): string {
  const messages: Record<string, string> = {
    unsupported_os: "Native screen recording requires macOS 14 or newer. Update macOS, or use a browser that supports screen sharing.",
    picker_failed: "Couldn't open the screen picker. Choose a screen again to retry.",
    permission_denied: "Screen recording permission was denied. Allow Holdmap in System Settings, then choose a screen again.",
    capture_start_failed: "Couldn't start capturing the selected screen. Check screen recording permission in System Settings, then retry.",
    invalid_dimensions: "The selected screen has an unsupported size. Try another screen or window.",
    capture_setup_failed: "Couldn't prepare the selected screen for recording. Choose a screen again to retry.",
    missing_writer: "The video encoder couldn't start. Try recording again.",
    mp4_failed: "Couldn't finish the recording. Try recording again.",
    gif_failed: "Couldn't finish the GIF. The MP4 may still be available.",
    missing_directory: "The temporary recording files are unavailable. Try recording again.",
  };
  return messages[message] || (/^[a-z][a-z0-9_]*$/.test(message) ? "Native screen recording failed. Choose a screen again to retry." : message);
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
  private readonly onAbort = () => this.discard();

  private constructor(private status: NativeRecordingStatus, private readonly listener: RecordingListener, private readonly signal: AbortSignal) {
    signal.addEventListener("abort", this.onAbort, { once: true });
  }

  static async start(listener: RecordingListener, signal: AbortSignal, onPreparing: () => void): Promise<NativeRecordingSession> {
    if (signal.aborted) throw cancelled();
    let abandoned = false;
    let session: NativeRecordingSession | null = null;
    const request = nativeCall<NativeRecordingStatus>("start_screen_recording").then((value) => {
      // Own the acknowledged session before another promise turn can cancel
      // startup. Its abort listener and bounded cleanup must cover the handoff.
      session = new NativeRecordingSession(checkedStatus(value), listener, signal);
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

  elapsedMs(): number { return Math.min(MAX_DURATION_MS, this.status.elapsedMs); }

  stop(): Promise<CaptureResult> {
    this.stopped ??= this.finish();
    return this.stopped;
  }

  discard(): void {
    this.discarded = true;
    this.finishing = true;
    this.clearPoll();
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
    if (this.status.phase === "cancelled") throw new Error("Screen selection was cancelled. Choose a screen again to retry.");
    if (this.status.phase === "error") throw new Error(this.status.error ? friendlyMessage(this.status.error) : "Native screen recording failed. Choose a screen again to retry.");
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
        if (this.status.phase !== "capturing" || this.elapsedMs() >= MAX_DURATION_MS) this.notifyEnded();
        else this.schedulePoll();
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
    this.clearPoll();
    const warnings = this.status.warnings.map(friendlyMessage);
    try {
      // The native boundary permits one operation at a time. A status request
      // already in flight must finish before Stop or any export read is sent.
      await this.polling;
      this.assertUsable();
      if (this.failure) throw this.failure;
      if (this.status.phase !== "ready" && this.status.phase !== "finalizing") {
        this.status = checkedStatus(await bounded(nativeCall<NativeRecordingStatus>("stop_screen_recording", { sessionId: this.status.sessionId }), this.controller.signal), this.status.sessionId);
      }
      const deadline = performance.now() + FINALIZE_TIMEOUT_MS;
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
        if (!Number.isSafeInteger(expected) || expected > MAX_EXPORT_BYTES) throw new Error("Recording must be smaller than 128 MiB.");
        const bytes = await bounded(nativeCall<ArrayBuffer>("read_screen_recording", { sessionId: this.status.sessionId, format }), this.controller.signal);
        this.assertUsable();
        if (!(bytes instanceof ArrayBuffer) || bytes.byteLength !== expected || bytes.byteLength > MAX_EXPORT_BYTES) throw new Error("Couldn't read the complete native recording.");
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
      return { durationMs: this.elapsedMs(), width: this.status.width, height: this.status.height, ...files, warnings };
    } finally {
      this.controller.abort();
      this.signal.removeEventListener("abort", this.onAbort);
      await this.release().catch(() => { warnings.push("Couldn't remove temporary recording files."); });
    }
  }
}
