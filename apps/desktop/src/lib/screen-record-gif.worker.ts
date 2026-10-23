/// <reference lib="webworker" />
import { GifTimeline, type GifFrame, type GifLimits } from "./screen-record-gif";
import { GIF_MAX_WIDTH, MAX_DURATION_MS } from "./screen-record";

type Request =
  | { type: "init"; limits: GifLimits }
  | { type: "frame"; frame: GifFrame; at: number }
  | { type: "finish"; durationMs: number };
declare const self: DedicatedWorkerGlobalScope;
if (typeof DedicatedWorkerGlobalScope === "undefined" || !(self instanceof DedicatedWorkerGlobalScope)) {
  throw new Error("GIF encoding requires a dedicated worker.");
}
const DEFAULT_LIMITS: GifLimits = { fps: 8, maxBytes: 25 * 1024 * 1024, maxDurationMs: 120_000 };
let timeline: GifTimeline | null = null;

const finite = (value: unknown, min: number, max: number): value is number => typeof value === "number" && Number.isFinite(value) && value >= min && value <= max;

function isRequest(value: unknown): value is Request {
  if (typeof value !== "object" || value === null) return false;
  const request = value as Record<string, unknown>;
  if (request.type === "init") {
    const limits = request.limits as Record<string, unknown> | null;
    return typeof limits === "object" && limits !== null && finite(limits.fps, 1, 30)
      && finite(limits.maxBytes, 1, 128 * 1024 * 1024) && finite(limits.maxDurationMs, 1, MAX_DURATION_MS);
  }
  if (request.type === "finish") return finite(request.durationMs, 0, MAX_DURATION_MS);
  if (request.type !== "frame" || !finite(request.at, 0, MAX_DURATION_MS) || typeof request.frame !== "object" || request.frame === null) return false;
  const frame = request.frame as Record<string, unknown>;
  return typeof frame.width === "number" && Number.isInteger(frame.width) && frame.width >= 1 && frame.width <= GIF_MAX_WIDTH
    && typeof frame.height === "number" && Number.isInteger(frame.height) && frame.height >= 1 && frame.height <= GIF_MAX_WIDTH
    && frame.data instanceof Uint8ClampedArray && frame.data.buffer instanceof ArrayBuffer
    && frame.data.length === frame.width * frame.height * 4;
}

self.onmessage = (event: MessageEvent<unknown>) => {
  // Dedicated workers receive their owner's implicit MessagePort events, whose
  // origin/source default to empty/null. Window messages are not this channel:
  // https://html.spec.whatwg.org/multipage/web-messaging.html#message-port-post-message-steps
  if (!event.isTrusted || event.origin !== "" || event.source !== null || event.ports.length !== 0) return;
  try {
    const request = event.data;
    if (!isRequest(request)) throw new Error("Invalid GIF recording request.");
    if (request.type === "init") {
      timeline = new GifTimeline(request.limits);
      return;
    }
    timeline ??= new GifTimeline(DEFAULT_LIMITS);
    if (request.type === "frame") {
      const progress = timeline.add(request.frame, request.at);
      self.postMessage({ type: "frame", ...progress });
    } else {
      const bytes = timeline.finish(request.durationMs);
      self.postMessage({ type: "done", bytes }, { transfer: [bytes.buffer] });
    }
  } catch (cause) {
    self.postMessage({ type: "error", message: cause instanceof Error ? cause.message : "Couldn't encode the GIF." });
  }
};
