// @vitest-environment jsdom
import { Blob as NodeBlob } from "node:buffer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MAX_SNAPSHOT_BYTES, captureVideoSnapshot, makeSnapshotResult, snapshotFilename, snapshotProvider, snapshotSize, snapshotWait } from "./screen-snapshot";

const PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==";
const png = () => new Blob([Uint8Array.from(atob(PNG), (c) => c.charCodeAt(0))], { type: "image/png" });
function video(readyState = 2) {
  const value = document.createElement("video");
  Object.defineProperties(value, { videoWidth: { configurable: true, value: 1 }, videoHeight: { configurable: true, value: 1 }, readyState: { configurable: true, value: readyState } });
  return value;
}
beforeEach(() => vi.stubGlobal("Blob", NodeBlob));
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.useRealTimers(); });

describe("PNG snapshot contract", () => {
  it("bounds dimensions without upscaling or changing the aspect ratio", () => {
    expect(snapshotSize(7680, 4320)).toEqual({ width: 3840, height: 2160 });
    expect(snapshotSize(2000, 4000)).toEqual({ width: 1080, height: 2160 });
    expect(snapshotSize(320, 180)).toEqual({ width: 320, height: 180 });
    for (const value of [0, -1, Infinity, NaN]) expect(() => snapshotSize(value, 1)).toThrow(/readable image/);
  });

  it("keeps video contracts optional and produces a timestamped PNG identity", async () => {
    expect(snapshotProvider({ stop() {}, discard() {} })).toBeNull();
    const provider = { captureSnapshot: vi.fn() };
    expect(snapshotProvider(provider)).toBe(provider);
    expect(snapshotFilename(new Date(2026, 9, 16, 9, 5, 2, 3))).toBe("holdmap-snapshot-20261016-090502-003.png");
    const result = await makeSnapshotResult(png(), "recording", { width: 1, height: 1, capturedAtMs: 1_000, recordingElapsedMs: 250 });
    expect(result).toMatchObject({ width: 1, height: 1, capturedAtMs: 1_000, source: "recording", recordingElapsedMs: 250 });
    expect(result.png.bytes).toBe(result.png.blob.size);
  });

  it("rejects empty, oversized and wrongly labelled data before reading it", async () => {
    for (const [size, type] of [[0, "image/png"], [MAX_SNAPSHOT_BYTES + 1, "image/png"], [100, "image/jpeg"]] as const) {
      const slice = vi.fn();
      await expect(makeSnapshotResult({ size, type, slice } as unknown as Blob, "screen", { width: 1, height: 1 })).rejects.toThrow(/PNG/);
      expect(slice).not.toHaveBeenCalled();
    }
    await expect(makeSnapshotResult(png(), "app", { width: 3841, height: 1 })).rejects.toThrow(/3840/);
  });

  it("rejects forged dimensions, missing PNG termination and invalid timing", async () => {
    await expect(makeSnapshotResult(png(), "screen", { width: 2, height: 1 })).rejects.toThrow(/complete PNG/);
    const bytes = new Uint8Array(await png().arrayBuffer());
    bytes[bytes.length - 1] ^= 1;
    await expect(makeSnapshotResult(new Blob([bytes], { type: "image/png" }), "screen", { width: 1, height: 1 })).rejects.toThrow(/complete PNG/);
    await expect(makeSnapshotResult(png(), "screen", { width: 1, height: 1, capturedAtMs: NaN })).rejects.toThrow(/timing/);
  });
});

describe("single ready video frame", () => {
  it("does not mistake metadata for a decoded frame or stop the borrowed source", async () => {
    vi.useFakeTimers();
    const source = video(1);
    const stop = vi.fn(); source.srcObject = { getTracks: () => [{ stop }] } as unknown as MediaStream;
    const drawImage = vi.fn();
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({ drawImage } as unknown as CanvasRenderingContext2D);
    vi.spyOn(HTMLCanvasElement.prototype, "toBlob").mockImplementation((callback) => callback(png()));
    const pending = captureVideoSnapshot(source, new AbortController().signal, "recording", 1_000);
    source.dispatchEvent(new Event("loadedmetadata"));
    await Promise.resolve();
    expect(drawImage).not.toHaveBeenCalled();
    Object.defineProperty(source, "readyState", { value: 2 });
    source.dispatchEvent(new Event("loadeddata"));
    expect(await pending).toMatchObject({ source: "recording", recordingElapsedMs: 1_000, width: 1, height: 1 });
    expect(drawImage).toHaveBeenCalledWith(source, 0, 0, 1, 1);
    expect(stop).not.toHaveBeenCalled();
    expect(source.srcObject).not.toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("cancels a frame wait and clears its timer/listeners", async () => {
    vi.useFakeTimers();
    const source = video(0), controller = new AbortController();
    const added = vi.spyOn(source, "addEventListener"), removed = vi.spyOn(source, "removeEventListener");
    const pending = captureVideoSnapshot(source, controller.signal, "screen");
    const rejected = expect(pending).rejects.toMatchObject({ name: "AbortError" });
    controller.abort(); await rejected;
    expect(vi.getTimerCount()).toBe(0);
    for (const [name, handler] of added.mock.calls) expect(removed.mock.calls.some(([event, callback]) => event === name && callback === handler)).toBe(true);
  });

  it("bounds stalled PNG encoding and releases canvas pixels", async () => {
    vi.useFakeTimers();
    let canvas: HTMLCanvasElement | null = null;
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({ drawImage() {} } as unknown as CanvasRenderingContext2D);
    vi.spyOn(HTMLCanvasElement.prototype, "toBlob").mockImplementation(function(this: HTMLCanvasElement) { canvas = this; });
    const pending = captureVideoSnapshot(video(), new AbortController().signal, "screen");
    const rejected = expect(pending).rejects.toThrow(/too long/);
    await vi.advanceTimersByTimeAsync(10_000); await rejected;
    expect(canvas).not.toBeNull();
    expect(canvas!.width).toBe(0); expect(canvas!.height).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("cleans deadline timers on cancellation and observes abandoned promises", async () => {
    vi.useFakeTimers();
    const controller = new AbortController(); controller.abort();
    await expect(snapshotWait(Promise.reject(new Error("late failure")), controller.signal, 10, "timeout")).rejects.toMatchObject({ name: "AbortError" });
    expect(vi.getTimerCount()).toBe(0);
  });
});
