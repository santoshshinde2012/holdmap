import { describe, expect, it, vi } from "vitest";
import { createSnapshotController, type SnapshotState } from "./snapshot-controller";
import type { SnapshotResult } from "./screen-snapshot";

const image = (source: SnapshotResult["source"] = "app"): SnapshotResult => ({
  source, width: 320, height: 180, capturedAtMs: 1000,
  png: { blob: new Blob(["png"], { type: "image/png" }), bytes: 3, filename: "holdmap-snapshot.png" },
});
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
};
function setup() {
  let state: SnapshotState | undefined;
  const app = vi.fn<(signal: AbortSignal) => Promise<SnapshotResult>>().mockResolvedValue(image());
  const screen = vi.fn<(signal: AbortSignal) => Promise<SnapshotResult>>().mockResolvedValue(image("screen"));
  const captureSnapshot = vi.fn().mockResolvedValue(image("recording"));
  const recording = vi.fn(() => ({ captureSnapshot }));
  const prepare = vi.fn().mockResolvedValue(undefined);
  const controller = createSnapshotController({ app, screen, recording, prepare }, (next) => (state = next));
  return { controller, app, screen, captureSnapshot, recording, prepare, state: () => state! };
}

describe("snapshot request ownership", () => {
  it("requests the browser screen picker before yielding to preparation", async () => {
    const s = setup();
    const pending = s.controller.capture("screen");
    expect(s.screen).toHaveBeenCalledOnce();
    expect(s.prepare).not.toHaveBeenCalled();
    await pending;
    expect(s.state()).toMatchObject({ busy: false, open: true, phase: "ready", result: image("screen") });
  });
  it("hides the chooser and prepares the view before borrowing recording pixels", async () => {
    const s = setup(), ready = deferred<void>();
    s.prepare.mockReturnValue(ready.promise);
    s.controller.show();
    const pending = s.controller.capture("recording");
    expect(s.state()).toMatchObject({ open: false, busy: true });
    expect(s.captureSnapshot).not.toHaveBeenCalled();
    ready.resolve();
    await pending;
    expect(s.captureSnapshot).toHaveBeenCalledOnce();
    expect(s.screen).not.toHaveBeenCalled();
    expect(s.app).not.toHaveBeenCalled();
  });
  it("retains the latest PNG on close, failure and cancellation; discard alone clears it", async () => {
    const s = setup();
    await s.controller.capture("app");
    s.controller.close();
    expect(s.state()).toMatchObject({ open: false, result: image() });
    s.controller.show();
    expect(s.state().phase).toBe("ready");
    s.app.mockRejectedValueOnce(new Error("Couldn't render this view"));
    await s.controller.capture("app");
    expect(s.state()).toMatchObject({ result: image(), error: "Couldn't render this view", phase: "choose" });
    const late = deferred<SnapshotResult>();
    s.screen.mockReturnValueOnce(late.promise);
    const pending = s.controller.capture("screen");
    const signal = s.screen.mock.calls.at(-1)![0];
    s.controller.cancel();
    expect(signal.aborted).toBe(true);
    late.resolve(image("screen"));
    await pending;
    expect(s.state()).toMatchObject({ open: false, result: image(), busy: false });
    s.controller.discard();
    expect(s.state().result).toBeNull();
  });
  it("rejects overlapping requests and ignores disposed completions", async () => {
    const s = setup(), late = deferred<SnapshotResult>();
    s.screen.mockReturnValue(late.promise);
    const pending = s.controller.capture("screen");
    await s.controller.capture("app");
    expect(s.app).not.toHaveBeenCalled();
    s.controller.dispose();
    const last = s.state();
    late.resolve(image("screen"));
    await pending;
    expect(s.state()).toBe(last);
    expect(s.screen.mock.calls[0][0].aborted).toBe(true);
  });
  it("reports a missing recording source without opening a picker or dropping the prior PNG", async () => {
    const s = setup();
    await s.controller.capture("app");
    s.recording.mockReturnValue(null as unknown as ReturnType<typeof s.recording>);
    await s.controller.capture("recording");
    expect(s.state()).toMatchObject({ result: image(), phase: "choose", error: expect.stringContaining("no longer available") });
    expect(s.screen).not.toHaveBeenCalled();
  });
});
