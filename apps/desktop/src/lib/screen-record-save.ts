import { downloadBlob, type CaptureFile } from "./screen-record";
import { MAX_SNAPSHOT_BYTES } from "./screen-snapshot";

export const MAX_RECORDING_BYTES = 128 * 1024 * 1024;

/** PNG uses the same local chooser and atomic save boundary with its own tighter limit. */
export async function saveSnapshot(file: CaptureFile): Promise<{ saved: boolean; path?: string }> {
  if (file.blob.type !== "image/png" || !file.filename.endsWith(".png")
    || !file.blob.size || file.blob.size > MAX_SNAPSHOT_BYTES) {
    throw new Error("Snapshot must be a PNG smaller than 32 MiB.");
  }
  return saveRecording(file);
}

/** Native saves report completion; browsers hand the download to the browser. */
export async function saveRecording(file: CaptureFile): Promise<{ saved: boolean; path?: string }> {
  if (!file.blob.size || file.blob.size > MAX_RECORDING_BYTES) {
    throw new Error("Recording must contain data and be smaller than 128 MiB.");
  }
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    const { invoke } = await import("@tauri-apps/api/core");
    const path = await invoke<string | null>("save_screen_recording", await file.blob.arrayBuffer(), {
      headers: { "x-holdmap-recording-name": file.filename },
    });
    return path ? { saved: true, path } : { saved: false };
  }
  downloadBlob(file.blob, file.filename);
  return { saved: true };
}
