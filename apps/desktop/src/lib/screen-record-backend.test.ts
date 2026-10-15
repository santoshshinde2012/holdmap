// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { initialRecordingSupport, resolveRecordingSupport, startRecording } from "./screen-record-backend";

const { invoke, encode } = vi.hoisted(() => ({ invoke: vi.fn(), encode: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./screen-record-encode", () => ({ ScreenRecording: { start: encode } }));
const original = Object.getOwnPropertyDescriptor(navigator, "mediaDevices");
const media = (getDisplayMedia?: () => Promise<MediaStream>) => Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getDisplayMedia } });
afterEach(() => {
  if (original) Object.defineProperty(navigator, "mediaDevices", original);
  else Reflect.deleteProperty(navigator, "mediaDevices");
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  vi.resetAllMocks();
  vi.useRealTimers();
});

describe("screen recording backend selection", () => {
  it("keeps browser capture when it exists, even in the native desktop", async () => {
    media(vi.fn());
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    expect(await resolveRecordingSupport()).toEqual({ backend: "browser", pending: false, reason: null });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("resolves native fallback only after missing browser support is checked", async () => {
    media();
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValue({ available: true, reason: null });
    expect(initialRecordingSupport()).toEqual({ backend: null, pending: true, reason: null });
    expect(await resolveRecordingSupport()).toEqual({ backend: "native", pending: false, reason: null });
    expect(invoke).toHaveBeenCalledExactlyOnceWith("screen_recording_capabilities");
  });

  it("preserves a native unsupported reason and leaves ordinary browsers unsupported", async () => {
    media();
    expect(await resolveRecordingSupport()).toMatchObject({ backend: null, pending: false });
    expect(invoke).not.toHaveBeenCalled();
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValue({ available: false, reason: "Native recording requires macOS 14 or newer." });
    expect(await resolveRecordingSupport()).toMatchObject({ backend: null, pending: false, reason: "Native recording requires macOS 14 or newer." });
  });

  it("does not leave discovery pending when native IPC fails", async () => {
    media();
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    invoke.mockRejectedValue(new Error("not installed"));
    expect(await resolveRecordingSupport()).toMatchObject({ backend: null, pending: false, reason: expect.stringContaining("Reopen") });
  });

  it("bounds an unresponsive native capability query", async () => {
    vi.useFakeTimers();
    media();
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    invoke.mockReturnValue(new Promise(() => {}));
    const pending = resolveRecordingSupport();
    await vi.dynamicImportSettled();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(await pending).toMatchObject({ backend: null, pending: false, reason: expect.stringContaining("Reopen") });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("invokes the browser picker synchronously in the click turn", async () => {
    const track = { readyState: "live", stop: vi.fn() };
    const stream = { getTracks: () => [track], getVideoTracks: () => [track] } as unknown as MediaStream;
    const picker = vi.fn().mockResolvedValue(stream);
    media(picker);
    const session = { elapsedMs: () => 0, stop: vi.fn(), discard: vi.fn() };
    encode.mockResolvedValue(session);
    const pending = startRecording("browser", { onTick() {}, onEnded() {} }, new AbortController().signal, vi.fn());
    expect(picker).toHaveBeenCalledOnce();
    expect(encode).not.toHaveBeenCalled();
    expect(await pending).toBe(session);
  });
});
