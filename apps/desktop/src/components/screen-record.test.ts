// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import ScreenRecordDialog from "./ScreenRecordDialog.svelte";
import type { CaptureResult } from "../lib/screen-record";

beforeAll(() => {
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} }));
  URL.createObjectURL ??= vi.fn(() => "blob:preview");
  URL.revokeObjectURL ??= vi.fn();
});
afterEach(cleanup);

const result = (): CaptureResult => ({
  durationMs: 2_400,
  width: 1280,
  height: 720,
  mp4: { blob: new Blob(["mp4"], { type: "video/mp4" }), bytes: 4_096, filename: "holdmap-clip.mp4" },
  gif: { blob: new Blob(["gif"], { type: "image/gif" }), bytes: 1_536, filename: "holdmap-clip.gif" },
  warnings: [],
});

describe("screen record dialog", () => {
  it("explains local capture before the OS picker", () => {
    const onstart = vi.fn();
    render(ScreenRecordDialog, { phase: "confirm", onstart, onclose: vi.fn() });
    expect(screen.getByRole("dialog", { name: "Record screen" })).toBeTruthy();
    expect(screen.getByText(/never leaves this machine/)).toBeTruthy();
    expect(screen.getByText(/Microphone and system audio are not captured/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Choose screen…" })).toBeTruthy();
  });

  it("starts capture from Choose screen", async () => {
    const onstart = vi.fn();
    render(ScreenRecordDialog, { phase: "confirm", onstart, onclose: vi.fn() });
    await fireEvent.click(screen.getByRole("button", { name: "Choose screen…" }));
    expect(onstart).toHaveBeenCalledOnce();
  });

  it("keeps encoding non-dismissable", async () => {
    const onclose = vi.fn();
    render(ScreenRecordDialog, { phase: "encoding", onstart: vi.fn(), onclose });
    const dialog = screen.getByRole("dialog", { name: "Preparing files" });
    expect(screen.getByRole("status").textContent).toContain("Encoding MP4 and GIF");
    expect(screen.queryByRole("button", { name: "Close" })).toBeNull();
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onclose).not.toHaveBeenCalled();
  });

  it("offers MP4 and GIF downloads from a finished capture", async () => {
    const download = vi.fn();
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:file");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(download as unknown as () => void);
    render(ScreenRecordDialog, { phase: "ready", result: result(), onstart: vi.fn(), onclose: vi.fn() });
    expect(screen.getByRole("dialog", { name: "Download recording" })).toBeTruthy();
    expect(screen.getByText(/MP4 4.0 KB/)).toBeTruthy();
    expect(screen.getByText(/GIF 1.5 KB/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Download MP4" }));
    await fireEvent.click(screen.getByRole("button", { name: "Download GIF" }));
    expect(download).toHaveBeenCalledTimes(2);
    click.mockRestore();
    vi.unstubAllGlobals();
  });
});
