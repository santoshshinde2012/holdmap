import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { StopReport } from "./types";

const effects = vi.hoisted(() => ({ targets: [] as string[], failure: false, historyFailure: false }));
vi.mock("./mock", async (original) => {
  const real = await original<typeof import("./mock")>();
  return {
    ...real,
    mockStop: (target: string): StopReport => {
      effects.targets.push(target);
      return { target, success: !effects.failure, freed: true, signalled: [], survivors: [], escalated: false, ports_still_busy: [], elapsed_ms: 1, log: [], error: effects.failure ? "A reviewed step failed" : null, history_error: effects.historyFailure ? "History could not be saved" : null };
    },
  };
});

beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal("window", {});
  vi.useFakeTimers();
  effects.targets = [];
  effects.failure = false;
  effects.historyFailure = false;
});
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

async function reviewedRequests() {
  const api = await import("./api");
  const targets = ["3000/tcp", "5173/tcp"];
  const plans = await Promise.all(targets.map((target) => api.plan(target, false, false)));
  return { api, requests: targets.map((target, index) => ({ target, confirmation_id: plans[index].confirmation_id!, force: false, allow_protected: false })) };
}

describe("browser agent batch simulation", () => {
  it("checks every handle before the first simulated stop and consumes attempted reviews", async () => {
    const { api, requests } = await reviewedRequests();
    const invalid = [{ ...requests[0] }, { ...requests[1], confirmation_id: "missing-review" }];
    await expect(api.stopMany("agent:52000", invalid)).rejects.toThrow("expired");
    expect(effects.targets).toEqual([]);
    await expect(api.stopMany("agent:52000", requests)).rejects.toThrow("already used");
    expect(effects.targets).toEqual([]);
  });

  it("returns all simulated results after one complete review", async () => {
    const { api, requests } = await reviewedRequests();
    const pending = api.stopMany("agent:52000", requests);
    await vi.advanceTimersByTimeAsync(1500);
    expect(await pending).toHaveLength(2);
    expect(effects.targets).toEqual(["3000/tcp", "5173/tcp"]);
    await expect(api.stopMany("agent:52000", requests)).rejects.toThrow("already used");
  });

  it.each(["failure", "historyFailure"] as const)("dispatches no later simulation after %s", async (failure) => {
    const { api, requests } = await reviewedRequests();
    effects[failure] = true;
    const pending = api.stopMany("agent:52000", requests);
    await vi.advanceTimersByTimeAsync(1500);
    expect(await pending).toHaveLength(1);
    expect(effects.targets).toEqual(["3000/tcp"]);
  });

  it("rejects an agent that does not own the reviewed listeners", async () => {
    const { api, requests } = await reviewedRequests();
    await expect(api.stopMany("agent:48000", requests)).rejects.toThrow("no longer belong");
    expect(effects.targets).toEqual([]);
  });
});
