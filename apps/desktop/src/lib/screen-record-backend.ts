import { captureSupport, requestDisplayStream, stopTracks, type RecordingListener, type RecordingSession } from "./screen-record";
export type { RecordingSession } from "./screen-record";

export type RecordingSupport = {
  backend: "browser" | "native" | null;
  pending: boolean;
  reason: string | null;
};

const unavailable = "This window does not offer screen sharing. Try a browser that supports it, or use your operating system's screen recorder.";

export function initialRecordingSupport(): RecordingSupport {
  if (captureSupport().display) return { backend: "browser", pending: false, reason: null };
  const native = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  return { backend: null, pending: native, reason: native ? null : unavailable };
}

/** Capability discovery never opens a picker or requests screen permission. */
export async function resolveRecordingSupport(): Promise<RecordingSupport> {
  const support = initialRecordingSupport();
  if (!support.pending) return support;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const native = await Promise.race([
      invoke<{ available: boolean; reason: string | null }>("screen_recording_capabilities"),
      new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("Capability check timed out.")), 10_000); }),
    ]);
    return native?.available === true
      ? { backend: "native", pending: false, reason: null }
      : { backend: null, pending: false, reason: typeof native?.reason === "string" && native.reason ? native.reason : unavailable };
  } catch {
    return { backend: null, pending: false, reason: "Couldn't check native screen recording support. Reopen Record screen to retry." };
  } finally {
    clearTimeout(timer);
  }
}

export async function startRecording(
  backend: NonNullable<RecordingSupport["backend"]>,
  listener: RecordingListener,
  signal: AbortSignal,
  onPreparing: () => void,
): Promise<RecordingSession> {
  if (signal.aborted) throw new DOMException("Recording cancelled.", "AbortError");
  if (backend === "native") {
    const { NativeRecordingSession } = await import("./screen-record-native");
    return NativeRecordingSession.start(listener, signal, onPreparing);
  }
  // Call before the first await/import: browsers require the user's click activation.
  const requested = requestDisplayStream();
  let stream: MediaStream | null = null;
  let released = false;
  const release = () => {
    if (stream && !released) { released = true; stopTracks(stream); }
  };
  signal.addEventListener("abort", release, { once: true });
  try {
    stream = await requested;
    if (signal.aborted) throw new DOMException("Recording cancelled.", "AbortError");
    onPreparing();
    const { ScreenRecording } = await import("./screen-record-encode");
    if (signal.aborted) throw new DOMException("Recording cancelled.", "AbortError");
    const session = await ScreenRecording.start(stream, listener);
    if (signal.aborted) { session.discard(); throw new DOMException("Recording cancelled.", "AbortError"); }
    if (stream.getVideoTracks().every((track) => track.readyState === "ended")) listener.onEnded();
    return session;
  } catch (cause) {
    release();
    throw cause;
  } finally {
    signal.removeEventListener("abort", release);
  }
}
