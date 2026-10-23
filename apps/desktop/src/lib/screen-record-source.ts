import { MediaStreamVideoTrackSource, VideoSample, VideoSampleSource, type VideoEncodingConfig, type VideoSource } from "mediabunny";

/** The longest stretch one frame may represent. A longer stall is skipped, not replayed. */
export const MAX_STALL_MS = 250;
/** Frames kept for the encoder: the newest two, so a slow encoder drops frames instead of buffering them. */
const FRAME_QUEUE = 2;

/** What the recorder needs from a screen track source, whichever way the frames arrive. */
export type TrackSource = {
  source: VideoSource;
  /** Starts feeding frames; call after the output has started. */
  begin: () => void;
  pause: () => void;
  resume: () => void;
  close: () => void;
  errorPromise: Promise<void>;
};

type Processor = new (init: { track: MediaStreamVideoTrack; maxBufferSize: number }) => { readable: ReadableStream<VideoFrame> };

/**
 * Feeds a screen track to the encoder at a steady rate.
 *
 * mediabunny's own track source replays every missed frame after the page stalls (laptop sleep,
 * heavy load, a debugger). Measured on Chromium, a 20 s stall held about 4 GB of frames and a
 * 40 s one crashed the page. This source stamps each frame with the time actually recorded and
 * counts any stall as at most MAX_STALL_MS, so a stall costs a frozen moment, not memory.
 * Browsers without MediaStreamTrackProcessor keep mediabunny's own source.
 */
export function createTrackSource(track: MediaStreamVideoTrack, config: VideoEncodingConfig, frameRate: number): TrackSource {
  const Processor = (globalThis as { MediaStreamTrackProcessor?: Processor }).MediaStreamTrackProcessor;
  if (!Processor) {
    const source = new MediaStreamVideoTrackSource(track, config, { frameRate });
    return { source, begin: () => undefined, pause: () => source.pause(), resume: () => source.resume(), close: () => source.close(), errorPromise: source.errorPromise };
  }
  const sink = new VideoSampleSource(config);
  let fail: (cause: unknown) => void = () => undefined;
  const errorPromise = new Promise<void>((_, reject) => { fail = reject; });
  errorPromise.catch(() => undefined);
  let latest: VideoFrame | null = null;
  let paused = false;
  let stopped = false;
  let busy = false;
  let mediaMs = 0;
  let lastTick = 0;
  let timer: ReturnType<typeof setInterval> | undefined;
  let reader: ReadableStreamDefaultReader<VideoFrame> | undefined;

  const emit = () => {
    const now = performance.now();
    const elapsed = now - lastTick;
    lastTick = now;
    if (stopped || paused || !latest) return;
    mediaMs += Math.min(elapsed, MAX_STALL_MS);
    if (busy) return;
    busy = true;
    const frame = new VideoFrame(latest, { timestamp: Math.round(mediaMs * 1000), duration: Math.round(1_000_000 / frameRate) });
    const sample = new VideoSample(frame);
    sink.add(sample).catch((cause) => { if (!stopped) fail(cause); }).finally(() => { sample.close(); busy = false; });
  };
  const read = async () => {
    try {
      for (;;) {
        const next = await reader!.read();
        if (next.done) return;
        latest?.close();
        latest = next.value;
      }
    } catch (cause) {
      if (!stopped) fail(cause);
    }
  };
  const close = () => {
    if (stopped) return;
    stopped = true;
    clearInterval(timer);
    void reader?.cancel().catch(() => undefined);
    latest?.close();
    latest = null;
    sink.close();
  };
  return {
    source: sink,
    begin() {
      reader = new Processor({ track, maxBufferSize: FRAME_QUEUE }).readable.getReader();
      lastTick = performance.now();
      timer = setInterval(emit, 1000 / frameRate);
      void read();
    },
    pause() { paused = true; },
    resume() { paused = false; lastTick = performance.now(); },
    close,
    errorPromise,
  };
}
