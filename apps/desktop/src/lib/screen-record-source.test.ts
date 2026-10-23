import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const added: { timestamp: number }[] = [];
vi.mock("mediabunny", () => ({
  MediaStreamVideoTrackSource: class { errorPromise = new Promise<void>(() => undefined); pause() {} resume() {} close() {} },
  VideoSample: class { constructor(public frame: { timestamp: number }) {} close() {} },
  VideoSampleSource: class {
    add(sample: { frame: { timestamp: number } }) { added.push(sample.frame); return Promise.resolve(); }
    close() {}
  },
}));

import { createTrackSource, MAX_STALL_MS } from "./screen-record-source";

class FakeFrame { constructor(_: unknown, public init: { timestamp: number }) {} get timestamp() { return this.init.timestamp; } close() {} }

describe("steady screen frame source", () => {
  let clock = 0;
  let push: (frame: object) => void = () => undefined;
  const config = { codec: "avc" } as never;
  const track = {} as MediaStreamVideoTrack;

  beforeEach(() => {
    added.length = 0;
    clock = 0;
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    vi.spyOn(performance, "now").mockImplementation(() => clock);
    vi.stubGlobal("VideoFrame", FakeFrame);
    vi.stubGlobal("MediaStreamTrackProcessor", class {
      readable = new ReadableStream({ start: (controller) => { push = (frame) => controller.enqueue(frame); } });
      constructor(public init: { maxBufferSize: number }) { seen.push(init.maxBufferSize); }
    });
  });
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); seen.length = 0; });
  const seen: number[] = [];
  const step = async (ms: number) => { clock += ms; await vi.advanceTimersByTimeAsync(34); };

  it("keeps a bounded frame queue and stamps frames with the time recorded", async () => {
    const frames = createTrackSource(track, config, 30);
    frames.begin();
    expect(seen).toEqual([2]);
    push({ close() {} });
    await step(33);
    await step(33);
    expect(added.map((frame) => frame.timestamp)).toEqual([33_000, 66_000]);
    frames.close();
  });

  it("counts a long page stall as one short frame instead of replaying every missed frame", async () => {
    const frames = createTrackSource(track, config, 30);
    frames.begin();
    push({ close() {} });
    await step(33);
    await step(60_000);
    await step(33);
    const stamps = added.map((frame) => frame.timestamp);
    expect(stamps).toHaveLength(3);
    expect(stamps[1] - stamps[0]).toBe(MAX_STALL_MS * 1000);
    expect(stamps[2] - stamps[1]).toBe(33_000);
    frames.close();
  });

  it("leaves paused time out and sends nothing after close", async () => {
    const frames = createTrackSource(track, config, 30);
    frames.begin();
    push({ close() {} });
    await step(33);
    frames.pause();
    await step(5_000);
    frames.resume();
    await step(33);
    const stamps = added.map((frame) => frame.timestamp);
    expect(stamps).toHaveLength(2);
    expect(stamps[1] - stamps[0]).toBe(33_000);
    frames.close();
    await step(100);
    expect(added).toHaveLength(2);
  });
});
