// @vitest-environment jsdom
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { CaptureResult } from "./lib/screen-record";

const encoder = vi.hoisted(() => ({ start: vi.fn() }));
vi.mock("./lib/screen-record-encode", () => ({ ScreenRecording: { start: encoder.start } }));
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
  localStorage.clear();
  if (originalMedia) Object.defineProperty(navigator, "mediaDevices", originalMedia);
  else Reflect.deleteProperty(navigator, "mediaDevices");
});
const mount = () => render(App);
async function choose(view: ReturnType<typeof mount>) {
  await fireEvent.click(view.getByRole("button", { name: /^Record screen$/ }));
  await fireEvent.click(view.getByRole("button", { name: "Choose screen…" }));
  await vi.dynamicImportSettled();
}

describe("screen recording lifecycle", () => {
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
});
