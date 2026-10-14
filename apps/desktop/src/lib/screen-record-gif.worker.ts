import { GifTimeline, type GifFrame } from "./screen-record-gif";

type Request = { type: "frame"; frame: GifFrame; at: number } | { type: "finish"; durationMs: number };
const timeline = new GifTimeline();

self.onmessage = (event: MessageEvent<Request>) => {
  try {
    if (event.data.type === "frame") {
      timeline.add(event.data.frame, event.data.at);
      self.postMessage({ type: "frame", intervalMs: timeline.intervalMs });
    } else {
      const bytes = timeline.finish(event.data.durationMs);
      self.postMessage({ type: "done", bytes }, { transfer: [bytes.buffer] });
    }
  } catch (cause) {
    self.postMessage({ type: "error", message: cause instanceof Error ? cause.message : "Couldn't encode the GIF." });
  }
};
