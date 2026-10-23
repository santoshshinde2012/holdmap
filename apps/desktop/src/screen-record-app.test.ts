// @vitest-environment jsdom
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { CaptureResult } from "./lib/screen-record";

const encoder = vi.hoisted(() => ({ start: vi.fn() }));
const save = vi.hoisted(() => vi.fn());
const nativeInvoke = vi.hoisted(() => vi.fn());
vi.mock("./lib/screen-record-encode", () => ({ ScreenRecording: { start: encoder.start } }));
vi.mock("./lib/screen-record-save", () => ({ saveRecording: save }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: nativeInvoke }));
import App from "./App.svelte";

const capture = vi.fn();
const originalMedia = Object.getOwnPropertyDescriptor(navigator, "mediaDevices");
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}
function stream() {
  const track = { readyState: "live", stop: vi.fn() };
  return { track, stream: { getTracks: () => [track], getVideoTracks: () => [track] } as unknown as MediaStream };
}
function result(): CaptureResult {
  return { durationMs: 1_000, width: 320, height: 180, mp4: null, gif: { blob: new Blob(["gif"], { type: "image/gif" }), bytes: 3, filename: "holdmap-test.gif" }, warnings: [] };
}
function session() { return { stop: vi.fn().mockResolvedValue(result()), discard: vi.fn() }; }

beforeAll(() => {
  window.matchMedia ??= ((q: string) => ({ matches: q === "(prefers-reduced-motion: reduce)", media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} }));
  Element.prototype.scrollTo ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
  URL.createObjectURL ??= vi.fn(() => "blob:recording");
  URL.revokeObjectURL ??= vi.fn();
});
beforeEach(() => {
  vi.useFakeTimers();
  localStorage.setItem("holdmap.onboarded", "1");
  Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getDisplayMedia: capture } });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  capture.mockReset();
  encoder.start.mockReset();
  save.mockReset();
  nativeInvoke.mockReset();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  localStorage.clear();
  if (originalMedia) Object.defineProperty(navigator, "mediaDevices", originalMedia);
  else Reflect.deleteProperty(navigator, "mediaDevices");
});
const mount = () => render(App);
async function choose(view: ReturnType<typeof mount>) {
  await fireEvent.click(view.getByRole("button", { name: /^Record screen$/ }));
  await vi.dynamicImportSettled();
  await vi.advanceTimersByTimeAsync(0);
  await fireEvent.click(view.getByRole("button", { name: "Choose screen…" }));
  await vi.dynamicImportSettled();
}

describe("screen recording lifecycle", () => {
  it("waits for native capabilities before declaring a webview unsupported", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: {} });
    const capabilities = deferred<{ available: boolean; reason: string | null }>();
    nativeInvoke.mockReturnValue(capabilities.promise);
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await fireEvent.click(view.getByRole("button", { name: /^Record screen$/ }));
    expect(view.getByRole("status").textContent).toContain("Checking screen recording support");
    expect(view.queryByText("Screen capture unavailable")).toBeNull();
    expect(view.queryByRole("button", { name: "Choose screen…" })).toBeNull();
    capabilities.resolve({ available: true, reason: null });
    await vi.advanceTimersByTimeAsync(0);
    expect(view.getByRole("button", { name: "Choose screen…" })).toBeTruthy();
    expect(capture).not.toHaveBeenCalled();
  });

  it("records through native fallback and retains copied files for review after closing", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: {} });
    const status = { sessionId: "ui-session", phase: "capturing", elapsedMs: 1_000, width: 320, height: 180, mp4Bytes: 0, gifBytes: 3, warnings: [], error: null };
    nativeInvoke.mockImplementation(async (command: string) => {
      if (command === "screen_recording_capabilities") return { available: true, reason: null };
      if (command === "start_screen_recording") return status;
      if (command === "stop_screen_recording") return { ...status, phase: "ready" };
      if (command === "read_screen_recording") return new ArrayBuffer(3);
    });
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    await fireEvent.click(view.getByRole("button", { name: "Stop screen recording" }));
    await vi.advanceTimersByTimeAsync(0);
    expect(view.getByRole("dialog", { name: "Download recording" })).toBeTruthy();
    expect(nativeInvoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "ui-session" });
    await fireEvent.keyDown(view.getByRole("dialog", { name: "Download recording" }), { key: "Escape" });
    await vi.advanceTimersByTimeAsync(20);
    await fireEvent.click(view.getByRole("button", { name: "Review recording" }));
    expect(view.getByRole("button", { name: "Download GIF" })).toBeTruthy();
    expect(nativeInvoke.mock.calls.filter(([command]) => command === "start_screen_recording")).toHaveLength(1);
    expect(capture).not.toHaveBeenCalled();
    expect(encoder.start).not.toHaveBeenCalled();
  });

  it("discards a native picker acknowledgement arriving after the user cancels", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: {} });
    const started = deferred<unknown>();
    nativeInvoke.mockImplementation(async (command: string) => {
      if (command === "screen_recording_capabilities") return { available: true, reason: null };
      if (command === "start_screen_recording") return started.promise;
    });
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
    started.resolve({ sessionId: "late-ui", phase: "choosing", elapsedMs: 0, width: 0, height: 0, mp4Bytes: 0, gifBytes: 0, warnings: [], error: null });
    await vi.advanceTimersByTimeAsync(20);
    expect(nativeInvoke).toHaveBeenCalledWith("discard_screen_recording", { sessionId: "late-ui" });
    expect(view.queryByRole("dialog", { name: "Record screen" })).toBeNull();
    expect(view.queryByRole("button", { name: "Stop screen recording" })).toBeNull();
  });

  it("releases a picker result that arrives after cancellation", async () => {
    const pending = deferred<MediaStream>();
    const captured = stream();
    capture.mockReturnValue(pending.promise);
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    await fireEvent.click(view.getByRole("button", { name: /^Cancel$/ }));
    pending.resolve(captured.stream);
    await vi.advanceTimersByTimeAsync(20);
    expect(captured.track.stop).toHaveBeenCalledOnce();
    expect(encoder.start).not.toHaveBeenCalled();
    expect(view.queryByRole("dialog", { name: "Record screen" })).toBeNull();
    expect(view.queryByRole("button", { name: "Stop screen recording" })).toBeNull();
  });

  it("stops acquired tracks immediately and discards a late encoder after cancel", async () => {
    const captured = stream();
    const pending = deferred<ReturnType<typeof session>>();
    const recording = session();
    capture.mockResolvedValue(captured.stream);
    encoder.start.mockReturnValue(pending.promise);
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    expect(encoder.start).toHaveBeenCalledOnce();
    expect(view.getByRole("status").textContent).toContain("Preparing recording");
    await fireEvent.keyDown(view.getByRole("dialog", { name: "Record screen" }), { key: "Escape" });
    expect(captured.track.stop).toHaveBeenCalledOnce();
    pending.resolve(recording);
    await vi.advanceTimersByTimeAsync(20);
    expect(recording.discard).toHaveBeenCalledOnce();
    expect(view.queryByRole("button", { name: "Stop screen recording" })).toBeNull();
  });

  it("finishes sharing that ends during encoder startup", async () => {
    const pending = deferred<ReturnType<typeof session>>();
    const recording = session();
    capture.mockResolvedValue(stream().stream);
    encoder.start.mockImplementation((_stream, listener: { onEnded: () => void }) => { listener.onEnded(); return pending.promise; });
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    pending.resolve(recording);
    await vi.advanceTimersByTimeAsync(20);
    expect(recording.stop).toHaveBeenCalledOnce();
    expect(view.getByRole("dialog", { name: "Download recording" })).toBeTruthy();
    expect(view.queryByRole("button", { name: "Stop screen recording" })).toBeNull();
  });

  it("does not reopen discarded results over a newer recording flow", async () => {
    const pending = deferred<CaptureResult>();
    const recording = session();
    recording.stop.mockReturnValue(pending.promise);
    capture.mockResolvedValue(stream().stream);
    encoder.start.mockResolvedValue(recording);
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    await fireEvent.click(view.getByRole("button", { name: "Stop screen recording" }));
    await fireEvent.click(view.getByRole("button", { name: "Discard recording" }));
    expect(recording.discard).toHaveBeenCalledOnce();
    await fireEvent.click(view.getByRole("button", { name: /^Record screen$/ }));
    pending.resolve(result());
    await vi.advanceTimersByTimeAsync(20);
    expect(view.getByRole("dialog", { name: "Record screen" })).toBeTruthy();
    expect(view.queryByRole("dialog", { name: "Download recording" })).toBeNull();
  });

  it("keeps permission failure actionable and identifies finish errors accurately", async () => {
    capture.mockRejectedValueOnce(new DOMException("Denied", "NotAllowedError"));
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    expect(view.getByRole("alert").textContent).toContain("Couldn't start");
    expect(view.getByRole("alert").textContent).toContain("cancelled or permission was denied");
    const recording = session();
    recording.stop.mockRejectedValueOnce(new Error("Encoding timed out"));
    capture.mockResolvedValueOnce(stream().stream);
    encoder.start.mockResolvedValueOnce(recording);
    await fireEvent.click(view.getByRole("button", { name: "Choose screen…" }));
    await vi.dynamicImportSettled();
    await fireEvent.click(view.getByRole("button", { name: "Stop screen recording" }));
    await vi.advanceTimersByTimeAsync(20);
    expect(view.getByRole("alert").textContent).toContain("Couldn't finish");
    expect(view.getByRole("alert").textContent).toContain("Encoding timed out");
    expect((view.getByRole("button", { name: "Choose screen…" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("releases late picker streams after the app unmounts", async () => {
    const pending = deferred<MediaStream>();
    const captured = stream();
    capture.mockReturnValue(pending.promise);
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    view.unmount();
    pending.resolve(captured.stream);
    await vi.advanceTimersByTimeAsync(20);
    expect(captured.track.stop).toHaveBeenCalledOnce();
    expect(encoder.start).not.toHaveBeenCalled();
  });

  it("reports encoder preparation failure without blaming screen-sharing support", async () => {
    const captured = stream();
    capture.mockResolvedValue(captured.stream);
    encoder.start.mockRejectedValue(new DOMException("The encoder could not prepare video frames", "NotSupportedError"));
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    expect(view.getByRole("alert").textContent).toContain("Couldn't prepare recording");
    expect(view.getByRole("alert").textContent).toContain("The encoder could not prepare video frames");
    expect(captured.track.stop).toHaveBeenCalledOnce();
  });

  it.each(["cancelled", "failed"])("retains the recording after a %s save, close and keyboard reopen", async (outcome) => {
    const captured = result();
    const recording = session();
    recording.stop.mockResolvedValue(captured);
    capture.mockResolvedValue(stream().stream);
    encoder.start.mockResolvedValue(recording);
    if (outcome === "cancelled") save.mockResolvedValueOnce({ saved: false });
    else save.mockRejectedValueOnce(new Error("Destination is read-only"));
    save.mockResolvedValueOnce({ saved: true, path: "/tmp/holdmap-test.gif" });
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    await fireEvent.click(view.getByRole("button", { name: "Stop screen recording" }));
    await vi.advanceTimersByTimeAsync(20);
    await fireEvent.click(view.getByRole("button", { name: "Download GIF" }));
    await vi.advanceTimersByTimeAsync(20);
    if (outcome === "failed") expect(view.getByRole("alert").textContent).toContain("Destination is read-only");
    await fireEvent.keyDown(view.getByRole("dialog", { name: "Download recording" }), { key: "Escape" });
    await vi.advanceTimersByTimeAsync(20);
    expect(view.queryByRole("dialog", { name: "Download recording" })).toBeNull();
    expect(document.activeElement).toBe(view.getByRole("button", { name: "Review recording" }));
    await fireEvent.keyDown(document.body, { key: "V", shiftKey: true });
    await vi.advanceTimersByTimeAsync(20);
    expect(view.getByRole("dialog", { name: "Download recording" })).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Download GIF" }));
    await vi.advanceTimersByTimeAsync(20);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1][0].blob).toBe(captured.gif!.blob);
    expect(view.getByRole("status").textContent).toMatch(/GIF saved.*\/tmp\/holdmap-test\.gif/);
    expect(encoder.start).toHaveBeenCalledOnce();
    expect(capture).toHaveBeenCalledOnce();
  });

  it("explicitly discards the latest result before retaining a replacement recording", async () => {
    const first = session();
    const second = session();
    const next = result();
    next.gif!.filename = "holdmap-second.gif";
    second.stop.mockResolvedValue(next);
    capture.mockImplementation(async () => stream().stream);
    encoder.start.mockResolvedValueOnce(first).mockResolvedValueOnce(second);
    save.mockResolvedValue({ saved: false });
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    await fireEvent.click(view.getByRole("button", { name: "Stop screen recording" }));
    await vi.advanceTimersByTimeAsync(20);
    await fireEvent.click(view.getByRole("button", { name: "Discard recording" }));
    await vi.advanceTimersByTimeAsync(20);
    expect(view.queryByRole("button", { name: "Review recording" })).toBeNull();
    expect(document.activeElement).toBe(view.getByRole("button", { name: "Record screen" }));
    await choose(view);
    await fireEvent.click(view.getByRole("button", { name: "Stop screen recording" }));
    await vi.advanceTimersByTimeAsync(20);
    await fireEvent.click(view.getByRole("button", { name: "Download GIF" }));
    await vi.advanceTimersByTimeAsync(20);
    expect(encoder.start).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[0][0].filename).toBe("holdmap-second.gif");
    expect(save.mock.calls[0][0].blob).toBe(next.gif!.blob);
  });

  it("keeps active capture across hidden and visible events until it is explicitly stopped", async () => {
    const recording = session();
    capture.mockResolvedValue(stream().stream);
    encoder.start.mockResolvedValue(recording);
    const view = render(App);
    await vi.advanceTimersByTimeAsync(500);
    await choose(view);
    const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    await fireEvent(document, new Event("visibilitychange"));
    hidden.mockReturnValue(false);
    await fireEvent(document, new Event("visibilitychange"));
    await fireEvent(window, new Event("focus"));
    expect(view.getByRole("button", { name: "Stop screen recording" })).toBeTruthy();
    expect(recording.stop).not.toHaveBeenCalled();
    expect(recording.discard).not.toHaveBeenCalled();
    expect(encoder.start).toHaveBeenCalledOnce();
    await fireEvent.click(view.getByRole("button", { name: "Stop screen recording" }));
    await vi.advanceTimersByTimeAsync(20);
    expect(view.getByRole("dialog", { name: "Download recording" })).toBeTruthy();
  });
});
