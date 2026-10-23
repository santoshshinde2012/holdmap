// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { MAX_RECORDING_BYTES, revealSavedRecording, saveRecording, saveSnapshot } from "./screen-record-save";
import { MAX_SNAPSHOT_BYTES } from "./screen-snapshot";
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
  it("saves PNG through the same authorized binary native chooser", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValue("/chosen/snapshot.png");
    const png = { ...file(), filename: "holdmap-snapshot.png", blob: { size: 100, type: "image/png", arrayBuffer: async () => new ArrayBuffer(100) } as Blob };
    expect(await saveSnapshot(png)).toEqual({ saved: true, path: "/chosen/snapshot.png" });
    expect(invoke).toHaveBeenCalledWith("save_screen_recording", expect.any(ArrayBuffer), {
      headers: { "x-holdmap-recording-name": "holdmap-snapshot.png" },
    });
  });
  it("rejects a mislabeled or oversized snapshot before allocating save bytes", async () => {
    const arrayBuffer = vi.fn();
    for (const [type, size, filename] of [
      ["image/gif", 100, "holdmap-test.png"],
      ["image/png", MAX_SNAPSHOT_BYTES + 1, "holdmap-test.png"],
      ["image/png", 100, "holdmap-test.mp4"],
      ["image/png", 0, "holdmap-test.png"],
    ] as const) {
      await expect(saveSnapshot({ ...file(), filename, blob: { type, size, arrayBuffer } as unknown as Blob })).rejects.toThrow("32 MiB");
    }
    expect(arrayBuffer).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
    expect(download).not.toHaveBeenCalled();
  });
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

  it("exports a long native recording by copying the file, never through the window", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValue("/chosen/long.mp4");
    const arrayBuffer = vi.fn();
    const long: CaptureFile = { blob: { size: 0, arrayBuffer } as unknown as Blob, bytes: 900_000_000, filename: "holdmap-long.mp4", native: { sessionId: "abc", format: "mp4" } };
    expect(await saveRecording(long)).toEqual({ saved: true, path: "/chosen/long.mp4" });
    expect(invoke).toHaveBeenCalledWith("export_screen_recording", { sessionId: "abc", format: "mp4", filename: "holdmap-long.mp4" });
    expect(arrayBuffer).not.toHaveBeenCalled();
    invoke.mockResolvedValue(null);
    expect(await saveRecording(long)).toEqual({ saved: false });
  });

  it("refuses a native export outside the desktop app", async () => {
    const long: CaptureFile = { blob: new Blob([]), bytes: 5, filename: "x.mp4", native: { sessionId: "abc", format: "mp4" } };
    await expect(saveRecording(long)).rejects.toThrow("desktop app");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("reveals a saved file's folder only from the desktop app", async () => {
    await expect(revealSavedRecording("/a/b.mp4")).rejects.toThrow("desktop app");
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValue(undefined);
    await revealSavedRecording("/a/b.mp4");
    expect(invoke).toHaveBeenCalledWith("reveal_saved_recording", { path: "/a/b.mp4" });
  });
});
