// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import RecordingControls from "./RecordingControls.svelte";
import type { RecordingStatus } from "../../../lib/agent-flow/model";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
const recording = (active: boolean): RecordingStatus => ({ active, endpoint: null, started_at_ms: active ? 1000 : null, received: 0, dropped: 0 });
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("recording operation controls", () => {
  it("waits for recording status before offering a start operation", async () => {
    const start = vi.fn().mockResolvedValue(undefined);
    const { container, getByRole, rerender } = render(RecordingControls, { statusLoading: true, onstartrecording: start });
    expect(container.textContent).toContain("Checking recording status");
    expect(container.textContent).not.toContain("Recording off");
    const button = getByRole("button", { name: "Start recording" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    button.click();
    expect(start).not.toHaveBeenCalled();

    await rerender({ statusLoading: false, recording: recording(false), onstartrecording: start });
    expect(container.textContent).toContain("Recording off");
    expect(button.disabled).toBe(false);
  });

  it("explains an unavailable status without guessing that recording stopped", () => {
    const { container, getByRole } = render(RecordingControls, { statusError: true, onstartrecording: vi.fn() });
    expect(container.textContent).toContain("Recording status unavailable");
    expect(container.textContent).toContain("Refresh to check recording status");
    expect(container.textContent).not.toContain("Recording off");
    expect((getByRole("button", { name: "Start recording" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("keeps the last known active status and permits an explicit stop after a refresh failure", async () => {
    const stop = vi.fn().mockRejectedValue(new Error("private-collector-token"));
    const { container, getByRole } = render(RecordingControls, { recording: recording(true), statusError: true, onstoprecording: stop });
    expect(container.textContent).toContain("Recording on this computer");
    expect(container.textContent).toContain("Last known recording status");
    expect(container.textContent).not.toContain("Recording off");
    await fireEvent.click(getByRole("button", { name: "Stop recording" }));
    await waitFor(() => expect(container.textContent).toContain("That action could not be completed"));
    expect(stop).toHaveBeenCalledOnce();
    expect(container.innerHTML).not.toContain("private-collector-token");
    expect(container.textContent).not.toContain("Recording stopped.");
    expect((getByRole("button", { name: "Stop recording" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("blocks repeated operations while waiting and uses controlled status after completion", async () => {
    const pending = deferred<void>(), stop = vi.fn(() => pending.promise);
    const { container, getByRole, rerender } = render(RecordingControls, { recording: recording(true), onstoprecording: stop });
    const button = getByRole("button", { name: "Stop recording" }) as HTMLButtonElement;
    await fireEvent.click(button);
    expect(button.disabled).toBe(true);
    button.click();
    expect(stop).toHaveBeenCalledOnce();
    expect(container.textContent).not.toContain("Recording stopped.");
    await rerender({ recording: recording(false), onstoprecording: stop });
    pending.resolve();
    await waitFor(() => expect(container.textContent).toContain("Recording stopped. Captured metadata remains available."));
    expect(container.textContent).toContain("Recording off");
  });

  it("requires separate clear confirmation and does not clear on cancellation", async () => {
    const clear = vi.fn().mockResolvedValue(undefined);
    const { container, getByRole } = render(RecordingControls, { records: 3, recording: recording(true), onclearactivity: clear });
    await fireEvent.click(getByRole("button", { name: "Clear records" }));
    expect(clear).not.toHaveBeenCalled();
    expect(container.textContent).toContain("Recording stays as it is");
    await fireEvent.click(getByRole("button", { name: "Cancel" }));
    expect(clear).not.toHaveBeenCalled();
    await fireEvent.click(getByRole("button", { name: "Clear records" }));
    await fireEvent.click(getByRole("button", { name: "Clear captured activity" }));
    await waitFor(() => expect(container.textContent).toContain("Captured activity cleared."));
    expect(clear).toHaveBeenCalledOnce();
    expect(container.textContent).toContain("Recording on this computer");
  });

  it("discards late private setup configuration after controls are disposed", async () => {
    const pending = deferred<string>(), setup = vi.fn(() => pending.promise), writeText = vi.fn();
    vi.stubGlobal("ClipboardItem", undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const { container, getByRole, unmount } = render(RecordingControls, { recording: recording(true), onactivitysetup: setup });
    await fireEvent.click(getByRole("button", { name: "Setup" }));
    expect(setup).not.toHaveBeenCalled();
    await fireEvent.click(getByRole("button", { name: "Copy Codex settings" }));
    expect(setup).toHaveBeenCalledWith("codex");
    unmount();
    pending.resolve("private-local-setup-token");
    await new Promise((done) => setTimeout(done, 0));
    expect(writeText).not.toHaveBeenCalled();
    expect(container.innerHTML).not.toContain("private-local-setup-token");
  });
});
