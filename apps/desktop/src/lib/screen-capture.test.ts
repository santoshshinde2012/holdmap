// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

const backend = vi.hoisted(() => ({
  initialRecordingSupport: vi.fn(() => ({ backend: "browser" as const, pending: false, reason: null })),
  resolveRecordingSupport: vi.fn(),
  startRecording: vi.fn(),
}));
vi.mock("./screen-record-backend", () => backend);

import { ScreenCaptureController } from "./screen-capture.svelte";

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
});
