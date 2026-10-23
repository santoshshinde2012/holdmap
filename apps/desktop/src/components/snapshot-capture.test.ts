// @vitest-environment jsdom
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import SnapshotCapture from "./SnapshotCapture.svelte";
import type { SnapshotCaptureHandle } from "../lib/snapshot-controller";
import type { SnapshotResult } from "../lib/screen-snapshot";

const mocks = vi.hoisted(() => ({ app: vi.fn(), screen: vi.fn(), save: vi.fn(), support: vi.fn() }));
vi.mock("../lib/app-snapshot", () => ({ captureAppSnapshot: mocks.app }));
vi.mock("../lib/screen-snapshot-backend", () => ({ captureScreenSnapshot: mocks.screen }));
vi.mock("../lib/screen-record-save", () => ({ saveSnapshot: mocks.save }));
vi.mock("../lib/screen-record-backend", () => ({ initialRecordingSupport: mocks.support, resolveRecordingSupport: mocks.support }));
const png = (source: SnapshotResult["source"] = "app"): SnapshotResult => ({
  source, width: 320, height: 180, capturedAtMs: 1000,
  png: { blob: new Blob(["PNG"], { type: "image/png" }), bytes: 3, filename: "holdmap-snapshot.png" },
});
beforeAll(() => {
  Element.prototype.animate ??= (() => ({ cancel() {}, finish() {} })) as unknown as typeof Element.prototype.animate;
  URL.createObjectURL ??= () => "blob:preview";
  URL.revokeObjectURL ??= () => {};
  globalThis.requestAnimationFrame ??= (callback) => setTimeout(() => callback(performance.now()), 0) as unknown as number;
  globalThis.cancelAnimationFrame ??= (id) => clearTimeout(id);
});
beforeEach(() => {
  mocks.support.mockReturnValue({ backend: "browser", pending: false, reason: null });
  mocks.app.mockResolvedValue(png());
  mocks.screen.mockResolvedValue(png("screen"));
  mocks.save.mockResolvedValue({ saved: true });
  const root = document.createElement("div"); root.id = "holdmap-capture-root"; document.body.append(root);
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:png");
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
});
afterEach(() => {
  cleanup(); document.getElementById("holdmap-capture-root")?.remove();
  document.querySelector('[role="dialog"][aria-label="Command palette"]')?.remove();
  vi.restoreAllMocks(); vi.resetAllMocks(); vi.useRealTimers();
});
const handle = (view: { component: unknown }) => view.component as SnapshotCaptureHandle;

describe("snapshot capture view", () => {
  it("offers app PNG capture even when screen sharing is unavailable", async () => {
    mocks.support.mockReturnValue({ backend: null, pending: false, reason: "Screen sharing unavailable here." });
    const view = render(SnapshotCapture);
    handle(view).show();
    await waitFor(() => expect(view.getByRole("dialog", { name: "Take snapshot" })).toBeTruthy());
    expect(view.getByRole("radio", { name: "Holdmap view" }).getAttribute("aria-checked")).toBe("true");
    expect((view.getByRole("radio", { name: "Holdmap window" }) as HTMLButtonElement).disabled).toBe(true);
    expect(view.getByRole("button", { name: "Capture PNG" })).toBeTruthy();
    expect(view.getByText(/No screen-sharing permission is needed/)).toBeTruthy();
  });
  it("normalizes a cancelled standalone source when a video starts, preserving valid choices before that transition", async () => {
    const session = { elapsedMs: () => 900, stop: vi.fn(), discard: vi.fn(), captureSnapshot: vi.fn().mockResolvedValue(png("recording")) };
    const view = render(SnapshotCapture);
    handle(view).show();
    await fireEvent.click(await view.findByRole("radio", { name: "Holdmap window" }));
    await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
    handle(view).show();
    await waitFor(() => expect(view.getByRole("radio", { name: "Holdmap window" }).getAttribute("aria-checked")).toBe("true"));
    await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
    await view.rerender({ session });
    handle(view).show();
    await waitFor(() => expect(view.getByRole("radio", { name: "Holdmap view" }).getAttribute("aria-checked")).toBe("true"));
    expect(view.queryByRole("radio", { name: "Holdmap window" })).toBeNull();
    expect(view.getByRole("radio", { name: "Recording window" })).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Capture PNG" }));
    await view.findByRole("dialog", { name: "Snapshot preview" });
    expect(mocks.app).toHaveBeenCalledOnce();
    expect(mocks.screen).not.toHaveBeenCalled();
    expect(session.captureSnapshot).not.toHaveBeenCalled();
  });
  it("normalizes a cancelled recording source when video ends, preserving valid choices before that transition", async () => {
    const session = { elapsedMs: () => 900, stop: vi.fn(), discard: vi.fn(), captureSnapshot: vi.fn().mockResolvedValue(png("recording")) };
    const view = render(SnapshotCapture, { session });
    handle(view).show();
    await fireEvent.click(await view.findByRole("radio", { name: "Recording window" }));
    await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
    handle(view).show();
    await waitFor(() => expect(view.getByRole("radio", { name: "Recording window" }).getAttribute("aria-checked")).toBe("true"));
    await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
    await view.rerender({ session: null });
    handle(view).show();
    await waitFor(() => expect(view.getByRole("radio", { name: "Holdmap view" }).getAttribute("aria-checked")).toBe("true"));
    expect(view.queryByRole("radio", { name: "Recording window" })).toBeNull();
    expect(view.getByRole("radio", { name: "Holdmap window" })).toBeTruthy();
    const capture = view.getByRole("button", { name: "Capture PNG" }) as HTMLButtonElement;
    expect(capture.disabled).toBe(false);
    await fireEvent.click(capture);
    await view.findByRole("dialog", { name: "Snapshot preview" });
    expect(mocks.app).toHaveBeenCalledOnce();
    expect(mocks.screen).not.toHaveBeenCalled();
    expect(session.captureSnapshot).not.toHaveBeenCalled();
  });
  it("retains a PNG after closing, reports save failures, and permits retry", async () => {
    mocks.save.mockRejectedValueOnce(new Error("Disk full")).mockResolvedValueOnce({ saved: true, path: "/chosen/snapshot.png" });
    const view = render(SnapshotCapture);
    handle(view).show();
    await fireEvent.click(await view.findByRole("button", { name: "Capture PNG" }));
    await view.findByRole("dialog", { name: "Snapshot preview" });
    expect(mocks.app).toHaveBeenCalledWith(document.getElementById("holdmap-capture-root"), expect.any(AbortSignal));
    await fireEvent.click(view.getByRole("button", { name: "Download PNG" }));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Disk full"));
    await fireEvent.click(view.getByRole("button", { name: "Download PNG" }));
    await waitFor(() => expect(view.getByRole("status").textContent).toContain("Saved to /chosen/snapshot.png"));
    await fireEvent.click(view.getAllByRole("button", { name: "Close" }).at(-1)!);
    handle(view).show();
    await view.findByRole("dialog", { name: "Snapshot preview" });
    expect(mocks.app).toHaveBeenCalledOnce();
  });
  it("hides capture controls while the picker is pending and cancels a late result", async () => {
    let resolve!: (value: SnapshotResult) => void;
    mocks.screen.mockReturnValue(new Promise((done) => (resolve = done)));
    const status = vi.fn();
    const view = render(SnapshotCapture, { onstatechange: status });
    handle(view).show();
    await fireEvent.click(await view.findByRole("radio", { name: "Holdmap window" }));
    await fireEvent.click(view.getByRole("button", { name: "Capture window" }));
    expect(mocks.screen).toHaveBeenCalledOnce();
    await waitFor(() => expect(view.queryByRole("dialog")).toBeNull());
    expect(status).toHaveBeenLastCalledWith({ open: false, busy: true, hasResult: false });
    const signal = mocks.screen.mock.calls[0][1] as AbortSignal;
    handle(view).cancel();
    expect(signal.aborted).toBe(true);
    resolve(png("screen"));
    await waitFor(() => expect(status).toHaveBeenLastCalledWith({ open: false, busy: false, hasResult: false }));
    expect(view.queryByRole("dialog")).toBeNull();
  });
  it("borrows the active recording without stopping it and retains the PNG when video review opens", async () => {
    const session = { elapsedMs: () => 900, stop: vi.fn(), discard: vi.fn(), captureSnapshot: vi.fn().mockResolvedValue(png("recording")) };
    const view = render(SnapshotCapture, { session });
    handle(view).show();
    await fireEvent.click(await view.findByRole("radio", { name: "Recording window" }));
    await fireEvent.click(view.getByRole("button", { name: "Capture PNG" }));
    await view.findByRole("dialog", { name: "Snapshot preview" });
    expect(session.captureSnapshot).toHaveBeenCalledOnce();
    expect(session.stop).not.toHaveBeenCalled();
    expect(session.discard).not.toHaveBeenCalled();
    expect(mocks.screen).not.toHaveBeenCalled();
    expect(view.getByText("Your video is still recording.")).toBeTruthy();
    await view.rerender({ session: null, blocked: true });
    await waitFor(() => expect(view.queryByRole("dialog")).toBeNull());
    await view.rerender({ session: null, blocked: false });
    handle(view).show();
    await view.findByRole("dialog", { name: "Snapshot preview" });
  });
  it("waits for an outgoing command palette to be removed before borrowing the video", async () => {
    const palette = document.createElement("div");
    palette.setAttribute("role", "dialog"); palette.setAttribute("aria-label", "Command palette"); document.body.append(palette);
    const session = { elapsedMs: () => 900, stop: vi.fn(), discard: vi.fn(), captureSnapshot: vi.fn().mockResolvedValue(png("recording")) };
    const view = render(SnapshotCapture, { session });
    const pending = handle(view).captureRecording();
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(session.captureSnapshot).not.toHaveBeenCalled();
    palette.remove();
    await pending;
    await view.findByRole("dialog", { name: "Snapshot preview" });
    expect(session.captureSnapshot).toHaveBeenCalledOnce();
  });
  it.each(["cancel", "timeout"])("cleans up a %s while waiting for the outgoing palette", async (action) => {
    vi.useFakeTimers();
    const palette = document.createElement("div");
    palette.setAttribute("role", "dialog"); palette.setAttribute("aria-label", "Command palette"); document.body.append(palette);
    const session = { elapsedMs: () => 900, stop: vi.fn(), discard: vi.fn(), captureSnapshot: vi.fn() };
    const view = render(SnapshotCapture, { session });
    const pending = handle(view).captureRecording();
    await vi.advanceTimersByTimeAsync(0);
    if (action === "cancel") handle(view).cancel();
    else await vi.advanceTimersByTimeAsync(2_000);
    await pending;
    expect(session.captureSnapshot).not.toHaveBeenCalled();
    // Let Svelte paint the timeout's error dialog before checking pending work.
    await vi.advanceTimersByTimeAsync(50);
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
    if (action === "timeout") expect((await view.findByRole("alert")).textContent).toContain("Close the command palette");
    palette.remove();
    await Promise.resolve();
    expect(session.captureSnapshot).not.toHaveBeenCalled();
  });
  it("prevents closing or discarding during a native save and treats save cancellation quietly", async () => {
    let resolve!: (value: { saved: boolean }) => void;
    mocks.save.mockReturnValue(new Promise((done) => (resolve = done)));
    const view = render(SnapshotCapture);
    handle(view).show();
    await fireEvent.click(await view.findByRole("button", { name: "Capture PNG" }));
    const dialog = await view.findByRole("dialog", { name: "Snapshot preview" });
    await fireEvent.click(view.getByRole("button", { name: "Download PNG" }));
    expect((view.getByRole("button", { name: "Discard snapshot" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(view.getByRole("dialog", { name: "Snapshot preview" })).toBeTruthy();
    resolve({ saved: false });
    await waitFor(() => expect((view.getByRole("button", { name: "Download PNG" }) as HTMLButtonElement).disabled).toBe(false));
    expect(view.queryByRole("alert")).toBeNull();
    expect(view.queryByRole("status")).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "Discard snapshot" }));
    await waitFor(() => expect(view.queryByRole("dialog")).toBeNull());
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:png");
  });
});
