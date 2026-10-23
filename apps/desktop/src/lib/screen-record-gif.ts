import { GIFEncoder, applyPalette, quantize } from "gifenc";
import { GIF_MAX_COLORS, GIF_MAX_WIDTH } from "./screen-record";

export type GifFrame = { data: Uint8ClampedArray; width: number; height: number };
export type GifLimits = { fps: number; maxBytes: number; maxDurationMs: number };
export type GifProgress = { intervalMs: number; bytes: number; full: boolean };
type IndexedFrame = { index: Uint8Array; palette: number[][]; at: number };

const MAX_DELAY_MS = 600_000;
/** Stop adding frames this close to the budget: the pending frame and trailer still need room. */
const FULL_SHARE = 0.96;
/** Start sampling more slowly once the whole recording would overshoot this share of the budget. */
const PACE_SHARE = 0.85;
const MIN_PACE_ELAPSED_MS = 1_000;
const SLOWEST_INTERVAL_MS = 1_000;

function validateFrame(frame: GifFrame): void {
  if (!Number.isInteger(frame.width) || !Number.isInteger(frame.height) || frame.width < 1 || frame.height < 1
    || frame.width > GIF_MAX_WIDTH || frame.height > GIF_MAX_WIDTH || frame.data.length !== frame.width * frame.height * 4) {
    throw new Error("The GIF frame has invalid dimensions.");
  }
}

/**
 * Writes the GIF as frames arrive, so memory holds the encoded bytes and one pending frame
 * instead of every frame. Unchanged screens add nothing, and the sampling interval stretches
 * when the projected size would pass the budget, so a long clip degrades to fewer frames
 * rather than a failed export.
 */
export class GifTimeline {
  private readonly gif = GIFEncoder();
  private pending: IndexedFrame | null = null;
  private previous: Uint32Array | null = null;
  private width = 0;
  private height = 0;
  private writtenCentiseconds = 0;
  private lastAt = 0;
  private written = 0;
  private full = false;
  intervalMs: number;

  constructor(private readonly limits: GifLimits) {
    this.intervalMs = 1_000 / limits.fps;
  }

  get frameCount(): number { return this.written + (this.pending ? 1 : 0); }
  get bytes(): number { return this.gif.bytes().byteLength; }

  add(frame: GifFrame, at: number): GifProgress {
    validateFrame(frame);
    if (!Number.isFinite(at) || at < 0) throw new Error("The GIF frame has an invalid timestamp.");
    if (this.pending && (frame.width !== this.width || frame.height !== this.height || at < this.lastAt)) {
      throw new Error("GIF frame dimensions and timestamps must remain consistent.");
    }
    const progress = (): GifProgress => ({ intervalMs: this.intervalMs, bytes: this.bytes, full: this.full });
    if (this.full) return progress();
    this.width = frame.width;
    this.height = frame.height;
    // Views avoid another full RGBA copy; the worker owns the transferred frame buffer.
    const view = new Uint8Array(frame.data.buffer, frame.data.byteOffset, frame.data.byteLength);
    const rgba = view.byteOffset % 4 === 0 ? view : view.slice();
    const pixels = new Uint32Array(rgba.buffer, rgba.byteOffset, rgba.byteLength >> 2);
    if (this.previous && this.previous.length === pixels.length && this.same(pixels)) return progress();
    this.previous = pixels.slice();
    const palette = quantize(rgba, GIF_MAX_COLORS);
    const next: IndexedFrame = { index: applyPalette(rgba, palette), palette, at };
    if (this.pending) this.write(this.pending, at);
    this.pending = next;
    this.lastAt = at;
    this.pace(at);
    return progress();
  }

  finish(durationMs: number): Uint8Array {
    if (!this.pending) throw new Error("Nothing was captured.");
    if (!Number.isFinite(durationMs) || durationMs < this.pending.at) throw new Error("The GIF duration is invalid.");
    this.write(this.pending, durationMs);
    this.pending = null;
    this.gif.finish();
    const bytes = this.gif.bytes();
    if (bytes.byteLength > 128 * 1024 * 1024) throw new Error("The GIF exceeded the 128 MiB recording limit. Try a shorter recording.");
    return bytes;
  }

  private same(pixels: Uint32Array): boolean {
    const previous = this.previous!;
    for (let i = 0; i < pixels.length; i += 1) if (pixels[i] !== previous[i]) return false;
    return true;
  }

  private write(frame: IndexedFrame, end: number): void {
    // Round cumulative time, rather than every frame, to avoid timing drift.
    const endCentiseconds = Math.round(Math.min(end, frame.at + MAX_DELAY_MS) / 10);
    const delay = Math.max(2, endCentiseconds - this.writtenCentiseconds) * 10;
    this.writtenCentiseconds += delay / 10;
    this.gif.writeFrame(frame.index, this.width, this.height, {
      palette: frame.palette, delay, ...(this.written === 0 ? { repeat: 0 } : {}),
    });
    this.written += 1;
  }

  private pace(at: number): void {
    const bytes = this.bytes;
    if (bytes >= this.limits.maxBytes * FULL_SHARE) { this.full = true; return; }
    if (at < MIN_PACE_ELAPSED_MS) return;
    const projected = (bytes / at) * this.limits.maxDurationMs;
    if (projected > this.limits.maxBytes * PACE_SHARE) {
      this.intervalMs = Math.min(SLOWEST_INTERVAL_MS, this.intervalMs * 1.25);
    }
  }
}

/** Small synchronous helper for fixtures; production quantization runs in a worker. */
export function encodeGif(frames: GifFrame[], delayMs: number): Blob {
  if (!frames.length) throw new Error("Nothing was captured.");
  if (!Number.isFinite(delayMs) || delayMs < 20) throw new Error("The GIF frame delay is invalid.");
  const timeline = new GifTimeline({ fps: 1_000 / delayMs, maxBytes: 128 * 1024 * 1024, maxDurationMs: frames.length * delayMs });
  frames.forEach((frame, i) => timeline.add(frame, i * delayMs));
  const bytes = timeline.finish(frames.length * delayMs);
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new Blob([copy], { type: "image/gif" });
}
