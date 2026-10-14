/// <reference lib="webworker" />
import { GifTimeline, type GifFrame } from "./screen-record-gif";
import { GIF_MAX_WIDTH, MAX_DURATION_MS } from "./screen-record";

type Request = { type: "frame"; frame: GifFrame; at: number } | { type: "finish"; durationMs: number };
declare const self: DedicatedWorkerGlobalScope;
if (typeof DedicatedWorkerGlobalScope === "undefined" || !(self instanceof DedicatedWorkerGlobalScope)) {
  throw new Error("GIF encoding requires a dedicated worker.");
}
const timeline = new GifTimeline();

function isRequest(value: unknown): value is Request {
  if (typeof value !== "object" || value === null) return false;
  const request = value as Record<string, unknown>;
  if (request.type === "finish") {
    return typeof request.durationMs === "number" && Number.isFinite(request.durationMs)
      && request.durationMs >= 0 && request.durationMs <= MAX_DURATION_MS;
  }
  if (request.type !== "frame" || typeof request.at !== "number" || !Number.isFinite(request.at)
    || request.at < 0 || request.at > MAX_DURATION_MS || typeof request.frame !== "object" || request.frame === null) return false;
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
    if (request.type === "frame") {
      timeline.add(request.frame, request.at);
      self.postMessage({ type: "frame", intervalMs: timeline.intervalMs });
    } else {
      const bytes = timeline.finish(request.durationMs);
      self.postMessage({ type: "done", bytes }, { transfer: [bytes.buffer] });
    }
  } catch (cause) {
    self.postMessage({ type: "error", message: cause instanceof Error ? cause.message : "Couldn't encode the GIF." });
  }
};
