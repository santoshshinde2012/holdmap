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
    expect(screen.getByText(/never leaves this machine/)).toBeTruthy();
    expect(screen.getByText(/Microphone and system audio are not captured/)).toBeTruthy();
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
    expect(screen.getByRole("status").textContent).toContain("Encoding MP4 and GIF");
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
    expect(screen.getByRole("status").textContent).toContain("Download started");
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
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Saved to /tmp/holdmap-clip.mp4"));
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
    expect(screen.queryByText(/Saved to|Download started/)).toBeNull();
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
});
