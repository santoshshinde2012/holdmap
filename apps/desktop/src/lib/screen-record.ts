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

export function gifSize(srcW: number, srcH: number, maxWidth = GIF_MAX_WIDTH): { width: number; height: number } {
  const width = Math.max(1, srcW);
  const height = Math.max(1, srcH);
  if (width <= maxWidth) return { width, height };
  return { width: maxWidth, height: Math.max(1, Math.round(height * (maxWidth / width))) };
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
    return new Error("Screen recording was cancelled or blocked. Grant screen recording permission and try again.");
  }
  if (error?.name === "NotFoundError") return new Error("No screen or window was available to capture.");
  if (error?.name === "NotSupportedError" || error?.name === "TypeError") {
    return new Error("This window cannot capture the screen. Use the Holdmap desktop app, or Chrome, Edge or Safari.");
  }
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
  link.href = href;
  link.download = filename;
  link.rel = "noopener";
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(href), 30_000);
}

export function stopTracks(stream: MediaStream | null): void {
  if (!stream) return;
  for (const track of stream.getTracks()) track.stop();
}
