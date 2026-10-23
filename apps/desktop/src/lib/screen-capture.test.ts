// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

const backend = vi.hoisted(() => ({
  initialRecordingSupport: vi.fn(() => ({ backend: "browser" as const, pending: false, reason: null })),
  resolveRecordingSupport: vi.fn(),
  startRecording: vi.fn(),
}));
vi.mock("./screen-record-backend", () => backend);

import { ScreenCaptureController } from "./screen-capture.svelte";
import { PRESETS, withPreset } from "./screen-record-plan";

function session(stop: () => Promise<unknown> = async () => ({ blob: new Blob(), mime: "video/webm", durationMs: 1000 })) {
  return { stop: vi.fn(stop), discard: vi.fn() };
}

describe("ScreenCaptureController", () => {
  let blocked = false;
  const make = () => new ScreenCaptureController({ blocked: () => blocked, modalOpen: () => false });
  beforeEach(() => { blocked = false; backend.startRecording.mockReset(); });

  it("opens the confirm step and records until finished", async () => {
    const s = session();
    backend.startRecording.mockResolvedValue(s);
    const c = make();
    c.open();
    expect(c.dialog).toBe("confirm");
    await c.begin();
    expect(c.session?.stop).toBe(s.stop);
    expect(c.dialog).toBeNull();
    await c.finish();
    expect(s.stop).toHaveBeenCalledOnce();
    expect(c.dialog).toBe("ready");
    expect(c.result).not.toBeNull();
  });

  it("does not open while the snapshot picker owns the screen", () => {
    blocked = true;
    const c = make();
    c.open();
    expect(c.dialog).toBeNull();
  });

  it("shows a start error on the confirm step", async () => {
    backend.startRecording.mockRejectedValue(new Error("denied"));
    const c = make();
    c.open();
    await c.begin();
    expect(c.dialog).toBe("confirm");
    expect(c.error).toBeTruthy();
    expect(c.busy).toBe(false);
  });

  it("discards a running session and ignores a late start", async () => {
    let resolve!: (s: ReturnType<typeof session>) => void;
    backend.startRecording.mockReturnValue(new Promise((r) => (resolve = r)));
    const c = make();
    c.open();
    const started = c.begin();
    c.discard();
    const late = session();
    resolve(late);
    await started;
    expect(late.discard).toHaveBeenCalledOnce();
    expect(c.session).toBeNull();
    expect(c.dialog).toBeNull();
  });

  it("keeps a finished result when the preview is closed, until discarded", async () => {
    backend.startRecording.mockResolvedValue(session());
    const c = make();
    c.open();
    await c.begin();
    await c.finish();
    c.close();
    expect(c.dialog).toBeNull();
    expect(c.result).not.toBeNull();
    c.open();
    expect(c.dialog).toBe("ready");
    c.discardAndClose();
    expect(c.result).toBeNull();
  });

  it("starts with the chosen plan and keeps it for the running recording", async () => {
    backend.startRecording.mockResolvedValue(session());
    const c = make();
    c.open();
    c.setPlan(withPreset("clip", c.plan));
    await c.begin();
    expect(backend.startRecording.mock.calls[0][4]).toMatchObject({ preset: "clip", formats: ["gif"], maxDurationMs: PRESETS.clip.maxDurationMs });
    c.setPlan(withPreset("session", c.plan));
    expect(c.activePlan.preset).toBe("clip");
  });

  it("clamps an out-of-range plan", () => {
    const c = make();
    c.setPlan({ ...PRESETS.clip, maxDurationMs: 99 * 3_600_000 });
    expect(c.plan.maxDurationMs).toBeLessThanOrEqual(3_600_000);
  });

  it("records why a recording stopped by itself and caps the clock at the limit", async () => {
    backend.startRecording.mockImplementation(async (_backend, listener) => {
      listener.onTick(PRESETS.clip.maxDurationMs + 5_000);
      listener.onLimit("duration");
      return session();
    });
    const c = make();
    c.open();
    c.setPlan(withPreset("clip", c.plan));
    await c.begin();
    expect(c.stoppedBy).toBe("duration");
    expect(c.elapsed).toBe(PRESETS.clip.maxDurationMs);
    c.discard();
    expect(c.stoppedBy).toBeNull();
  });

  it("pauses and resumes only when the backend can", async () => {
    const plain = session();
    backend.startRecording.mockResolvedValueOnce(plain);
    const c = make();
    c.open();
    await c.begin();
    expect(c.canPause).toBe(false);
    c.togglePause();
    expect(c.paused).toBe(false);
    c.discard();

    const pausable = { ...session(), pause: vi.fn(), resume: vi.fn() };
    backend.startRecording.mockResolvedValueOnce(pausable);
    c.open();
    await c.begin();
    expect(c.canPause).toBe(true);
    c.togglePause();
    expect(pausable.pause).toHaveBeenCalledOnce();
    expect(c.paused).toBe(true);
    c.togglePause();
    expect(pausable.resume).toHaveBeenCalledOnce();
    expect(c.paused).toBe(false);
    c.togglePause();
    await c.finish();
    expect(c.paused).toBe(false);
  });

  it("releases an on-disk result when it is discarded", async () => {
    const release = vi.fn(async () => undefined);
    backend.startRecording.mockResolvedValue(session(async () => ({ mp4: null, gif: null, width: 2, height: 2, durationMs: 1, warnings: [], release })));
    const c = make();
    c.open();
    await c.begin();
    await c.finish();
    c.discardAndClose();
    expect(release).toHaveBeenCalledOnce();
  });
});
