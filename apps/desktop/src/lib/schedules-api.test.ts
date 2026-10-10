import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const request = (confirmation_id: string, target = "3000/tcp") => ({ target, confirmation_id, force: false, allow_protected: false });

beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  invoke.mockReset();
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
});
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("native scheduled action API", () => {
  it("forwards reviewed plans and delay without authority over remote hosts", async () => {
    const api = await import("./api");
    await api.scheduleShutdown("power-1", 3600);
    await api.scheduleStops([request("stop-1")], 1800, "agent-1");
    await api.listSchedules();
    await api.cancelSchedule("schedule-1");
    expect(invoke.mock.calls).toEqual([
      ["schedule_shutdown", { confirmationId: "power-1", delaySecs: 3600 }],
      ["schedule_stops", { requests: [request("stop-1")], delaySecs: 1800, agentId: "agent-1" }],
      ["list_schedules", undefined],
      ["cancel_schedule", { id: "schedule-1" }],
    ]);
  });

  it("rejects invalid time, missing review and destructive overrides before IPC", async () => {
    const api = await import("./api");
    await expect(api.scheduleShutdown("", 3600)).rejects.toThrow("Review shutdown");
    await expect(api.scheduleShutdown("power-1", 59)).rejects.toThrow("1 minute");
    await expect(api.scheduleStops([{ ...request("stop-1"), force: true }], 3600)).rejects.toThrow("Force-stop requests");
    await expect(api.scheduleStops([request("stop-1", "3000")], 3600)).rejects.toThrow("specific local port");
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("browser schedule authorization", () => {
  beforeEach(() => vi.stubGlobal("window", {}));

  it("consumes a shutdown review when scheduling and refuses replay or immediate shutdown afterward", async () => {
    const api = await import("./api");
    const preview = await api.shutdownPreview();
    const action = await api.scheduleShutdown(preview.confirmation_id, 3600);
    expect(action.kind).toBe("shutdown");
    await expect(api.scheduleShutdown(preview.confirmation_id, 3600)).rejects.toThrow("already used");
    await expect(api.shutdownMachine(preview.confirmation_id)).rejects.toThrow("already used");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("expires shutdown reviews at sixty seconds and port reviews at five minutes", async () => {
    const api = await import("./api");
    const power = await api.shutdownPreview();
    const stop = await api.plan("3000/tcp", false);
    await vi.advanceTimersByTimeAsync(60000);
    await expect(api.scheduleShutdown(power.confirmation_id, 3600)).rejects.toThrow("expired");
    await expect(api.scheduleStops([request(stop.confirmation_id!)], 3600)).resolves.toMatchObject({ status: "pending" });
    const next = await api.plan("3000/tcp", false);
    await vi.advanceTimersByTimeAsync(300000);
    await expect(api.scheduleStops([request(next.confirmation_id!)], 3600)).rejects.toThrow("expired");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("validates every reviewed plan before consuming any part of a batch", async () => {
    const api = await import("./api");
    const reviewed = await api.plan("3000/tcp", false);
    await expect(api.scheduleStops([request(reviewed.confirmation_id!), request("invented", "3001/tcp")], 60)).rejects.toThrow("reviewed stop plan");
    expect(await api.listSchedules()).toEqual([]);
    const action = await api.scheduleStops([request(reviewed.confirmation_id!)], 60);
    expect(action.status).toBe("pending");
    await expect(api.scheduleStops([request(reviewed.confirmation_id!)], 60)).rejects.toThrow("already used");
    await expect(api.stop("3000/tcp", false, false, reviewed.confirmation_id)).rejects.toThrow("already used");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("binds authorization to the reviewed target and broadcasts simulation completion while the manager is closed", async () => {
    const api = await import("./api");
    const reviewed = await api.plan("3000/tcp", false);
    await expect(api.scheduleStops([request(reviewed.confirmation_id!, "3001/tcp")], 60)).rejects.toThrow("changed");
    const changes: string[] = [];
    const unsubscribe = await api.onEvent<import("./schedules").ScheduledAction[]>("schedules-changed", (items) => changes.push(items[0].status));
    await api.scheduleStops([request(reviewed.confirmation_id!)], 60, "agent-fixture");
    await vi.advanceTimersByTimeAsync(60000);
    expect(changes).toEqual(["pending", "completed"]);
    expect((await api.listSchedules())[0].message).toContain("not changed");
    unsubscribe();
    expect(invoke).not.toHaveBeenCalled();
  });
});
