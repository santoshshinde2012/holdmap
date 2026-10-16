// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { captureScreenSnapshot } from "./screen-snapshot-backend";

const { frame } = vi.hoisted(() => ({ frame: vi.fn() }));
vi.mock("./screen-snapshot", async (original) => ({ ...await original<typeof import("./screen-snapshot")>(), captureVideoSnapshot: frame }));
const original = Object.getOwnPropertyDescriptor(navigator, "mediaDevices");
function media(picker: () => Promise<MediaStream>) { Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getDisplayMedia: picker } }); }
function stream() {
  const track = { readyState: "live", stop: vi.fn() };
  return { track, value: { getTracks: () => [track], getVideoTracks: () => [track] } as unknown as MediaStream };
}
afterEach(() => {
  if (original) Object.defineProperty(navigator, "mediaDevices", original); else Reflect.deleteProperty(navigator, "mediaDevices");
  vi.restoreAllMocks(); frame.mockReset(); vi.useRealTimers(); document.body.innerHTML = "";
});

describe("owned standalone screen snapshot", () => {
  it("opens the picker in the click turn, requests bounded pixels without audio and releases its source", async () => {
    const owned = stream(), picker = vi.fn().mockResolvedValue(owned.value);
    media(picker); vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
    const result = { source: "screen" }; frame.mockResolvedValue(result);
    const pending = captureScreenSnapshot("browser", new AbortController().signal);
    expect(picker).toHaveBeenCalledOnce();
    expect(picker).toHaveBeenCalledWith(expect.objectContaining({ video: { frameRate: 30, width: { max: 3840 }, height: { max: 2160 } }, audio: false, systemAudio: "exclude" }));
    expect(await pending).toBe(result);
    expect(frame).toHaveBeenCalledWith(expect.any(HTMLVideoElement), expect.any(AbortSignal), "screen");
    expect(owned.track.stop).toHaveBeenCalledOnce(); expect(document.querySelector("video")).toBeNull();
  });

  it("releases a picker result that arrives after cancellation", async () => {
    let resolve!: (stream: MediaStream) => void;
    const owned = stream(); media(() => new Promise((done) => { resolve = done; }));
    const controller = new AbortController();
    const pending = captureScreenSnapshot("browser", controller.signal);
    const rejected = expect(pending).rejects.toMatchObject({ name: "AbortError" });
    controller.abort(); await rejected;
    resolve(owned.value); await Promise.resolve(); await Promise.resolve();
    expect(owned.track.stop).toHaveBeenCalledOnce(); expect(frame).not.toHaveBeenCalled();
  });

  it("releases tracks and its video after playback or PNG failure", async () => {
    for (const playbackFails of [true, false]) {
      const owned = stream(); media(vi.fn().mockResolvedValue(owned.value));
      const play = vi.spyOn(HTMLMediaElement.prototype, "play");
      if (playbackFails) play.mockRejectedValue(new Error("playback failed")); else play.mockResolvedValue(undefined);
      frame.mockRejectedValue(new Error("PNG failed"));
      await expect(captureScreenSnapshot("browser", new AbortController().signal)).rejects.toThrow(playbackFails ? "playback failed" : "PNG failed");
      expect(owned.track.stop).toHaveBeenCalledOnce(); expect(document.querySelector("video")).toBeNull();
      play.mockRestore();
    }
  });

  it("does not open a source after an already-cancelled request", async () => {
    const picker = vi.fn(); media(picker);
    const controller = new AbortController(); controller.abort();
    await expect(captureScreenSnapshot("browser", controller.signal)).rejects.toMatchObject({ name: "AbortError" });
    expect(picker).not.toHaveBeenCalled();
  });

  it("bounds an abandoned picker and still releases its late stream", async () => {
    vi.useFakeTimers();
    let resolve!: (stream: MediaStream) => void;
    const owned = stream(); media(() => new Promise((done) => { resolve = done; }));
    const pending = captureScreenSnapshot("browser", new AbortController().signal);
    const rejected = expect(pending).rejects.toThrow(/selection took too long/);
    await vi.advanceTimersByTimeAsync(120_000); await rejected;
    resolve(owned.value); await Promise.resolve(); await Promise.resolve();
    expect(owned.track.stop).toHaveBeenCalledOnce(); expect(vi.getTimerCount()).toBe(0);
  });
});
