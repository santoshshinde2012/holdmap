import { describe, expect, it } from "vitest";
import { activeScheduleCount, DemoSchedules, resolveScheduleDelay, scheduleCountdown, validateScheduleDelay, validateScheduledStops } from "./schedules";

const request = (target = "3000/tcp", confirmation_id = "review-1") => ({ target, confirmation_id, force: false, allow_protected: false });

describe("schedule authorization and timing", () => {
  it("allows only whole-second delays within the one-minute to one-day limit", () => {
    for (const value of [0, -1, 59, 86401, 60.5, NaN, Infinity]) expect(() => validateScheduleDelay(value)).toThrow();
    for (const value of [60, 3600, 86400]) expect(() => validateScheduleDelay(value)).not.toThrow();
  });

  it("requires unique protocol-qualified ports and reviewed handles without force or protection overrides", () => {
    expect(() => validateScheduledStops([request(), request("5353/udp", "review-2")])).not.toThrow();
    for (const requests of [[], [request("3000")], [request("0/tcp")], [request("65536/udp")], [request("3000/tcp", "")], [request(), request()], [request(), request("3001/tcp")], [{ ...request(), force: true }], [{ ...request(), allow_protected: true }], Array.from({ length: 33 }, (_, i) => request(`${3000 + i}/tcp`, `review-${i}`))]) {
      expect(() => validateScheduledStops(requests)).toThrow();
    }
  });

  it("formats useful countdowns without negative times", () => {
    expect(scheduleCountdown(3600000, 0)).toBe("In 1h 0m");
    expect(scheduleCountdown(61000, 0)).toBe("In 1m 1s");
    expect(scheduleCountdown(900, 0)).toBe("In 1s");
    expect(scheduleCountdown(1000, 2000)).toBe("Due now");
  });

  it("resolves an absolute deadline at confirmation and rejects a time that became invalid", () => {
    expect(resolveScheduleDelay(3600, 3_600_500, 1000)).toBe(3600);
    expect(resolveScheduleDelay(3600, 3_600_500, 60_000)).toBe(3541);
    expect(() => resolveScheduleDelay(3600, 3_600_500, 3_550_000)).toThrow("1 minute");
    expect(() => resolveScheduleDelay(3600, NaN, 1000)).toThrow("1 minute");
  });
});

describe("effect-free browser schedule session", () => {
  function session() {
    let wall = 100000;
    let mono = 1000;
    const scheduler = new DemoSchedules(() => wall, () => mono);
    return { scheduler, advance(ms: number) { wall += ms; mono += ms; }, shift(ms: number) { wall += ms; } };
  }

  it("simulates a due action once and emits independent copies", () => {
    const { scheduler, advance } = session();
    const observed: string[] = [];
    const unsubscribe = scheduler.subscribe((actions) => observed.push(actions[0].status));
    const action = scheduler.create("port", "Stop :3000", ["3000/tcp"], 60);
    action.targets[0] = "changed";
    expect(scheduler.list()[0].targets).toEqual(["3000/tcp"]);
    advance(59999); scheduler.tick();
    expect(scheduler.list()[0].status).toBe("pending");
    advance(1); scheduler.tick(); scheduler.tick();
    expect(observed).toEqual(["pending", "completed"]);
    expect(scheduler.list()[0].message).toContain("not changed");
    expect(activeScheduleCount(scheduler.list())).toBe(0);
    unsubscribe();
  });

  it("cancels only pending actions and never presents cancellation after the deadline as success", () => {
    const { scheduler, advance } = session();
    const cancelled = scheduler.create("shutdown", "Shutdown", [], 60);
    expect(scheduler.cancel(cancelled.id).status).toBe("cancelled");
    expect(() => scheduler.cancel(cancelled.id)).toThrow("no longer");
    const due = scheduler.create("shutdown", "Shutdown", [], 60);
    advance(60000);
    expect(() => scheduler.cancel(due.id)).toThrow("no longer");
    expect(scheduler.list()[0].status).toBe("completed");
  });

  it("requires a fresh review after a missed deadline and never retries the action", () => {
    const { scheduler, advance } = session();
    scheduler.create("port", "Stop", ["3000/tcp"], 60);
    advance(120001); scheduler.tick();
    expect(scheduler.list()[0].status).toBe("needs_review");
    expect(scheduler.list()[0].message).toContain("deadline was missed");
    advance(5000); scheduler.tick();
    expect(scheduler.list()[0].status).toBe("needs_review");
  });

  it("detects forward and backward wall-clock jumps before a future deadline", () => {
    for (const drift of [-30001, 30001]) {
      const { scheduler, shift } = session();
      scheduler.create("shutdown", "Shutdown", [], 3600);
      shift(drift); scheduler.tick();
      expect(scheduler.list()[0].status).toBe("needs_review");
      expect(scheduler.list()[0].message).toContain("clock changed");
    }
  });

  it("limits pending actions and allows only one scheduled shutdown", () => {
    const { scheduler } = session();
    scheduler.create("shutdown", "Shutdown", [], 60);
    expect(() => scheduler.create("shutdown", "Shutdown again", [], 60)).toThrow("already scheduled");
    for (let i = 0; i < 31; i++) scheduler.create("port", `Stop ${i}`, [`${3000 + i}/tcp`], 60);
    expect(() => scheduler.create("port", "Extra", ["9000/tcp"], 60)).toThrow("Cancel a pending");
  });

  it.each(["completion", "cancellation"])("retains at most 64 finished actions after %s without needing another creation", (transition) => {
    const { scheduler, advance } = session();
    let firstId = "";
    for (let batch = 0; batch < 2; batch++) {
      for (let i = 0; i < 32; i++) {
        const created = scheduler.create("port", `Stop ${batch}-${i}`, [`${3000 + i}/tcp`], 60);
        if (!batch && !i) firstId = created.id;
      }
      advance(60000); scheduler.tick();
    }
    const pending = Array.from({ length: 32 }, (_, i) => scheduler.create("port", `Latest ${i}`, [`${4000 + i}/tcp`], 60));
    expect(scheduler.list()).toHaveLength(96);
    const terminalCounts: number[] = [];
    scheduler.subscribe((actions) => terminalCounts.push(actions.filter((action) => action.status !== "pending").length));
    if (transition === "completion") { advance(60000); scheduler.tick(); }
    else for (const action of pending) scheduler.cancel(action.id);
    expect(scheduler.list()).toHaveLength(64);
    expect(activeScheduleCount(scheduler.list())).toBe(0);
    expect(terminalCounts.every((count) => count <= 64)).toBe(true);
    expect(scheduler.list().some((action) => action.id === firstId)).toBe(false);
    expect(pending.every((action) => scheduler.list().some((item) => item.id === action.id))).toBe(true);
  });
});
