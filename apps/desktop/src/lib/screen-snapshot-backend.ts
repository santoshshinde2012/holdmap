import { requestDisplayStream, stopTracks } from "./screen-record";
import type { RecordingSupport } from "./screen-record-backend";
import { MAX_SNAPSHOT_HEIGHT, MAX_SNAPSHOT_WIDTH, captureVideoSnapshot, snapshotCancelled, snapshotWait, type SnapshotResult } from "./screen-snapshot";

/** Acquires one explicit source and releases it after a single PNG, including late cancellation. */
export async function captureScreenSnapshot(backend: NonNullable<RecordingSupport["backend"]>, signal: AbortSignal): Promise<SnapshotResult> {
  if (signal.aborted) throw snapshotCancelled();
  if (backend === "native") {
    const { captureNativeSnapshot } = await import("./screen-snapshot-native");
    return captureNativeSnapshot(signal);
  }
  // Browser permission requires this request in the user's click turn, before any await.
  const requested = requestDisplayStream({ width: MAX_SNAPSHOT_WIDTH, height: MAX_SNAPSHOT_HEIGHT });
  let stream: MediaStream | null = null;
  let abandoned = false;
  const release = () => { if (stream) { stopTracks(stream); stream = null; } };
  const abort = () => { abandoned = true; release(); };
  signal.addEventListener("abort", abort, { once: true });
  let video: HTMLVideoElement | null = null;
  try {
    const selected = await snapshotWait(requested.then((value) => {
      if (abandoned || signal.aborted) { stopTracks(value); throw snapshotCancelled(); }
      return value;
    }), signal, 120_000, "Sharing took too long. Try again.");
    stream = selected;
    if (signal.aborted) throw snapshotCancelled();
    if (!selected.getVideoTracks().some((track) => track.readyState === "live")) throw new Error("Screen sharing ended before a snapshot was available.");
    video = document.createElement("video");
    video.muted = true; video.playsInline = true; video.autoplay = true;
    video.setAttribute("aria-hidden", "true");
    video.style.cssText = "position:fixed;left:-9999px;width:1px;height:1px;opacity:0;pointer-events:none";
    video.srcObject = selected;
    document.body.append(video);
    await snapshotWait(video.play(), signal, 8_000, "The selected source did not start playing.");
    return await captureVideoSnapshot(video, signal, "screen");
  } finally {
    abandoned = true;
    signal.removeEventListener("abort", abort);
    release();
    if (video) { video.srcObject = null; video.remove(); }
  }
}
