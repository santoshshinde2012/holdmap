// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { MAX_RECORDING_BYTES, saveRecording } from "./screen-record-save";
import type { CaptureFile } from "./screen-record";

const { invoke, download } = vi.hoisted(() => ({ invoke: vi.fn(), download: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./screen-record", () => ({ downloadBlob: download }));

const file = (): CaptureFile => ({
  blob: { size: 12, arrayBuffer: async () => new ArrayBuffer(12) } as Blob,
  bytes: 12,
  filename: "holdmap-test.mp4",
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  vi.resetAllMocks();
});

describe("recording downloads", () => {
  it("uses the browser download flow outside the desktop", async () => {
    const capture = file();
    expect(await saveRecording(capture)).toEqual({ saved: true });
    expect(download).toHaveBeenCalledWith(capture.blob, capture.filename);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("sends binary bytes to a native dialog and reports the completed save", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValue("/chosen/recording.mp4");
    expect(await saveRecording(file())).toEqual({ saved: true, path: "/chosen/recording.mp4" });
    expect(invoke).toHaveBeenCalledWith("save_screen_recording", expect.any(ArrayBuffer), {
      headers: { "x-holdmap-recording-name": "holdmap-test.mp4" },
    });
    expect(download).not.toHaveBeenCalled();
  });

  it("preserves cancellation and save failures for retry", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValueOnce(null).mockRejectedValueOnce(new Error("Disk full"));
    expect(await saveRecording(file())).toEqual({ saved: false });
    await expect(saveRecording(file())).rejects.toThrow("Disk full");
  });

  it("rejects oversized or empty files before allocating an IPC buffer", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    for (const size of [0, MAX_RECORDING_BYTES + 1]) {
      const arrayBuffer = vi.fn();
      await expect(saveRecording({ ...file(), blob: { size, arrayBuffer } as unknown as Blob })).rejects.toThrow("128 MiB");
      expect(arrayBuffer).not.toHaveBeenCalled();
    }
    expect(invoke).not.toHaveBeenCalled();
  });
});
