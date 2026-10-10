import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

beforeEach(() => {
  vi.resetModules();
  invoke.mockReset();
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
});
afterEach(() => vi.unstubAllGlobals());

describe("native stop confirmation contract", () => {
  it("refuses to invoke a stop without its reviewed plan handle", async () => {
    const api = await import("./api");
    await expect(api.stop("3000", false)).rejects.toThrow("Review a stop plan");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("forwards the returned handle with the exact target and authorization", async () => {
    const api = await import("./api");
    invoke.mockResolvedValueOnce({ confirmation_id: "fixture-42" });
    const reviewed = await api.plan("3000", true, true);
    invoke.mockResolvedValueOnce({ success: true });
    await api.stop("3000", true, true, reviewed.confirmation_id);
    expect(invoke).toHaveBeenNthCalledWith(1, "plan", { target: "3000", force: true, allowProtected: true });
    expect(invoke).toHaveBeenNthCalledWith(2, "stop", { target: "3000", force: true, allowProtected: true, confirmationId: "fixture-42" });
  });
});

describe("local shutdown confirmation contract", () => {
  it("refuses shutdown without a reviewed handle before invoking native IPC", async () => {
    const api = await import("./api");
    await expect(api.shutdownMachine("")).rejects.toThrow("Review shutdown");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("forwards only the reviewed handle, without a remote host or arbitrary options", async () => {
    const api = await import("./api");
    invoke.mockResolvedValueOnce({ confirmation_id: "power-42", hostname: "local", platform: "macos", expires_in_secs: 60 });
    const preview = await api.shutdownPreview();
    invoke.mockResolvedValueOnce({ requested: true });
    await api.shutdownMachine(preview.confirmation_id);
    expect(invoke).toHaveBeenNthCalledWith(1, "shutdown_preview", undefined);
    expect(invoke).toHaveBeenNthCalledWith(2, "shutdown_machine", { confirmationId: "power-42" });
  });

  it("browser simulation never invokes native and rejects replay", async () => {
    vi.stubGlobal("window", {});
    const api = await import("./api");
    const preview = await api.shutdownPreview();
    expect(preview.hostname).toBe("Demo computer");
    await expect(api.shutdownMachine(preview.confirmation_id)).resolves.toEqual({ requested: true });
    await expect(api.shutdownMachine(preview.confirmation_id)).rejects.toThrow("already used");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("browser simulation expires at the same sixty-second review boundary", async () => {
    vi.stubGlobal("window", {});
    const api = await import("./api");
    const clock = vi.spyOn(Date, "now").mockReturnValue(1000);
    try {
      const preview = await api.shutdownPreview();
      clock.mockReturnValue(61000);
      await expect(api.shutdownMachine(preview.confirmation_id)).rejects.toThrow("expired");
      expect(invoke).not.toHaveBeenCalled();
    } finally { clock.mockRestore(); }
  });
});
