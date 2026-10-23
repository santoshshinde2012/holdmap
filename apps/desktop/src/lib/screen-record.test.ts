// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { captureSupport, displayCaptureError, formatClock, gifSize, recordingFilename } from "./screen-record";

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });

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
    expect(gifSize(1920, 1080, 800)).toEqual({ width: 800, height: 450 });
    expect(gifSize(0, 0)).toEqual({ width: 1, height: 1 });
  });

  it("bounds portrait and ultrawide recordings by their longest edge", () => {
    expect(gifSize(1080, 1920)).toEqual({ width: 360, height: 640 });
    expect(gifSize(7680, 1080)).toEqual({ width: 640, height: 90 });
    expect(gifSize(1080, 7680)).toEqual({ width: 90, height: 640 });
  });

  it("keeps malformed frame dimensions finite and inside the pixel budget", () => {
    expect(gifSize(Number.POSITIVE_INFINITY, Number.NaN)).toEqual({ width: 1, height: 1 });
    expect(gifSize(1920, 1080, Number.NaN)).toEqual({ width: 640, height: 360 });
    expect(gifSize(1920, 1080, -1)).toEqual({ width: 1, height: 1 });
  });

  it("explains permission and support failures without leaking internals", () => {
    expect(displayCaptureError({ name: "NotAllowedError" }).message).toMatch(/cancelled or permission was denied/);
    expect(displayCaptureError({ name: "NotSupportedError" }).message).toMatch(/unavailable in this window/);
    expect(displayCaptureError({ name: "NotFoundError" }).message).toMatch(/holdmap window was not available/);
    expect(displayCaptureError({ name: "NotReadableError" }).message).toMatch(/could not be read/);
    expect(displayCaptureError({ name: "InvalidStateError" }).message).toMatch(/Return to this window/);
    expect(displayCaptureError({ name: "AbortError" }).message).toMatch(/interrupted/);
    expect(displayCaptureError({ message: "boom" }).message).toBe("boom");
  });

  it("reports display capture from the current environment", () => {
    expect(captureSupport()).toEqual({ display: false, gif: true });
  });
});

describe("downloadBlob", () => {
  it("creates a temporary object URL and clicks an anchor", async () => {
    vi.useFakeTimers();
    const create = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:holdmap-test");
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    const { downloadBlob } = await import("./screen-record");
    downloadBlob(new Blob(["ok"], { type: "video/mp4" }), "holdmap-test.mp4");
    expect(create).toHaveBeenCalledOnce();
    expect(click).toHaveBeenCalledOnce();
    expect(document.querySelector("a[download]")).toBeNull();
    await vi.advanceTimersByTimeAsync(30_000);
    expect(revoke).toHaveBeenCalledWith("blob:holdmap-test");
  });

  it("cleans up the anchor and URL when a download request fails", async () => {
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:failed-download");
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => { throw new Error("Download blocked"); });
    const { downloadBlob } = await import("./screen-record");
    expect(() => downloadBlob(new Blob(["ok"]), "holdmap-test.gif")).toThrow("Download blocked");
    expect(document.querySelector("a[download]")).toBeNull();
    expect(revoke).toHaveBeenCalledWith("blob:failed-download");
  });
});
