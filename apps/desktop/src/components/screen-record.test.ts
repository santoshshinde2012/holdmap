// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import ScreenRecordDialog from "./ScreenRecordDialog.svelte";
import type { CaptureResult } from "../lib/screen-record";
const save = vi.hoisted(() => vi.fn());
vi.mock("../lib/screen-record-save", () => ({ saveRecording: save }));

beforeAll(() => {
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} }));
  URL.createObjectURL ??= vi.fn(() => "blob:preview");
  URL.revokeObjectURL ??= vi.fn();
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); save.mockReset(); });

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
    render(ScreenRecordDialog, { phase: "confirm", supported: true, onstart, onclose: vi.fn() });
    expect(screen.getByRole("dialog", { name: "Record screen" })).toBeTruthy();
    expect(screen.getByText(/Nothing is uploaded/)).toBeTruthy();
    expect(screen.getByText(/No microphone or system audio/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Choose screen…" })).toBeTruthy();
  });

  it("starts capture from Choose screen", async () => {
    const onstart = vi.fn();
    render(ScreenRecordDialog, { phase: "confirm", supported: true, onstart, onclose: vi.fn() });
    await fireEvent.click(screen.getByRole("button", { name: "Choose screen…" }));
    expect(onstart).toHaveBeenCalledOnce();
  });

  it("allows encoding to be discarded without trapping keyboard focus", async () => {
    const onclose = vi.fn();
    render(ScreenRecordDialog, { phase: "encoding", onstart: vi.fn(), onclose });
    const dialog = screen.getByRole("dialog", { name: "Preparing files" });
    expect(screen.getByRole("status").textContent).toContain("Finishing your recording");
    expect(screen.getByText(/Finishing MP4/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Close" })).toBeNull();
    expect(screen.getByRole("button", { name: "Discard recording" })).toBeTruthy();
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("explains unavailable capture without offering a picker that cannot work", () => {
    render(ScreenRecordDialog, { phase: "confirm", supported: false, onstart: vi.fn(), onclose: vi.fn() });
    expect(screen.getByText("Screen capture unavailable")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Choose screen…" })).toBeNull();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
  });

  it("shows capability discovery and the native platform's reason without a false picker", async () => {
    const view = render(ScreenRecordDialog, { phase: "confirm", supported: false, checkingSupport: true, onstart: vi.fn(), onclose: vi.fn() });
    expect(screen.getByRole("status").textContent).toContain("Checking screen recording support");
    expect(screen.queryByText("Screen capture unavailable")).toBeNull();
    await view.rerender({ phase: "confirm", supported: false, checkingSupport: false, unsupportedReason: "Native recording requires macOS 14 or newer.", onstart: vi.fn(), onclose: vi.fn() });
    expect(screen.getByText("Native recording requires macOS 14 or newer.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Choose screen…" })).toBeNull();
  });

  it("offers MP4 and GIF downloads from a finished capture", async () => {
    save.mockResolvedValue({ saved: true });
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:file");
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    render(ScreenRecordDialog, { phase: "ready", result: result(), onstart: vi.fn(), onclose: vi.fn() });
    expect(screen.getByRole("dialog", { name: "Download recording" })).toBeTruthy();
    expect(screen.getByText(/MP4 4.0 KB/)).toBeTruthy();
    expect(screen.getByText(/GIF 1.5 KB/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Download MP4" }));
    await fireEvent.click(screen.getByRole("button", { name: "Download GIF" }));
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[0][0].filename).toBe("holdmap-clip.mp4");
    expect(screen.getAllByRole("status").map((node) => node.textContent).join(" ")).toContain("Download started");
    expect(screen.queryByText(/Saved to/)).toBeNull();
  });

  it("retains both files and exposes retry after saving fails", async () => {
    save.mockRejectedValueOnce(new Error("The destination is read-only")).mockResolvedValueOnce({ saved: true, path: "/tmp/holdmap-clip.mp4" });
    render(ScreenRecordDialog, { phase: "ready", result: result(), onstart: vi.fn(), onclose: vi.fn() });
    await fireEvent.click(screen.getByRole("button", { name: "Download MP4" }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("The destination is read-only"));
    expect(screen.getByText(/Your recording is still available/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Download GIF" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Download MP4" }));
    await waitFor(() => expect(screen.getByRole("status").textContent).toMatch(/MP4 saved.*\/tmp\/holdmap-clip\.mp4/));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("keeps a native save picker from losing its recording and handles cancel quietly", async () => {
    let resolve!: (value: { saved: boolean }) => void;
    save.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    const onclose = vi.fn();
    render(ScreenRecordDialog, { phase: "ready", result: result(), onstart: vi.fn(), onclose });
    await fireEvent.click(screen.getByRole("button", { name: "Download MP4" }));
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Close" }).disabled).toBe(true);
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onclose).not.toHaveBeenCalled();
    resolve({ saved: false });
    await waitFor(() => expect(screen.getByRole<HTMLButtonElement>("button", { name: "Download MP4" }).disabled).toBe(false));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText(/ saved|Download started/)).toBeNull();
  });

  it("releases preview URLs after closing", () => {
    const create = vi.spyOn(URL, "createObjectURL").mockReturnValueOnce("blob:mp4").mockReturnValueOnce("blob:gif");
    const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    const view = render(ScreenRecordDialog, { phase: "ready", result: result(), onstart: vi.fn(), onclose: vi.fn() });
    expect(create).toHaveBeenCalledTimes(2);
    view.unmount();
    expect(revoke.mock.calls.flat()).toEqual(["blob:mp4", "blob:gif"]);
  });

  it("distinguishes closing the preview from explicitly discarding its recording", async () => {
    const onclose = vi.fn(), ondiscard = vi.fn();
    render(ScreenRecordDialog, { phase: "ready", result: result(), onstart: vi.fn(), onclose, ondiscard });
    expect(screen.getByText(/Close keeps this recording for this session/)).toBeTruthy();
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onclose).toHaveBeenCalledOnce();
    expect(ondiscard).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Discard recording" }));
    expect(ondiscard).toHaveBeenCalledOnce();
  });

  it("offers presets and a live estimate before the picker, and reports the chosen plan", async () => {
    const onplan = vi.fn();
    render(ScreenRecordDialog, { phase: "confirm", supported: true, backend: "browser", onstart: vi.fn(), onclose: vi.fn(), onplan });
    expect(screen.getByRole("radio", { name: "Full session" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("list", { name: "Expected size and length" }).textContent).toContain("Stops by itself after");
    await fireEvent.click(screen.getByRole("radio", { name: "Share a clip" }));
    expect(onplan).toHaveBeenCalledWith(expect.objectContaining({ preset: "clip", formats: ["gif"] }));
    await fireEvent.click(screen.getByRole("radio", { name: "Both" }));
    expect(onplan).toHaveBeenLastCalledWith(expect.objectContaining({ preset: "custom", formats: ["mp4", "gif"] }));
  });

  it("says why a recording stopped by itself and what to do next", () => {
    render(ScreenRecordDialog, { phase: "ready", result: result(), stoppedBy: "duration", onstart: vi.fn(), onclose: vi.fn() });
    expect(screen.getByText("Stopped automatically")).toBeTruthy();
  });

  it("keeps a large desktop recording on disk and offers its folder after saving", async () => {
    save.mockResolvedValue({ saved: true, path: "/Users/me/Movies/holdmap-long.mp4" });
    const long = { ...result(), gif: null, mp4: { blob: new Blob([]), bytes: 900_000_000, filename: "holdmap-long.mp4", native: { sessionId: "s", format: "mp4" as const } } };
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    try {
      render(ScreenRecordDialog, { phase: "ready", result: long, onstart: vi.fn(), onclose: vi.fn() });
      expect(screen.getByText(/stays on disk/)).toBeTruthy();
      expect(document.querySelector("video")).toBeNull();
      await fireEvent.click(screen.getByRole("button", { name: "Download MP4" }));
      await waitFor(() => expect(screen.getByRole("button", { name: "Show in folder" })).toBeTruthy());
    } finally { delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; }
  });
});
