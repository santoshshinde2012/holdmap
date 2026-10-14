import { downloadBlob, type CaptureFile } from "./screen-record";

export const MAX_RECORDING_BYTES = 128 * 1024 * 1024;

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
