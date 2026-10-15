import {
  BufferTarget,
  Quality,
  Mp4OutputFormat,
  MediaStreamVideoTrackSource,
  Output,
  canEncodeVideo,
} from "mediabunny";
import {
  MAX_DURATION_MS,
  gifSize,
  recordingFilename,
  stopTracks,
  type CaptureResult,
  type RecordingListener,
} from "./screen-record";

const MAX_MP4_BYTES = 128 * 1024 * 1024;
const FINALIZE_TIMEOUT_MS = 20_000;

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
  return new Promise((resolve, reject) => {
    let settled = false;
    const detach = () => {
      clearTimeout(timer);
      video.removeEventListener("error", fail);
      video.removeEventListener("loadedmetadata", loaded);
      stream.getVideoTracks()[0]?.removeEventListener("ended", fail);
    };
    const fail = () => {
      if (settled) return;
      settled = true;
      detach();
      video.srcObject = null;
      video.remove();
      reject(new Error("The captured surface never started playing."));
    };
    const timer = setTimeout(fail, 8_000);
    const loaded = () => {
      void video.play().then(() => {
        if (settled) return;
        settled = true;
        detach();
        resolve(video);
      }).catch(fail);
    };
    video.addEventListener("error", fail, { once: true });
    video.addEventListener("loadedmetadata", loaded, { once: true });
    stream.getVideoTracks()[0]?.addEventListener("ended", fail, { once: true });
    video.srcObject = stream;
    document.body.append(video);
    if (stream.getVideoTracks()[0]?.readyState === "ended") fail();
  });
}

async function startWebCodecsMp4(track: MediaStreamVideoTrack, signal: AbortSignal): Promise<Mp4Session | null> {
  const settings = track.getSettings();
  const scale = Math.min(1, 1920 / (settings.width || 1280), 1080 / (settings.height || 720));
  const width = even((settings.width || 1280) * scale, 1280);
  const height = even((settings.height || 720) * scale, 720);
  const quality = new Quality({ bitrate: 5_000_000 });
  if (!await canEncodeVideo("avc", { width, height, frameRate: 30, quality })) return null;
  if (signal.aborted) return null;
  const target = new BufferTarget();
  const output = new Output({ format: new Mp4OutputFormat(), target });
  let failure: unknown = null;
  let encodedBytes = 0;
  const source = new MediaStreamVideoTrackSource(track, {
    codec: "avc",
    quality,
    latencyMode: "realtime",
    contentHint: "detail",
    sizeChangeBehavior: "contain",
    transform: { width, height },
    onEncodedPacket(packet) {
      encodedBytes += packet.byteLength;
      // Leave room for MP4 metadata and indexes in addition to encoded packets.
      if (encodedBytes > MAX_MP4_BYTES - 1024 * 1024) throw new Error("The MP4 exceeded the 128 MiB recording limit.");
    },
  }, { frameRate: 30 });
  void source.errorPromise.catch((cause) => {
    failure = cause;
    source.close();
    void output.cancel().catch(() => undefined);
  });
  output.addVideoTrack(source);
  const cancel = () => { source.close(); void output.cancel().catch(() => undefined); };
  signal.addEventListener("abort", cancel, { once: true });
  try {
    if (signal.aborted) { cancel(); return null; }
    await output.start();
    if (signal.aborted) { cancel(); return null; }
  } catch (cause) {
    source.close();
    await output.cancel().catch(() => undefined);
    throw cause;
  } finally {
    signal.removeEventListener("abort", cancel);
  }
  return {
    close: () => source.close(),
    cancel: () => { source.close(); void output.cancel().catch(() => undefined); },
    async finalize() {
      if (failure) throw failure;
      await output.finalize();
      if (failure) throw failure;
      const buffer = target.buffer;
      if (buffer && buffer.byteLength > MAX_MP4_BYTES) throw new Error("The MP4 exceeded the 128 MiB recording limit.");
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
  let failure: Error | null = null;
  let cancelled = false;
  let bytes = 0;
  const blob = () => chunks.length ? new Blob(chunks, { type: "video/mp4" }) : null;
  const stop = () => { if (recorder.state !== "inactive") recorder.stop(); };
  const completed = new Promise<Blob | null>((resolve, reject) => {
    recorder.ondataavailable = (event) => {
      if (cancelled || failure || !event.data.size) return;
      bytes += event.data.size;
      if (bytes > MAX_MP4_BYTES) {
        failure = new Error("The MP4 exceeded the 128 MiB recording limit.");
        chunks.length = 0;
        stop();
      } else chunks.push(event.data);
    };
    recorder.onerror = () => {
      failure = new Error("The MP4 encoder stopped unexpectedly. The GIF may still be available.");
      chunks.length = 0;
      stop();
      reject(failure);
    };
    recorder.onstop = () => {
      if (failure) reject(failure);
      else resolve(cancelled ? null : blob());
      chunks.length = 0;
    };
  });
  // A recorder can fail before Stop is clicked; keep that failure for finalize().
  void completed.catch(() => undefined);
  recorder.start(400);
  return {
    close() {},
    cancel() { cancelled = true; chunks.length = 0; stop(); },
    finalize() {
      stop();
      return completed;
    },
  };
}

async function startMp4(track: MediaStreamVideoTrack): Promise<Mp4Session | null> {
  const controller = new AbortController();
  const ended = () => controller.abort();
  const timeout = setTimeout(ended, 8_000);
  track.addEventListener("ended", ended, { once: true });
  try {
    const aborted = new Promise<null>((resolve) => controller.signal.addEventListener("abort", () => resolve(null), { once: true }));
    const encoded = await Promise.race([startWebCodecsMp4(track, controller.signal), aborted]);
    if (encoded) return encoded;
  } catch {
    // Safari and some webviews still record MP4 through MediaRecorder.
  } finally {
    clearTimeout(timeout);
    track.removeEventListener("ended", ended);
  }
  if (track.readyState === "ended") return null;
  return startRecorderMp4(track);
}

function gifBlob(bytes: Uint8Array): Blob {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return new Blob([copy], { type: "image/gif" });
}

function withDeadline<T>(promise: Promise<T>, onTimeout: () => void): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      onTimeout();
      reject(new Error("Preparing this recording took too long. Try a shorter recording."));
    }, FINALIZE_TIMEOUT_MS);
    promise.then(resolve, reject).finally(() => clearTimeout(timer));
  });
}

class GifWorkerSession {
  private worker = new Worker(new URL("./screen-record-gif.worker.ts", import.meta.url), { type: "module" });
  private busy = false;
  private failure: Error | null = null;
  private resolve: ((blob: Blob) => void) | null = null;
  private reject: ((cause: Error) => void) | null = null;
  intervalMs = 125;

  constructor() {
    this.worker.onmessage = (event: MessageEvent) => {
      if (event.data.type === "frame") {
        this.busy = false;
        this.intervalMs = event.data.intervalMs;
      } else if (event.data.type === "done") this.resolve?.(gifBlob(event.data.bytes));
      else if (event.data.type === "error") this.fail(new Error(event.data.message));
    };
    this.worker.onerror = () => this.fail(new Error("Couldn't prepare the GIF. The MP4 may still be available."));
    this.worker.onmessageerror = () => this.fail(new Error("Couldn't read the GIF encoder's result."));
  }

  get ready(): boolean { return !this.busy && !this.failure; }

  add(data: Uint8ClampedArray, width: number, height: number, at: number): void {
    if (!this.ready) return;
    this.busy = true;
    this.worker.postMessage({ type: "frame", frame: { data, width, height }, at }, [data.buffer]);
  }

  finish(durationMs: number): Promise<Blob> {
    if (this.failure) return Promise.reject(this.failure);
    return new Promise((resolve, reject) => {
      this.resolve = resolve;
      this.reject = reject;
      // Worker messages are ordered: finish follows the last in-flight frame.
      this.worker.postMessage({ type: "finish", durationMs });
    });
  }

  cancel(): void {
    this.worker.terminate();
    this.fail(new Error("Recording discarded."));
  }

  private fail(cause: Error): void {
    this.failure = cause;
    this.reject?.(cause);
  }
}

export class ScreenRecording {
  private startedAt = performance.now();
  private stream: MediaStream;
  private video: HTMLVideoElement | null = null;
  private gif: GifWorkerSession | null = null;
  private gifCanvas: HTMLCanvasElement | null = null;
  private gifCtx: CanvasRenderingContext2D | null = null;
  private gifNextAt = 0;
  private mp4: Mp4Session | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private gifLoop: ReturnType<typeof setTimeout> | null = null;
  private stopped = false;
  private discarded = false;
  private gifFailure: string | null = null;
  private endedTrack: MediaStreamTrack | null = null;
  private readonly onTrackEnded = () => { if (!this.stopped) this.listener.onEnded(); };
  private limit: ReturnType<typeof setTimeout> | null = null;
  private readonly listener: RecordingListener;
  private readonly stamp = new Date();

  private constructor(stream: MediaStream, listener: RecordingListener) {
    this.stream = stream;
    this.listener = listener;
  }

  static async start(stream: MediaStream, listener: RecordingListener): Promise<ScreenRecording> {
    const session = new ScreenRecording(stream, listener);
    try { await session.begin(); }
    catch (cause) { session.discard(); throw cause; }
    return session;
  }

  elapsedMs(): number {
    return Math.max(0, performance.now() - this.startedAt);
  }

  async stop(): Promise<CaptureResult> {
    if (this.stopped) throw new Error("This recording has already stopped.");
    this.stopped = true;
    this.teardownClock();
    const durationMs = Math.min(MAX_DURATION_MS, this.elapsedMs());
    const warnings: string[] = [];
    const track = this.stream.getVideoTracks()[0];
    const settings = track?.getSettings();
    const width = this.video?.videoWidth || settings?.width || 0;
    const height = this.video?.videoHeight || settings?.height || 0;
    let mp4 = null;
    let gif = null;
    const mp4Session = this.mp4;
    const gifSession = this.gif;
    try {
      mp4Session?.close();
      const mp4Ready = mp4Session ? withDeadline(mp4Session.finalize(), () => mp4Session.cancel()) : Promise.resolve(null);
      const gifReady = gifSession ? withDeadline(gifSession.finish(durationMs), () => gifSession.cancel()) : Promise.resolve(null);
      // Stop the privacy indicator immediately, while already captured frames finish encoding.
      stopTracks(this.stream);
      const [mp4Result, gifResult] = await Promise.allSettled([mp4Ready, gifReady]);
      if (this.discarded) throw new Error("Recording discarded.");
      if (mp4Result.status === "fulfilled" && mp4Result.value?.size) {
        const blob = mp4Result.value;
        mp4 = { blob, bytes: blob.size, filename: recordingFilename("mp4", this.stamp) };
      } else {
        if (mp4Result.status === "rejected") warnings.push(mp4Result.reason instanceof Error ? mp4Result.reason.message : "Couldn't finish the MP4.");
        warnings.push("MP4 encoding isn't available here.");
      }
      if (gifResult.status === "fulfilled" && gifResult.value?.size) {
        const blob = gifResult.value;
        gif = { blob, bytes: blob.size, filename: recordingFilename("gif", this.stamp) };
      } else if (gifResult.status === "rejected") warnings.push(gifResult.reason instanceof Error ? gifResult.reason.message : "Couldn't finish the GIF.");
      if (this.gifFailure) warnings.push(this.gifFailure);
    } finally {
      this.cleanup();
    }
    if (!mp4 && !gif) throw new Error("Nothing was captured. Try another screen or window.");
    return { durationMs, width, height, mp4, gif, warnings: warnings.filter((item, i, all) => all.indexOf(item) === i) };
  }

  discard(): void {
    this.discarded = true;
    this.stopped = true;
    this.teardownClock();
    this.mp4?.cancel();
    this.cleanup();
  }

  private async begin() {
    const track = this.stream.getVideoTracks()[0];
    if (!track) throw new Error("The chosen screen or window had no video track.");
    this.endedTrack = track;
    track.addEventListener("ended", this.onTrackEnded);
    this.video = await attachVideo(this.stream);
    const srcW = this.video.videoWidth || 1280;
    const srcH = this.video.videoHeight || 720;
    const size = gifSize(srcW, srcH);
    try {
      this.gif = new GifWorkerSession();
      const canvas = document.createElement("canvas");
      canvas.width = size.width;
      canvas.height = size.height;
      const ctx = canvas.getContext("2d", { willReadFrequently: true });
      if (!ctx) throw new Error("Couldn't prepare GIF frames.");
      this.gifCanvas = canvas;
      this.gifCtx = ctx;
    } catch {
      this.gif?.cancel();
      this.gif = null;
      this.gifFailure = "GIF encoding isn't available here.";
    }
    try {
      this.mp4 = await startMp4(track);
    } catch {
      this.mp4 = null;
    }
    if (!this.gif && !this.mp4) throw new Error("This window cannot create an MP4 or GIF recording.");
    if (track.readyState === "ended") throw new Error("Screen sharing ended before recording could start. Choose a screen again.");
    this.startedAt = performance.now();
    this.gifNextAt = 0;
    const tickGif = () => {
      if (this.stopped) return;
      const now = this.elapsedMs();
      if (now >= this.gifNextAt && this.gif?.ready && this.video && this.gifCtx && this.gifCanvas) {
        try {
          this.gifCtx.drawImage(this.video, 0, 0, this.gifCanvas.width, this.gifCanvas.height);
          const { data } = this.gifCtx.getImageData(0, 0, this.gifCanvas.width, this.gifCanvas.height);
          this.gif.add(data, this.gifCanvas.width, this.gifCanvas.height, Math.min(now, MAX_DURATION_MS));
          this.gifNextAt = now + this.gif.intervalMs;
        } catch {
          this.gif.cancel();
          this.gif = null;
          this.gifFailure = "Couldn't capture GIF frames. The MP4 may still be available.";
        }
      }
      // rAF stops for hidden windows; a timer continues sampling while another app is focused.
      this.gifLoop = setTimeout(tickGif, this.gif?.intervalMs ?? 500);
    };
    tickGif();
    this.timer = setInterval(() => this.listener.onTick(this.elapsedMs()), 250);
    this.limit = setTimeout(() => { if (!this.stopped) this.listener.onEnded(); }, MAX_DURATION_MS);
    this.listener.onTick(0);
  }

  private teardownClock() {
    if (this.timer) clearInterval(this.timer);
    if (this.limit) clearTimeout(this.limit);
    if (this.gifLoop != null) clearTimeout(this.gifLoop);
    this.timer = null;
    this.limit = null;
    this.gifLoop = null;
  }

  private cleanup() {
    stopTracks(this.stream);
    this.endedTrack?.removeEventListener("ended", this.onTrackEnded);
    this.endedTrack = null;
    this.gif?.cancel();
    this.gif = null;
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
