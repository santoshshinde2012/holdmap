// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NativeRecordingSession, type NativeRecordingStatus } from "./screen-record-native";
import { LEGACY_PLAN, PRESETS } from "./screen-record-plan";
import { Blob as NodeBlob } from "node:buffer";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const status = (patch: Partial<NativeRecordingStatus> = {}): NativeRecordingStatus => ({
  sessionId: "owned-session", phase: "capturing", elapsedMs: 1_500, width: 320, height: 180,
  mp4Bytes: 4, gifBytes: 3, warnings: [], error: null, ...patch,
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}
const sessions: NativeRecordingSession[] = [];
async function start(controller = new AbortController(), listener = { onTick: vi.fn(), onEnded: vi.fn(), onLimit: vi.fn() }, plan = LEGACY_PLAN) {
  const session = await NativeRecordingSession.start(listener, controller.signal, vi.fn(), plan);
  sessions.push(session);
  return { session, controller, listener };
}

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockImplementation(async (command: string, args?: { format?: string }) => {
    if (command === "start_screen_recording") return status();
    if (command === "stop_screen_recording" || command === "screen_recording_status") return status({ phase: "ready" });
    if (command === "read_screen_recording") return new ArrayBuffer(args?.format === "mp4" ? 4 : 3);
  });
});
afterEach(() => {
  for (const session of sessions.splice(0)) session.discard();
  vi.clearAllTimers();
  vi.useRealTimers();
  invoke.mockReset();
});

describe("native recording session", () => {
  it("takes a PNG without stopping video and waits for an in-flight status operation", async () => {
    vi.stubGlobal("Blob", NodeBlob);
    const pixels = Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg=="), (c) => c.charCodeAt(0)).buffer;
    const readStatus = deferred<NativeRecordingStatus>();
    const snapshotId = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    invoke.mockImplementation(async (command) => {
      if (command === "start_screen_recording") return status();
      if (command === "screen_recording_status") return readStatus.promise;
      if (command === "start_screen_snapshot") return { snapshotId, phase: "ready", width: 1, height: 1, pngBytes: pixels.byteLength, error: null };
      if (command === "read_screen_snapshot") return pixels;
    });
    try {
      const { session, listener } = await start();
      await vi.advanceTimersByTimeAsync(250);
      const snapshot = session.captureSnapshot(new AbortController().signal);
      await vi.advanceTimersByTimeAsync(0);
      expect(invoke.mock.calls.some(([command]) => command === "start_screen_snapshot")).toBe(false);
      readStatus.resolve(status());
      expect(await snapshot).toMatchObject({ source: "recording", width: 1, height: 1 });
      expect(invoke).toHaveBeenCalledWith("start_screen_snapshot", { sessionId: "owned-session" });
      expect(invoke).toHaveBeenCalledWith("discard_screen_snapshot", { snapshotId });
      expect(invoke.mock.calls.some(([command]) => command === "stop_screen_recording" || command === "discard_screen_recording")).toBe(false);
      expect(listener.onEnded).not.toHaveBeenCalled();
      expect(vi.getTimerCount()).toBe(1); // exactly the resumed video status poll
    } finally { vi.unstubAllGlobals(); }
  });

  it("cancels pending PNG work before Stop without converting a PNG failure into video failure", async () => {
    const pngRead = deferred<ArrayBuffer>(), cleaned = deferred<void>();
    const snapshotId = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    invoke.mockImplementation(async (command: string, args?: { format?: string }) => {
      if (command === "start_screen_recording") return status();
      if (command === "start_screen_snapshot") return { snapshotId, phase: "ready", width: 1, height: 1, pngBytes: 70, error: null };
      if (command === "read_screen_snapshot") return pngRead.promise;
      if (command === "discard_screen_snapshot") return cleaned.promise;
      if (command === "stop_screen_recording" || command === "screen_recording_status") return status({ phase: "ready" });
      if (command === "read_screen_recording") return new ArrayBuffer(args?.format === "mp4" ? 4 : 3);
    });
    const { session } = await start();
    const snapshot = session.captureSnapshot(new AbortController().signal);
    const rejected = expect(snapshot).rejects.toMatchObject({ name: "AbortError" });
    await vi.advanceTimersByTimeAsync(0);
    const stopped = session.stop();
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenCalledWith("discard_screen_snapshot", { snapshotId });
    expect(invoke.mock.calls.some(([command]) => command === "stop_screen_recording")).toBe(false);
    cleaned.resolve(); await rejected;
    expect(await stopped).toMatchObject({ mp4: { bytes: 4 }, gif: { bytes: 3 } });
    expect(invoke).toHaveBeenCalledWith("stop_screen_recording", { sessionId: "owned-session" });
    pngRead.resolve(new ArrayBuffer(70));
    await vi.advanceTimersByTimeAsync(0);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("waits for source selection before starting, then reads exports and deletes native files", async () => {
    let polls = 0;
    invoke.mockImplementation(async (command: string, args?: { format?: string }) => {
      if (command === "start_screen_recording") return status({ phase: "choosing" });
      if (command === "screen_recording_status") return status({ phase: ++polls === 1 ? "capturing" : "ready" });
      if (command === "stop_screen_recording") return status({ phase: "finalizing" });
      if (command === "read_screen_recording") return new ArrayBuffer(args?.format === "mp4" ? 4 : 3);
    });
    const preparing = vi.fn();
    const pending = NativeRecordingSession.start({ onTick: vi.fn(), onEnded: vi.fn() }, new AbortController().signal, preparing);
    await vi.advanceTimersByTimeAsync(249);
    expect(preparing).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    const session = await pending;
    sessions.push(session);
    expect(preparing).toHaveBeenCalledOnce();
    const stopped = session.stop();
    await vi.advanceTimersByTimeAsync(250);
    const result = await stopped;
    expect(result).toMatchObject({ durationMs: 1_500, width: 320, height: 180, mp4: { bytes: 4 }, gif: { bytes: 3 } });
    expect(result.mp4?.blob.type).toBe("video/mp4");
    expect(result.gif?.blob.type).toBe("image/gif");
    expect(invoke.mock.calls.at(-1)?.[0]).toBe("discard_screen_recording");
    expect(invoke).toHaveBeenCalledWith("read_screen_recording", { sessionId: "owned-session", format: "mp4" });
    expect(session.stop()).toBe(stopped);
  });

  it("discards an immediate start acknowledgement that arrives after cancellation", async () => {
    const request = deferred<NativeRecordingStatus>();
    invoke.mockImplementation((command: string) => command === "start_screen_recording" ? request.promise : Promise.resolve());
    const controller = new AbortController();
    const pending = NativeRecordingSession.start({ onTick() {}, onEnded() {} }, controller.signal, vi.fn());
    const rejected = expect(pending).rejects.toThrow("cancelled");
    controller.abort();
    await rejected;
    request.resolve(status({ phase: "choosing" }));
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "owned-session" });
    expect(vi.getTimerCount()).toBe(0);
  });

  it.each([
    { phase: "choosing", handoff: 1 },
    { phase: "choosing", handoff: 2 },
    { phase: "capturing", handoff: 1 },
    { phase: "capturing", handoff: 2 },
  ] as const)("releases an acknowledged $phase session cancelled during handoff $handoff", async ({ phase, handoff }) => {
    const request = deferred<NativeRecordingStatus>();
    invoke.mockImplementation((command: string) => command === "start_screen_recording" ? request.promise : Promise.resolve());
    const controller = new AbortController();
    const added = vi.spyOn(controller.signal, "addEventListener");
    const removed = vi.spyOn(controller.signal, "removeEventListener");
    const listener = { onTick: vi.fn(), onEnded: vi.fn() };
    const preparing = vi.fn();
    let outcome: "pending" | "resolved" | "rejected" = "pending";
    let failure: unknown;
    const pending = NativeRecordingSession.start(listener, controller.signal, preparing).then(
      (session) => { sessions.push(session); outcome = "resolved"; },
      (cause) => { failure = cause; outcome = "rejected"; },
    );
    request.resolve(status({ phase }));
    // Cancel after acknowledgement validation, on either side of the promise
    // handoff to start(). No real picker or capture is involved.
    const abortAfter = (turns: number) => queueMicrotask(() => {
      if (turns) abortAfter(turns - 1);
      else controller.abort();
    });
    abortAfter(handoff);
    await vi.advanceTimersByTimeAsync(0);
    expect(outcome).toBe("rejected");
    await pending;
    expect(failure).toMatchObject({ name: "AbortError" });
    expect(preparing).not.toHaveBeenCalled();
    expect(listener.onTick).not.toHaveBeenCalled();
    expect(listener.onEnded).not.toHaveBeenCalled();
    expect(invoke.mock.calls.filter(([command]) => command === "discard_screen_recording")).toEqual([
      ["discard_screen_recording", { sessionId: "owned-session" }],
    ]);
    for (const [event, handler] of added.mock.calls) {
      if (event === "abort") expect(removed.mock.calls.some(([name, callback]) => name === event && callback === handler)).toBe(true);
    }
    expect(vi.getTimerCount()).toBe(0);
  });

  it("cancels a pending source picker and clears its polling timer", async () => {
    invoke.mockImplementation(async (command: string) => command === "start_screen_recording" ? status({ phase: "choosing" }) : undefined);
    const controller = new AbortController();
    const pending = NativeRecordingSession.start({ onTick() {}, onEnded() {} }, controller.signal, vi.fn());
    const rejected = expect(pending).rejects.toThrow("cancelled");
    await vi.advanceTimersByTimeAsync(0);
    controller.abort();
    await rejected;
    expect(invoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "owned-session" });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("reports a native two-minute completion once and skips a redundant stop command", async () => {
    const { session, listener } = await start();
    invoke.mockImplementation(async (command: string, args?: { format?: string }) => {
      if (command === "screen_recording_status") return status({ phase: "ready", elapsedMs: LEGACY_PLAN.maxDurationMs + 100 });
      if (command === "read_screen_recording") return new ArrayBuffer(args?.format === "mp4" ? 4 : 3);
    });
    await vi.advanceTimersByTimeAsync(1_000);
    expect(listener.onEnded).toHaveBeenCalledOnce();
    expect(listener.onLimit).toHaveBeenCalledExactlyOnceWith("duration");
    expect(session.elapsedMs()).toBe(LEGACY_PLAN.maxDurationMs);
    const result = await session.stop();
    expect(result.durationMs).toBe(LEGACY_PLAN.maxDurationMs);
    expect(invoke.mock.calls.some(([command]) => command === "stop_screen_recording")).toBe(false);
  });

  it("retains a successful format when the other export cannot be read", async () => {
    const { session } = await start();
    invoke.mockImplementation(async (command: string, args?: { format?: string }) => {
      if (command === "stop_screen_recording") return status({ phase: "ready" });
      if (command === "read_screen_recording") {
        if (args?.format === "gif") throw new Error("GIF export failed");
        return new ArrayBuffer(4);
      }
    });
    expect(await session.stop()).toMatchObject({ mp4: { bytes: 4 }, gif: null, warnings: ["GIF export failed"] });
    expect(invoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "owned-session" });
  });

  it("waits for an in-flight capture poll before Stop and ignores its stale metadata", async () => {
    const poll = deferred<NativeRecordingStatus>();
    const mp4 = deferred<ArrayBuffer>();
    const { session } = await start();
    invoke.mockImplementation((command: string, args?: { format?: string }) => {
      if (command === "screen_recording_status") return poll.promise;
      if (command === "stop_screen_recording") return Promise.resolve(status({ phase: "ready" }));
      if (command === "read_screen_recording") return args?.format === "mp4" ? mp4.promise : Promise.resolve(new ArrayBuffer(3));
      return Promise.resolve();
    });
    await vi.advanceTimersByTimeAsync(250);
    const stopped = session.stop();
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke.mock.calls.some(([command]) => command === "stop_screen_recording")).toBe(false);
    poll.resolve(status({ phase: "capturing", elapsedMs: 500, mp4Bytes: 0, gifBytes: 0 }));
    await vi.advanceTimersByTimeAsync(0);
    mp4.resolve(new ArrayBuffer(4));
    expect(await stopped).toMatchObject({ durationMs: 1_500, mp4: { bytes: 4 }, gif: { bytes: 3 } });
  });

  it("passes the plan to the native recorder and stops at its length", async () => {
    const { session, listener } = await start(undefined, undefined, PRESETS.clip);
    expect(invoke).toHaveBeenCalledWith("start_screen_recording", { options: {
      maxDurationMs: 15_000, mp4: false, gif: true, mp4Bitrate: 3_000_000, gifFps: 10, gifMaxEdge: 800, gifMaxBytes: 10 * 1024 * 1024,
    } });
    invoke.mockImplementation(async (command: string) => command === "screen_recording_status" ? status({ elapsedMs: 15_400 }) : undefined);
    await vi.advanceTimersByTimeAsync(250);
    expect(listener.onLimit).toHaveBeenCalledWith("duration");
    expect(listener.onEnded).toHaveBeenCalledOnce();
    expect(session.elapsedMs()).toBe(15_000);
  });

  it("keeps a long MP4 on disk instead of reading it into the window", async () => {
    const { session } = await start();
    const mp4Bytes = 300 * 1024 * 1024;
    invoke.mockImplementation(async (command: string, args?: { format?: string }) => {
      if (command === "stop_screen_recording") return status({ phase: "ready", mp4Bytes });
      if (command === "read_screen_recording" && args?.format === "gif") return new ArrayBuffer(3);
    });
    const result = await session.stop();
    expect(result.mp4).toMatchObject({ bytes: mp4Bytes, native: { sessionId: "owned-session", format: "mp4" } });
    expect(result.mp4!.blob.size).toBe(0);
    expect(invoke.mock.calls.some(([command, args]) => command === "read_screen_recording" && args?.format === "mp4")).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith("discard_screen_recording", expect.anything());
    await result.release!();
    expect(invoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "owned-session" });
  });

  it("rejects oversized and truncated exports before retaining any recording", async () => {
    const { session } = await start();
    invoke.mockImplementation(async (command: string) => {
      if (command === "stop_screen_recording") return status({ phase: "ready", mp4Bytes: 2 * 1024 * 1024 * 1024 + 1 });
      if (command === "read_screen_recording") return new ArrayBuffer(2);
    });
    await expect(session.stop()).rejects.toThrow("complete native recording");
    expect(invoke.mock.calls.filter(([command]) => command === "read_screen_recording")).toEqual([
      ["read_screen_recording", { sessionId: "owned-session", format: "gif" }],
    ]);
    expect(invoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "owned-session" });
  });

  it("discards safely while binary export is pending and ignores its later bytes", async () => {
    const bytes = deferred<ArrayBuffer>();
    const { session } = await start();
    invoke.mockImplementation((command: string) => {
      if (command === "stop_screen_recording") return Promise.resolve(status({ phase: "ready" }));
      if (command === "read_screen_recording") return bytes.promise;
      return Promise.resolve();
    });
    const stopped = session.stop();
    const rejected = expect(stopped).rejects.toThrow("cancelled");
    await vi.advanceTimersByTimeAsync(0);
    session.discard();
    await rejected;
    bytes.resolve(new ArrayBuffer(4));
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke.mock.calls.filter(([command]) => command === "discard_screen_recording")).toHaveLength(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("surfaces native permission errors and removes the failed session", async () => {
    invoke.mockImplementation(async (command: string) => command === "start_screen_recording"
      ? status({ phase: "error", error: "Allow screen recording in System Settings." }) : undefined);
    await expect(start()).rejects.toThrow("Allow screen recording");
    expect(invoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "owned-session" });
  });

  it("says plainly that only the holdmap window can be recorded when it is not on screen", async () => {
    invoke.mockImplementation(async (command: string) => command === "start_screen_recording"
      ? status({ phase: "error", error: "window_unavailable" }) : undefined);
    await expect(start()).rejects.toThrow(/only record its own window.*Nothing else is ever recorded/);
  });

  it.each(["unsupported_os", "window_unavailable", "capture_start_failed", "invalid_dimensions", "capture_setup_failed", "missing_writer", "mp4_failed", "gif_failed", "missing_directory"])("translates the native %s failure into actionable copy", async (code) => {
    invoke.mockImplementation(async (command: string) => command === "start_screen_recording"
      ? status({ phase: "error", error: code }) : undefined);
    let failure: unknown;
    try { await start(); } catch (cause) { failure = cause; }
    expect(failure).toBeInstanceOf(Error);
    expect((failure as Error).message).not.toContain(code);
    expect((failure as Error).message).toMatch(/[ .]/);
    // Native GIF is derived from a successful MP4; a fatal MP4 failure cannot
    // promise that another export survived, unlike a browser encoder failure.
    if (code === "mp4_failed") expect((failure as Error).message).not.toContain("may still be available");
  });

  it("translates native partial-export warnings", async () => {
    const { session } = await start();
    invoke.mockImplementation(async (command: string) => {
      if (command === "stop_screen_recording") return status({ phase: "ready", gifBytes: 0, warnings: ["gif_failed"] });
      if (command === "read_screen_recording") return new ArrayBuffer(4);
    });
    expect((await session.stop()).warnings).toEqual(["Couldn't finish the GIF. The MP4 may still be available."]);
  });

  it("bounds an unresponsive start acknowledgement and releases its late session", async () => {
    const request = deferred<NativeRecordingStatus>();
    invoke.mockImplementation((command: string) => command === "start_screen_recording" ? request.promise : Promise.resolve());
    const pending = start();
    const rejected = expect(pending).rejects.toThrow("too long to respond");
    await vi.advanceTimersByTimeAsync(15_000);
    await rejected;
    request.resolve(status());
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "owned-session" });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("bounds cleanup when a late acknowledgement follows cancellation and discard never replies", async () => {
    const request = deferred<NativeRecordingStatus>();
    invoke.mockImplementation((command: string) => command === "start_screen_recording"
      ? request.promise : new Promise(() => {}));
    const controller = new AbortController();
    const listener = { onTick: vi.fn(), onEnded: vi.fn() };
    const preparing = vi.fn();
    const pending = NativeRecordingSession.start(listener, controller.signal, preparing);
    const rejected = expect(pending).rejects.toMatchObject({ name: "AbortError" });
    controller.abort();
    await rejected;
    request.resolve(status({ phase: "choosing" }));
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke.mock.calls.filter(([command]) => command === "discard_screen_recording")).toEqual([
      ["discard_screen_recording", { sessionId: "owned-session" }],
    ]);
    expect(vi.getTimerCount()).toBe(1);
    await vi.advanceTimersByTimeAsync(15_000);
    expect(vi.getTimerCount()).toBe(0);
    expect(preparing).not.toHaveBeenCalled();
    expect(listener.onTick).not.toHaveBeenCalled();
    expect(listener.onEnded).not.toHaveBeenCalled();
  });

  it("rejects a status from another session and releases the original session", async () => {
    const { session, listener } = await start();
    invoke.mockImplementation(async (command: string) => command === "screen_recording_status" ? status({ sessionId: "different" }) : undefined);
    await vi.advanceTimersByTimeAsync(250);
    expect(listener.onEnded).toHaveBeenCalledOnce();
    await expect(session.stop()).rejects.toThrow("native recording status");
    expect(invoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "owned-session" });
  });

  it("times out finalization and still deletes native recording resources", async () => {
    const { session } = await start();
    invoke.mockImplementation(async (command: string) => command === "stop_screen_recording" || command === "screen_recording_status"
      ? status({ phase: "finalizing" }) : undefined);
    const stopped = session.stop();
    const rejected = expect(stopped).rejects.toThrow("Preparing this recording took too long");
    await vi.advanceTimersByTimeAsync(120_000);
    await rejected;
    expect(invoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "owned-session" });
    expect(vi.getTimerCount()).toBe(0);
  });
});
