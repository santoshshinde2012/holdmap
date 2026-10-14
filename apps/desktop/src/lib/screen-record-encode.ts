import { GIFEncoder, applyPalette, quantize } from "gifenc";
import {
  BufferTarget,
  QUALITY_HIGH,
  Mp4OutputFormat,
  MediaStreamVideoTrackSource,
  Output,
  canEncodeVideo,
} from "mediabunny";
import {
  GIF_MAX_COLORS,
  GIF_MAX_FRAMES,
  MAX_DURATION_MS,
  gifDelayMs,
  gifSize,
  recordingFilename,
  stopTracks,
  type CaptureResult,
} from "./screen-record";

export type RecordingListener = {
  onTick: (elapsedMs: number) => void;
  onEnded: () => void;
};

type Mp4Session = {
  close: () => void;
  cancel: () => void;
  finalize: () => Promise<Blob | null>;
};

function even(n: number, fallback: number): number {
  const size = Math.max(2, Math.round(n || fallback));
  return size % 2 === 0 ? size : size + 1;
}

function attachVideo(stream: MediaStream): Promise<HTMLVideoElement> {
  const video = document.createElement("video");
  video.muted = true;
  video.playsInline = true;
  video.autoplay = true;
  video.setAttribute("aria-hidden", "true");
  video.style.cssText = "position:fixed;left:-9999px;width:1px;height:1px;opacity:0;pointer-events:none";
  video.srcObject = stream;
  document.body.append(video);
  return new Promise((resolve, reject) => {
    let settled = false;
    const fail = () => {
      if (settled) return;
      settled = true;
      reject(new Error("The captured surface never started playing."));
    };
    const timer = setTimeout(fail, 8_000);
    video.addEventListener("error", fail, { once: true });
    video.addEventListener("loadedmetadata", () => {
      void video.play().then(() => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve(video);
      }).catch(() => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve(video);
      });
    }, { once: true });
  });
}

async function startWebCodecsMp4(track: MediaStreamVideoTrack): Promise<Mp4Session | null> {
  if (!await canEncodeVideo("avc")) return null;
  const target = new BufferTarget();
  const output = new Output({ format: new Mp4OutputFormat(), target });
  const settings = track.getSettings();
  const source = new MediaStreamVideoTrackSource(track, {
    codec: "avc",
    quality: QUALITY_HIGH,
    latencyMode: "realtime",
    contentHint: "detail",
    sizeChangeBehavior: "passThrough",
    transform: { width: even(settings.width ?? 0, 1280), height: even(settings.height ?? 0, 720), fit: "contain" },
  });
  output.addVideoTrack(source);
  await output.start();
  return {
    close: () => source.close(),
    cancel: () => { source.close(); void output.cancel().catch(() => undefined); },
    async finalize() {
      await output.finalize();
      const buffer = target.buffer;
      return buffer && buffer.byteLength > 0 ? new Blob([buffer], { type: "video/mp4" }) : null;
    },
  };
}

function startRecorderMp4(track: MediaStreamVideoTrack): Mp4Session | null {
  if (typeof MediaRecorder === "undefined") return null;
  const mime = ["video/mp4;codecs=avc1.42E01E", "video/mp4"].find((type) => MediaRecorder.isTypeSupported(type));
  if (!mime) return null;
  const recorder = new MediaRecorder(new MediaStream([track]), { mimeType: mime, videoBitsPerSecond: 5_000_000 });
  const chunks: Blob[] = [];
  recorder.ondataavailable = (event) => { if (event.data.size > 0) chunks.push(event.data); };
  recorder.start(400);
  const blob = () => chunks.length ? new Blob(chunks, { type: "video/mp4" }) : null;
  const stop = () => { if (recorder.state !== "inactive") recorder.stop(); };
  return {
    close() {},
    cancel() { chunks.length = 0; stop(); },
    finalize() {
      if (recorder.state === "inactive") return Promise.resolve(blob());
      return new Promise((resolve) => {
        recorder.addEventListener("stop", () => resolve(blob()), { once: true });
        stop();
      });
    },
  };
}

async function startMp4(track: MediaStreamVideoTrack): Promise<Mp4Session | null> {
  try {
    const encoded = await startWebCodecsMp4(track);
    if (encoded) return encoded;
  } catch {
    // Safari and some webviews still record MP4 through MediaRecorder.
  }
  return startRecorderMp4(track);
}

function writeGifFrame(gif: ReturnType<typeof GIFEncoder>, data: Uint8ClampedArray, width: number, height: number, delay: number, first: boolean) {
  const rgba = new Uint8Array(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
  const palette = quantize(rgba, GIF_MAX_COLORS);
  const index = applyPalette(rgba, palette);
  gif.writeFrame(index, width, height, { palette, delay: Math.max(20, Math.round(delay)), ...(first ? { repeat: 0 } : {}) });
}

/** Encode RGBA frames into a looping GIF. Used by the recorder and by tests. */
export function encodeGif(frames: { data: Uint8ClampedArray; width: number; height: number }[], delayMs: number): Blob {
  if (!frames.length) throw new Error("Nothing was captured.");
  const gif = GIFEncoder();
  frames.forEach((frame, i) => writeGifFrame(gif, frame.data, frame.width, frame.height, delayMs, i === 0));
  gif.finish();
  return gifBlob(gif.bytes());
}

function gifBlob(bytes: Uint8Array): Blob {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new Blob([copy], { type: "image/gif" });
}

export class ScreenRecording {
  readonly startedAt = Date.now();
  private stream: MediaStream;
  private video: HTMLVideoElement | null = null;
  private gif = GIFEncoder();
  private gifCanvas: HTMLCanvasElement | null = null;
  private gifCtx: CanvasRenderingContext2D | null = null;
  private gifFrames = 0;
  private gifNextAt = 0;
  private mp4: Mp4Session | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private gifLoop: number | null = null;
  private stopped = false;
  private limit: ReturnType<typeof setTimeout> | null = null;
  private readonly listener: RecordingListener;
  private readonly stamp = new Date();

  private constructor(stream: MediaStream, listener: RecordingListener) {
    this.stream = stream;
    this.listener = listener;
  }

  static async start(stream: MediaStream, listener: RecordingListener): Promise<ScreenRecording> {
    const session = new ScreenRecording(stream, listener);
    await session.begin();
    return session;
  }

  elapsedMs(): number {
    return Date.now() - this.startedAt;
  }

  async stop(): Promise<CaptureResult> {
    if (this.stopped) throw new Error("This recording has already stopped.");
    this.stopped = true;
    this.teardownClock();
    const durationMs = this.elapsedMs();
    const warnings: string[] = [];
    const track = this.stream.getVideoTracks()[0];
    const settings = track?.getSettings();
    const width = this.video?.videoWidth || settings?.width || 0;
    const height = this.video?.videoHeight || settings?.height || 0;
    let mp4 = null;
    try {
      this.mp4?.close();
      const blob = this.mp4 ? await this.mp4.finalize() : null;
      if (blob && blob.size > 0) mp4 = { blob, bytes: blob.size, filename: recordingFilename("mp4", this.stamp) };
    } catch (cause) {
      warnings.push(cause instanceof Error ? cause.message : "Couldn't finish the MP4.");
    }
    if (!mp4) warnings.push("MP4 encoding isn't available here. GIF is still ready to download.");
    let gif = null;
    try {
      if (this.gifFrames > 0) {
        this.gif.finish();
        const blob = gifBlob(this.gif.bytes());
        gif = { blob, bytes: blob.size, filename: recordingFilename("gif", this.stamp) };
      }
    } catch (cause) {
      warnings.push(cause instanceof Error ? cause.message : "Couldn't finish the GIF.");
    }
    this.cleanup();
    if (!mp4 && !gif) throw new Error("Nothing was captured. Try another screen or window.");
    return { durationMs, width, height, mp4, gif, warnings: warnings.filter((item, i, all) => all.indexOf(item) === i) };
  }

  discard(): void {
    this.stopped = true;
    this.teardownClock();
    this.mp4?.cancel();
    this.cleanup();
  }

  private async begin() {
    const track = this.stream.getVideoTracks()[0];
    if (!track) throw new Error("The chosen screen or window had no video track.");
    track.addEventListener("ended", () => { if (!this.stopped) this.listener.onEnded(); });
    this.video = await attachVideo(this.stream);
    const srcW = this.video.videoWidth || 1280;
    const srcH = this.video.videoHeight || 720;
    const size = gifSize(srcW, srcH);
    const canvas = document.createElement("canvas");
    canvas.width = size.width;
    canvas.height = size.height;
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    if (!ctx) throw new Error("Couldn't prepare GIF frames.");
    this.gifCanvas = canvas;
    this.gifCtx = ctx;
    try {
      this.mp4 = await startMp4(track);
    } catch {
      this.mp4 = null;
    }
    this.gifNextAt = this.startedAt;
    const tickGif = () => {
      if (this.stopped) return;
      const now = Date.now();
      const delay = gifDelayMs(Math.max(now - this.startedAt, 1_000 / 8));
      if (now >= this.gifNextAt && this.gifFrames < GIF_MAX_FRAMES && this.video && this.gifCtx && this.gifCanvas) {
        this.gifCtx.drawImage(this.video, 0, 0, this.gifCanvas.width, this.gifCanvas.height);
        const { data } = this.gifCtx.getImageData(0, 0, this.gifCanvas.width, this.gifCanvas.height);
        writeGifFrame(this.gif, data, this.gifCanvas.width, this.gifCanvas.height, delay, this.gifFrames === 0);
        this.gifFrames += 1;
        this.gifNextAt = now + delay;
      }
      this.gifLoop = requestAnimationFrame(tickGif);
    };
    this.gifLoop = requestAnimationFrame(tickGif);
    this.timer = setInterval(() => this.listener.onTick(this.elapsedMs()), 250);
    this.limit = setTimeout(() => { if (!this.stopped) this.listener.onEnded(); }, MAX_DURATION_MS);
    this.listener.onTick(0);
  }

  private teardownClock() {
    if (this.timer) clearInterval(this.timer);
    if (this.limit) clearTimeout(this.limit);
    if (this.gifLoop != null) cancelAnimationFrame(this.gifLoop);
    this.timer = null;
    this.limit = null;
    this.gifLoop = null;
  }

  private cleanup() {
    stopTracks(this.stream);
    if (this.video) {
      this.video.srcObject = null;
      this.video.remove();
      this.video = null;
    }
    this.gifCanvas = null;
    this.gifCtx = null;
    this.mp4 = null;
  }
}
