/** Local screen capture: MP4 (H.264) and GIF stay on this computer and are never uploaded. */

export const MAX_DURATION_MS = 120_000;
export const GIF_FPS = 8;
export const GIF_MAX_WIDTH = 640;
export const GIF_MAX_FRAMES = 240;
export const GIF_MAX_COLORS = 256;

export type CaptureFile = { blob: Blob; bytes: number; filename: string };
export type CaptureResult = {
  durationMs: number;
  width: number;
  height: number;
  mp4: CaptureFile | null;
  gif: CaptureFile | null;
  warnings: string[];
};

export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

export function recordingFilename(ext: "mp4" | "gif", at = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `holdmap-${at.getFullYear()}${p(at.getMonth() + 1)}${p(at.getDate())}-${p(at.getHours())}${p(at.getMinutes())}${p(at.getSeconds())}.${ext}`;
}

export function gifSize(srcW: number, srcH: number, maxEdge = GIF_MAX_WIDTH): { width: number; height: number } {
  const width = Math.max(1, Number.isFinite(srcW) ? srcW : 1);
  const height = Math.max(1, Number.isFinite(srcH) ? srcH : 1);
  const bound = Math.max(1, Number.isFinite(maxEdge) ? Math.floor(maxEdge) : GIF_MAX_WIDTH);
  const scale = Math.min(1, bound / Math.max(width, height));
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) };
}

/** Delay between GIF frames so a recording stays within `GIF_MAX_FRAMES`. */
export function gifDelayMs(durationMs: number): number {
  const elapsed = Math.max(1, durationMs);
  const ideal = 1_000 / GIF_FPS;
  if (elapsed / ideal <= GIF_MAX_FRAMES) return ideal;
  return elapsed / GIF_MAX_FRAMES;
}

export function captureSupport(): { display: boolean; gif: boolean } {
  return {
    display: typeof navigator !== "undefined" && typeof navigator.mediaDevices?.getDisplayMedia === "function",
    gif: typeof document !== "undefined",
  };
}

export function displayCaptureError(cause: unknown): Error {
  const error = cause as { name?: string; message?: string };
  if (error?.name === "NotAllowedError" || error?.name === "PermissionDeniedError") {
    return new Error("Screen selection was cancelled or permission was denied. Choose a screen again to retry. If access stays blocked, check screen recording permission for this app or browser in system settings.");
  }
  if (error?.name === "NotFoundError") return new Error("No screen or window was available to capture.");
  if (error?.name === "NotSupportedError") {
    return new Error("Screen capture is unavailable in this window. Try a browser that supports screen sharing, or use your operating system's screen recorder.");
  }
  if (error?.name === "NotReadableError") return new Error("The selected screen could not be read. Check system screen recording permission, then choose a screen again.");
  if (error?.name === "InvalidStateError") return new Error("Return to this window and choose a screen again to start recording.");
  if (error?.name === "AbortError") return new Error("Screen capture was interrupted before it started. Choose a screen again to retry.");
  return new Error(typeof error?.message === "string" && error.message.trim() ? error.message : "Couldn't start screen recording.");
}

export async function requestDisplayStream(): Promise<MediaStream> {
  if (!captureSupport().display) throw displayCaptureError({ name: "NotSupportedError" });
  try {
    return await navigator.mediaDevices.getDisplayMedia({
      video: { frameRate: 30, width: { max: 1920 }, height: { max: 1080 } },
      audio: false,
      preferCurrentTab: false,
      selfBrowserSurface: "include",
      systemAudio: "exclude",
    } as DisplayMediaStreamOptions);
  } catch (cause) {
    throw displayCaptureError(cause);
  }
}

export function downloadBlob(blob: Blob, filename: string): void {
  const href = URL.createObjectURL(blob);
  const link = document.createElement("a");
  try {
    link.href = href;
    link.download = filename;
    link.rel = "noopener";
    document.body.append(link);
    link.click();
    setTimeout(() => URL.revokeObjectURL(href), 30_000);
  } catch (cause) {
    URL.revokeObjectURL(href);
    throw cause;
  } finally {
    link.remove();
  }
}

export function stopTracks(stream: MediaStream | null): void {
  if (!stream) return;
  for (const track of stream.getTracks()) track.stop();
}
