import { invoke } from "@tauri-apps/api/core";
import { MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_HEIGHT, MAX_SNAPSHOT_WIDTH, makeSnapshotResult, snapshotCancelled, snapshotWait, type SnapshotResult } from "./screen-snapshot";

type NativeSnapshotStatus = {
  snapshotId: string;
  phase: "choosing" | "capturing" | "ready" | "cancelled" | "error";
  width: number;
  height: number;
  pngBytes: number;
  error: string | null;
};
const IPC_TIMEOUT_MS = 15_000;
const TOTAL_TIMEOUT_MS = 120_000;
const identifier = (value: unknown): value is string => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);

function friendlyError(code: string): string {
  const messages: Record<string, string> = {
    permission_denied: "Screen recording permission was denied. Allow Holdmap in System Settings, then try again.",
    window_unavailable: "Holdmap can only capture its own window, and that window isn't on screen. Bring it forward (not minimized or hidden), then try again.",
    invalid_dimensions: "The holdmap window has an unsupported size. Resize it and try again.",
    snapshot_timed_out: "Preparing the snapshot took too long. Try again.",
    snapshot_busy: "Wait for the current snapshot to finish or discard it.",
    recording_unavailable: "The recording is no longer capturing. Start a new recording or capture the holdmap window again.",
    invalid_directory: "Temporary snapshot files are unavailable. Try again.",
  };
  return messages[code] ?? "Couldn't capture the holdmap window. Try again.";
}

function nativeCall<T>(command: string, args: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args).catch((cause) => { throw new Error(friendlyError(cause instanceof Error ? cause.message : String(cause))); });
}

function checkedStatus(value: NativeSnapshotStatus, id: string): NativeSnapshotStatus {
  if (!value || value.snapshotId !== id || !["choosing", "capturing", "ready", "cancelled", "error"].includes(value.phase)
    || ![value.width, value.height, value.pngBytes].every((number) => Number.isSafeInteger(number) && number >= 0)
    || value.width > MAX_SNAPSHOT_WIDTH || value.height > MAX_SNAPSHOT_HEIGHT || value.pngBytes > MAX_SNAPSHOT_BYTES
    || (value.error !== null && typeof value.error !== "string")) throw new Error("Couldn't read the native snapshot status.");
  return value;
}

function usable(status: NativeSnapshotStatus) {
  if (status.phase === "cancelled") throw snapshotCancelled();
  if (status.phase === "error") {
    throw new Error(friendlyError(status.error ?? ""));
  }
}

function pause(signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.reject(snapshotCancelled());
  return new Promise((resolve, reject) => {
    const abort = () => { clearTimeout(timer); reject(snapshotCancelled()); };
    const timer = setTimeout(() => { signal.removeEventListener("abort", abort); resolve(); }, 150);
    signal.addEventListener("abort", abort, { once: true });
  });
}

/** Snapshot files have independent handles; cleanup never stops a recording session. */
export async function captureNativeSnapshot(signal: AbortSignal, sessionId?: string, recordingElapsedMs?: number): Promise<SnapshotResult> {
  if (signal.aborted) throw snapshotCancelled();
  let id: string | null = null;
  let abandoned = false;
  let cleanup: Promise<void> | null = null;
  const release = () => {
    if (!id) return Promise.resolve();
    cleanup ??= snapshotWait(nativeCall<void>("discard_screen_snapshot", { snapshotId: id }), new AbortController().signal, IPC_TIMEOUT_MS, "Couldn't remove temporary snapshot files.");
    return cleanup;
  };
  const request = nativeCall<NativeSnapshotStatus>("start_screen_snapshot", sessionId ? { sessionId } : {}).then((value) => {
    if (!identifier(value?.snapshotId)) throw new Error("Couldn't read the native snapshot identity.");
    id = value.snapshotId;
    if (abandoned || signal.aborted) { void release().catch(() => undefined); throw snapshotCancelled(); }
    return checkedStatus(value, id);
  });
  try {
    let status = await snapshotWait(request, signal, IPC_TIMEOUT_MS, "Native snapshot capture did not respond. Try again.");
    const deadline = performance.now() + TOTAL_TIMEOUT_MS;
    while (status.phase !== "ready") {
      usable(status);
      if (performance.now() >= deadline) throw new Error("Snapshot capture took too long. Try again.");
      await pause(signal);
      status = checkedStatus(await snapshotWait(nativeCall<NativeSnapshotStatus>("screen_snapshot_status", { snapshotId: id }), signal, IPC_TIMEOUT_MS, "Native snapshot capture did not respond. Try again."), id!);
    }
    usable(status);
    if (!status.pngBytes || !status.width || !status.height) throw new Error("No image was captured.");
    const bytes = await snapshotWait(nativeCall<ArrayBuffer>("read_screen_snapshot", { snapshotId: id }), signal, IPC_TIMEOUT_MS, "Couldn't read the native snapshot.");
    if (!(bytes instanceof ArrayBuffer) || bytes.byteLength !== status.pngBytes || bytes.byteLength > MAX_SNAPSHOT_BYTES) throw new Error("Couldn't read the complete native snapshot.");
    const result = await makeSnapshotResult(new Blob([bytes], { type: "image/png" }), sessionId ? "recording" : "screen", {
      width: status.width, height: status.height, recordingElapsedMs,
    });
    if (signal.aborted) throw snapshotCancelled();
    return result;
  } finally {
    abandoned = true;
    await release().catch(() => undefined);
  }
}
