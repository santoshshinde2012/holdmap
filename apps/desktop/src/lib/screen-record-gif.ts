import { GIFEncoder, applyPalette, quantize } from "gifenc";
import { GIF_FPS, GIF_MAX_COLORS, GIF_MAX_FRAMES } from "./screen-record";

export type GifFrame = { data: Uint8ClampedArray; width: number; height: number };
type IndexedFrame = { index: Uint8Array; palette: number[][]; at: number };

function validateFrame(frame: GifFrame): void {
  if (!Number.isInteger(frame.width) || !Number.isInteger(frame.height) || frame.width < 1 || frame.height < 1
    || frame.width > 640 || frame.height > 640 || frame.data.length !== frame.width * frame.height * 4) {
    throw new Error("The GIF frame has invalid dimensions.");
  }
}

/** Keeps a bounded, progressively sampled timeline instead of truncating long recordings. */
export class GifTimeline {
  private frames: IndexedFrame[] = [];
  private width = 0;
  private height = 0;
  intervalMs = 1_000 / GIF_FPS;

  get frameCount(): number { return this.frames.length; }

  add(frame: GifFrame, at: number): void {
    validateFrame(frame);
    if (!Number.isFinite(at) || at < 0) throw new Error("The GIF frame has an invalid timestamp.");
    if (this.frames.length && (frame.width !== this.width || frame.height !== this.height
      || at < this.frames[this.frames.length - 1].at)) {
      throw new Error("GIF frame dimensions and timestamps must remain consistent.");
    }
    if (this.frames.length >= GIF_MAX_FRAMES) {
      this.frames = this.frames.filter((_, i) => i % 2 === 0);
      this.intervalMs *= 2;
    }
    this.width = frame.width;
    this.height = frame.height;
    // Views avoid another full RGBA copy; the worker owns the transferred frame buffer.
    const rgba = new Uint8Array(frame.data.buffer, frame.data.byteOffset, frame.data.byteLength);
    const palette = quantize(rgba, GIF_MAX_COLORS);
    this.frames.push({ index: applyPalette(rgba, palette), palette, at });
  }

  finish(durationMs: number): Uint8Array {
    if (!this.frames.length) throw new Error("Nothing was captured.");
    if (!Number.isFinite(durationMs) || durationMs < this.frames[this.frames.length - 1].at) {
      throw new Error("The GIF duration is invalid.");
    }
    const gif = GIFEncoder();
    let writtenCentiseconds = 0;
    for (let i = 0; i < this.frames.length; i += 1) {
      const frame = this.frames[i];
      const end = this.frames[i + 1]?.at ?? durationMs;
      // Round cumulative time, rather than every 125 ms frame, to avoid timing drift.
      const endCentiseconds = Math.round(end / 10);
      const delay = Math.max(2, endCentiseconds - writtenCentiseconds) * 10;
      writtenCentiseconds += delay / 10;
      gif.writeFrame(frame.index, this.width, this.height, {
        palette: frame.palette, delay, ...(i === 0 ? { repeat: 0 } : {}),
      });
    }
    gif.finish();
    this.frames = [];
    const bytes = gif.bytes();
    if (bytes.byteLength > 128 * 1024 * 1024) throw new Error("The GIF exceeded the 128 MiB recording limit. Try a shorter recording.");
    return bytes;
  }
}

/** Small synchronous helper for fixtures; production quantization runs in a worker. */
export function encodeGif(frames: GifFrame[], delayMs: number): Blob {
  if (!frames.length) throw new Error("Nothing was captured.");
  if (!Number.isFinite(delayMs) || delayMs < 20) throw new Error("The GIF frame delay is invalid.");
  const timeline = new GifTimeline();
  frames.forEach((frame, i) => timeline.add(frame, i * delayMs));
  const bytes = timeline.finish(frames.length * delayMs);
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new Blob([copy], { type: "image/gif" });
}
