// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { ScreenRecording } from "./screen-record-encode";
import { GifTimeline, encodeGif } from "./screen-record-gif";
import { Blob as NodeBlob } from "node:buffer";

vi.mock("mediabunny", async (original) => ({
  ...await original<typeof import("mediabunny")>(),
  canEncodeVideo: vi.fn(async () => false),
}));

function fill(width: number, height: number, r: number, g: number, b: number): Uint8ClampedArray {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < data.length; i += 4) {
    data[i] = r;
    data[i + 1] = g;
    data[i + 2] = b;
    data[i + 3] = 255;
  }
  return data;
}

describe("GIF export", () => {
  it("writes a looping GIF89a with at least two frames", async () => {
    const blob = encodeGif([
      { data: fill(8, 8, 220, 40, 40), width: 8, height: 8 },
      { data: fill(8, 8, 40, 80, 220), width: 8, height: 8 },
    ], 125);
    expect(blob.type).toBe("image/gif");
    expect(blob.size).toBeGreaterThan(32);
    const bytes = new Uint8Array(await blob.arrayBuffer());
    expect(String.fromCharCode(...bytes.slice(0, 6))).toBe("GIF89a");
  });

  it("refuses an empty frame list", () => {
    expect(() => encodeGif([], 125)).toThrow(/Nothing was captured/);
  });

  const limits = { fps: 8, maxBytes: 128 * 1024 * 1024, maxDurationMs: 120_000 };
  const noise = (seed: number, size = 64) => {
    const data = new Uint8ClampedArray(size * size * 4);
    let x = seed * 2654435761 >>> 0;
    for (let i = 0; i < data.length; i += 1) { x = (x * 1664525 + 1013904223) >>> 0; data[i] = i % 4 === 3 ? 255 : x >>> 24; }
    return { data, width: size, height: size };
  };

  it("keeps a still two-minute recording to the frames that changed", () => {
    const timeline = new GifTimeline(limits);
    for (let at = 0; at < 120_000; at += timeline.intervalMs) {
      timeline.add({ data: fill(2, 2, at > 110_000 ? 255 : 0, 0, 0), width: 2, height: 2 }, at);
    }
    expect(timeline.frameCount).toBe(2);
    const decoded = gifTiming(timeline.finish(120_000));
    expect(decoded.delays.reduce((sum, delay) => sum + delay, 0)).toBe(120_000);
    expect(decoded.colors.at(-1)).toEqual([255, 0, 0]);
  });

  it("rounds cumulative frame delays without speeding up or slowing down the recording", () => {
    const timeline = new GifTimeline(limits);
    for (let i = 0; i < 20; i += 1) timeline.add({ data: fill(2, 2, i % 2 ? 0 : 255, 0, 255), width: 2, height: 2 }, i * 125);
    const delays = gifTiming(timeline.finish(2_500)).delays;
    expect(delays).toHaveLength(20);
    expect(delays.reduce((sum, delay) => sum + delay, 0)).toBe(2_500);
  });

  it("stops adding frames at the byte budget instead of failing the export", () => {
    const timeline = new GifTimeline({ fps: 8, maxBytes: 100_000, maxDurationMs: 60_000 });
    let full = false;
    for (let i = 0; i < 400 && !full; i += 1) full = timeline.add(noise(i), i * 125).full;
    expect(full).toBe(true);
    const bytes = timeline.finish(60_000);
    expect(bytes.byteLength).toBeLessThan(115_000);
    expect(gifTiming(bytes).delays.reduce((sum, delay) => sum + delay, 0)).toBe(60_000);
  });

  it("samples more slowly when the whole recording would pass the budget", () => {
    const timeline = new GifTimeline({ fps: 8, maxBytes: 200_000, maxDurationMs: 120_000 });
    const initial = timeline.intervalMs;
    for (let i = 0; i < 40; i += 1) timeline.add(noise(i, 24), i * timeline.intervalMs);
    expect(timeline.intervalMs).toBeGreaterThan(initial);
    expect(timeline.intervalMs).toBeLessThanOrEqual(1_000);
  });

  it("rejects corrupt, oversized, changing or backwards frames", () => {
    const timeline = new GifTimeline(limits);
    expect(() => timeline.add({ data: fill(1, 1, 0, 0, 0), width: 1000, height: 1000 }, 0)).toThrow(/dimensions/);
    expect(() => timeline.add({ data: fill(1, 1, 0, 0, 0), width: 961, height: 1 }, 0)).toThrow(/dimensions/);
    timeline.add({ data: fill(2, 2, 0, 0, 0), width: 2, height: 2 }, 100);
    expect(() => timeline.add({ data: fill(2, 2, 9, 0, 0), width: 2, height: 2 }, 50)).toThrow(/consistent/);
    expect(() => timeline.add({ data: fill(1, 1, 0, 0, 0), width: 1, height: 1 }, 200)).toThrow(/consistent/);
    expect(() => timeline.finish(50)).toThrow(/duration/);
  });
});

// Parse real GIF blocks, rather than treating a valid header as a successful animation.
function gifTiming(bytes: Uint8Array): { delays: number[]; colors: number[][] } {
  const delays: number[] = [];
  const colors: number[][] = [];
  let pos = 13;
  let palette = bytes.slice(pos, pos + ((bytes[10] & 128) ? 3 * 2 ** ((bytes[10] & 7) + 1) : 0));
  pos += palette.length;
  const blocks = () => { for (let size = bytes[pos++]; size; size = bytes[pos++]) pos += size; };
  while (pos < bytes.length) {
    const marker = bytes[pos++];
    if (marker === 0x3b) break;
    if (marker === 0x21) {
      const label = bytes[pos++];
      if (label === 0xf9) {
        const size = bytes[pos++];
        delays.push((bytes[pos + 1] | bytes[pos + 2] << 8) * 10);
        pos += size + 1;
      } else blocks();
    } else if (marker === 0x2c) {
      const packed = bytes[pos + 8];
      pos += 9;
      if (packed & 128) {
        const size = 3 * 2 ** ((packed & 7) + 1);
        palette = bytes.slice(pos, pos + size);
        pos += size;
      }
      colors.push(Array.from(palette.slice(0, 3)));
      pos += 1; // LZW minimum code size.
      blocks();
    } else throw new Error(`Invalid GIF block ${marker}`);
  }
  return { delays, colors };
}

describe("recording resources", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    document.body.innerHTML = "";
  });

  function fixture() {
    const track = Object.assign(new EventTarget(), {
      kind: "video", readyState: "live", getSettings: () => ({ width: 16, height: 8 }),
      stop: vi.fn(() => { track.readyState = "ended"; }),
    });
    const stream = { getVideoTracks: () => [track], getTracks: () => [track] } as unknown as MediaStream;
    return { stream, track, listener: { onTick: vi.fn(), onEnded: vi.fn() } };
  }

  it("releases tracks and the hidden player when startup playback fails", async () => {
    const { stream, track, listener } = fixture();
    vi.spyOn(HTMLMediaElement.prototype, "play").mockRejectedValue(new Error("blocked"));
    const start = ScreenRecording.start(stream, listener);
    document.querySelector("video")!.dispatchEvent(new Event("loadedmetadata"));
    await expect(start).rejects.toThrow(/never started/);
    expect(track.stop).toHaveBeenCalled();
    expect(document.querySelector("video")).toBeNull();
  });

  it("releases tracks and the hidden player when the picker selection ends during startup", async () => {
    const { stream, track, listener } = fixture();
    const start = ScreenRecording.start(stream, listener);
    track.dispatchEvent(new Event("ended"));
    await expect(start).rejects.toThrow(/never started/);
    expect(track.stop).toHaveBeenCalled();
    expect(document.querySelector("video")).toBeNull();
  });

  it("takes and cancels PNG snapshots while a GIF-only recording keeps capturing", async () => {
    const { stream, track, listener } = fixture();
    class FakeWorker {
      static latest: FakeWorker;
      onmessage: ((event: MessageEvent) => void) | null = null;
      onerror = null;
      onmessageerror = null;
      terminate = vi.fn();
      postMessage = vi.fn();
      constructor() { FakeWorker.latest = this; }
    }
    vi.stubGlobal("Blob", NodeBlob);
    vi.stubGlobal("Worker", FakeWorker);
    vi.stubGlobal("MediaRecorder", undefined);
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      drawImage: vi.fn(), getImageData: () => ({ data: fill(1, 1, 0, 0, 0) }),
    } as unknown as CanvasRenderingContext2D);
    const encode = vi.spyOn(HTMLCanvasElement.prototype, "toBlob").mockImplementation((callback) => callback(new Blob([
      Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg=="), (c) => c.charCodeAt(0)),
    ], { type: "image/png" })));
    const started = ScreenRecording.start(stream, listener);
    const player = document.querySelector("video")!;
    Object.defineProperties(player, { videoWidth: { value: 1 }, videoHeight: { value: 1 }, readyState: { value: 2 } });
    player.dispatchEvent(new Event("loadedmetadata"));
    const recording = await started;
    try {
      expect(await recording.captureSnapshot(new AbortController().signal)).toMatchObject({ source: "recording", width: 1, height: 1 });
      expect(track.stop).not.toHaveBeenCalled();
      expect(FakeWorker.latest.terminate).not.toHaveBeenCalled();
      expect(document.querySelector("video")).toBe(player);
      encode.mockImplementation(() => undefined);
      const controller = new AbortController();
      const pending = recording.captureSnapshot(controller.signal);
      const rejected = expect(pending).rejects.toMatchObject({ name: "AbortError" });
      await expect(recording.captureSnapshot(new AbortController().signal)).rejects.toThrow(/current snapshot/);
      controller.abort(); await rejected;
      expect(track.stop).not.toHaveBeenCalled();
      expect(listener.onEnded).not.toHaveBeenCalled();
    } finally { recording.discard(); }
    expect(FakeWorker.latest.terminate).toHaveBeenCalled();
  });

  it("stops capture before waiting for export and discards an in-flight GIF safely", async () => {
    const { stream, track, listener } = fixture();
    class FakeWorker {
      static latest: FakeWorker;
      onmessage: ((event: MessageEvent) => void) | null = null;
      onerror: (() => void) | null = null;
      onmessageerror: (() => void) | null = null;
      terminate = vi.fn();
      postMessage = vi.fn();
      constructor() { FakeWorker.latest = this; }
    }
    vi.stubGlobal("Worker", FakeWorker);
    vi.stubGlobal("MediaRecorder", undefined);
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      drawImage: vi.fn(), getImageData: () => ({ data: fill(640, 360, 0, 0, 0) }),
    } as unknown as CanvasRenderingContext2D);
    const start = ScreenRecording.start(stream, listener);
    document.querySelector("video")!.dispatchEvent(new Event("loadedmetadata"));
    const recording = await start;
    const stopped = recording.stop();
    expect(track.stop).toHaveBeenCalled();
    recording.discard();
    await expect(stopped).rejects.toThrow(/discarded/);
    expect(FakeWorker.latest.terminate).toHaveBeenCalled();
    expect(document.querySelector("video")).toBeNull();
  });
});
