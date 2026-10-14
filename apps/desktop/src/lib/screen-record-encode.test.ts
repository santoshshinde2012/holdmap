// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { ScreenRecording } from "./screen-record-encode";
import { GifTimeline, encodeGif } from "./screen-record-gif";

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

  it("preserves a two-minute timeline while bounding retained frames", () => {
    const timeline = new GifTimeline();
    for (let at = 0; at < 120_000; at += timeline.intervalMs) {
      timeline.add({ data: fill(2, 2, at > 110_000 ? 255 : 0, 0, 0), width: 2, height: 2 }, at);
      expect(timeline.frameCount).toBeLessThanOrEqual(240);
    }
    const decoded = gifTiming(timeline.finish(120_000));
    expect(decoded.delays.length).toBeLessThanOrEqual(240);
    expect(decoded.delays.reduce((sum, delay) => sum + delay, 0)).toBe(120_000);
    expect(decoded.colors.at(-1)).toEqual([255, 0, 0]);
  });

  it("rounds cumulative frame delays without speeding up or slowing down the recording", () => {
    const timeline = new GifTimeline();
    for (let i = 0; i < 20; i += 1) timeline.add({ data: fill(2, 2, 0, 0, 255), width: 2, height: 2 }, i * 125);
    expect(gifTiming(timeline.finish(2_500)).delays.reduce((sum, delay) => sum + delay, 0)).toBe(2_500);
  });

  it("rejects corrupt, oversized, changing or backwards frames", () => {
    const timeline = new GifTimeline();
    expect(() => timeline.add({ data: fill(1, 1, 0, 0, 0), width: 640, height: 640 }, 0)).toThrow(/dimensions/);
    expect(() => timeline.add({ data: fill(1, 1, 0, 0, 0), width: 641, height: 1 }, 0)).toThrow(/dimensions/);
    timeline.add({ data: fill(2, 2, 0, 0, 0), width: 2, height: 2 }, 100);
    expect(() => timeline.add({ data: fill(2, 2, 0, 0, 0), width: 2, height: 2 }, 50)).toThrow(/consistent/);
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
