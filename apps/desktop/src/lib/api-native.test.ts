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
