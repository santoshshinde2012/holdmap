import type { CaptureFile } from "./screen-record";

export const MAX_SNAPSHOT_WIDTH = 3840;
export const MAX_SNAPSHOT_HEIGHT = 2160;
export const MAX_SNAPSHOT_BYTES = 32 * 1024 * 1024;
const FRAME_TIMEOUT_MS = 8_000;
const ENCODE_TIMEOUT_MS = 10_000;

export type SnapshotSource = "app" | "screen" | "recording";
export type SnapshotResult = {
  png: CaptureFile;
  width: number;
  height: number;
  capturedAtMs: number;
  source: SnapshotSource;
  recordingElapsedMs?: number;
};

/** A still-image capability is separate from the existing video session contract. */
export interface SnapshotProvider {
  captureSnapshot(signal: AbortSignal): Promise<SnapshotResult>;
}

export function snapshotProvider(value: unknown): SnapshotProvider | null {
  return value !== null && (typeof value === "object" || typeof value === "function")
    && typeof (value as Partial<SnapshotProvider>).captureSnapshot === "function"
    ? value as SnapshotProvider : null;
}

export const snapshotCancelled = () => new DOMException("Snapshot cancelled.", "AbortError");

/** AbortSignal.any is unavailable in some supported webviews. */
export function snapshotSignal(...signals: AbortSignal[]): { signal: AbortSignal; dispose: () => void } {
  const controller = new AbortController();
  const abort = () => controller.abort();
  for (const signal of signals) {
    if (signal.aborted) abort();
    else signal.addEventListener("abort", abort, { once: true });
  }
  return { signal: controller.signal, dispose: () => { for (const signal of signals) signal.removeEventListener("abort", abort); } };
}

export function snapshotWait<T>(promise: PromiseLike<T>, signal: AbortSignal, timeoutMs: number, message: string): Promise<T> {
  const operation = Promise.resolve(promise);
  if (signal.aborted) { void operation.catch(() => undefined); return Promise.reject(snapshotCancelled()); }
  return new Promise((resolve, reject) => {
    const clear = () => { clearTimeout(timer); signal.removeEventListener("abort", abort); };
    const abort = () => { clear(); reject(snapshotCancelled()); };
    const timer = setTimeout(() => { clear(); reject(new Error(message)); }, timeoutMs);
    signal.addEventListener("abort", abort, { once: true });
    operation.then((value) => { clear(); resolve(value); }, (cause) => { clear(); reject(cause); });
  });
}

export function snapshotSize(width: number, height: number): { width: number; height: number } {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    throw new Error("The selected source has no readable image.");
  }
  const scale = Math.min(1, MAX_SNAPSHOT_WIDTH / width, MAX_SNAPSHOT_HEIGHT / height);
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) };
}

export function snapshotFilename(at = new Date()): string {
  const part = (value: number, length = 2) => String(value).padStart(length, "0");
  return `holdmap-snapshot-${at.getFullYear()}${part(at.getMonth() + 1)}${part(at.getDate())}-${part(at.getHours())}${part(at.getMinutes())}${part(at.getSeconds())}-${part(at.getMilliseconds(), 3)}.png`;
}

/** Check the bounded PNG envelope; adapters remain responsible for rendering its pixels. */
export async function makeSnapshotResult(
  blob: Blob,
  source: SnapshotSource,
  metadata: { width: number; height: number; capturedAtMs?: number; recordingElapsedMs?: number },
): Promise<SnapshotResult> {
  const { width, height } = metadata;
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width <= 0 || height <= 0
    || width > MAX_SNAPSHOT_WIDTH || height > MAX_SNAPSHOT_HEIGHT
    || !blob || blob.size < 45 || blob.size > MAX_SNAPSHOT_BYTES || blob.type !== "image/png") {
    throw new Error("Snapshot must be a PNG up to 3840×2160 and 32 MiB.");
  }
  const [prefix, suffix] = await Promise.all([blob.slice(0, 33).arrayBuffer(), blob.slice(-12).arrayBuffer()]);
  const bytes = new Uint8Array(prefix), tail = new Uint8Array(suffix);
  const magic = [137, 80, 78, 71, 13, 10, 26, 10];
  const view = new DataView(prefix);
  if (bytes.length !== 33 || tail.length !== 12 || magic.some((value, index) => bytes[index] !== value)
    || view.getUint32(8) !== 13 || String.fromCharCode(...bytes.slice(12, 16)) !== "IHDR"
    || view.getUint32(16) !== width || view.getUint32(20) !== height
    || tail.some((value, index) => value !== [0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130][index])) {
    throw new Error("Couldn't read the complete PNG snapshot.");
  }
  const capturedAtMs = metadata.capturedAtMs ?? Date.now();
  if (!Number.isFinite(capturedAtMs) || capturedAtMs < 0
    || (metadata.recordingElapsedMs !== undefined && (!Number.isFinite(metadata.recordingElapsedMs) || metadata.recordingElapsedMs < 0))) {
    throw new Error("Couldn't read the snapshot timing.");
  }
  return {
    png: { blob, bytes: blob.size, filename: snapshotFilename(new Date(capturedAtMs)) },
    width, height, capturedAtMs, source,
    ...(metadata.recordingElapsedMs === undefined ? {} : { recordingElapsedMs: metadata.recordingElapsedMs }),
  };
}

function currentFrame(video: HTMLVideoElement, signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.reject(snapshotCancelled());
  if (video.readyState >= 2 && video.videoWidth > 0 && video.videoHeight > 0) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const events = ["loadeddata", "canplay", "resize"];
    const clear = () => {
      clearTimeout(timer);
      events.forEach((event) => video.removeEventListener(event, ready));
      video.removeEventListener("error", fail);
      video.removeEventListener("ended", fail);
      signal.removeEventListener("abort", abort);
    };
    const ready = () => { if (video.readyState >= 2 && video.videoWidth > 0 && video.videoHeight > 0) { clear(); resolve(); } };
    const fail = () => { clear(); reject(new Error("The selected source has no readable frame.")); };
    const abort = () => { clear(); reject(snapshotCancelled()); };
    const timer = setTimeout(fail, FRAME_TIMEOUT_MS);
    events.forEach((event) => video.addEventListener(event, ready));
    video.addEventListener("error", fail);
    video.addEventListener("ended", fail);
    signal.addEventListener("abort", abort, { once: true });
    ready();
  });
}

/** Borrows a played video; never stops tracks or changes the recording lifecycle. */
export async function captureVideoSnapshot(video: HTMLVideoElement, signal: AbortSignal, source: SnapshotSource, recordingElapsedMs?: number): Promise<SnapshotResult> {
  await currentFrame(video, signal);
  if (signal.aborted) throw snapshotCancelled();
  const size = snapshotSize(video.videoWidth, video.videoHeight);
  const canvas = document.createElement("canvas");
  canvas.width = size.width; canvas.height = size.height;
  try {
    const context = canvas.getContext("2d");
    if (!context) throw new Error("This window cannot prepare PNG snapshots.");
    context.drawImage(video, 0, 0, size.width, size.height);
    const capturedAtMs = Date.now();
    const blob = await snapshotWait(new Promise<Blob>((resolve, reject) => {
      canvas.toBlob((value) => value ? resolve(value) : reject(new Error("Couldn't encode the PNG snapshot.")), "image/png");
    }), signal, ENCODE_TIMEOUT_MS, "Preparing the snapshot took too long. Try again.");
    const result = await makeSnapshotResult(blob, source, { ...size, capturedAtMs, recordingElapsedMs });
    if (signal.aborted) throw snapshotCancelled();
    return result;
  } finally {
    canvas.width = 0; canvas.height = 0;
  }
}
