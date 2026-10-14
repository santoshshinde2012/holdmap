// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { captureSupport, displayCaptureError, formatClock, gifDelayMs, gifSize, recordingFilename, GIF_MAX_FRAMES } from "./screen-record";

describe("screen recording helpers", () => {
  it("formats a compact elapsed clock", () => {
    expect(formatClock(0)).toBe("0:00");
    expect(formatClock(1_500)).toBe("0:01");
    expect(formatClock(61_000)).toBe("1:01");
    expect(formatClock(-20)).toBe("0:00");
  });

  it("names downloads with a local timestamp and the requested extension", () => {
    expect(recordingFilename("mp4", new Date("2026-10-14T13:08:09"))).toBe("holdmap-20261014-130809.mp4");
    expect(recordingFilename("gif", new Date("2026-10-14T13:08:09"))).toBe("holdmap-20261014-130809.gif");
  });

  it("scales GIF frames down without stretching", () => {
    expect(gifSize(320, 180)).toEqual({ width: 320, height: 180 });
    expect(gifSize(1920, 1080)).toEqual({ width: 640, height: 360 });
    expect(gifSize(0, 0)).toEqual({ width: 1, height: 1 });
  });

  it("keeps GIF length within the frame budget", () => {
    expect(gifDelayMs(1_000)).toBe(125);
    expect(gifDelayMs(120_000) * GIF_MAX_FRAMES).toBe(120_000);
  });

  it("explains permission and support failures without leaking internals", () => {
    expect(displayCaptureError({ name: "NotAllowedError" }).message).toMatch(/cancelled or blocked/);
    expect(displayCaptureError({ name: "NotSupportedError" }).message).toMatch(/cannot capture/);
    expect(displayCaptureError({ name: "NotFoundError" }).message).toMatch(/No screen/);
    expect(displayCaptureError({ message: "boom" }).message).toBe("boom");
  });

  it("reports display capture from the current environment", () => {
    expect(captureSupport()).toEqual({ display: false, gif: true });
  });
});

describe("downloadBlob", () => {
  it("creates a temporary object URL and clicks an anchor", async () => {
    const create = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:holdmap-test");
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    const { downloadBlob } = await import("./screen-record");
    downloadBlob(new Blob(["ok"], { type: "video/mp4" }), "holdmap-test.mp4");
    expect(create).toHaveBeenCalledOnce();
    expect(click).toHaveBeenCalledOnce();
    click.mockRestore();
    create.mockRestore();
    revoke.mockRestore();
  });
});
