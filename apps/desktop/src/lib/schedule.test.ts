import { describe, expect, it, vi } from "vitest";
import { DEFAULT_SCHEDULE, intervalLabel, requestFrom, untilLabel, widens } from "./schedule";
import { ScheduleDriver, TICK_MS, type DriverDeps } from "./schedule-driver.svelte";
import type { Due, ScheduleStatus } from "./schedule";

const status = (enabled: boolean, next: number | null): ScheduleStatus => ({ schedule: { ...DEFAULT_SCHEDULE, enabled, analyzers: ["a"] }, plan: { every_hours: 24, analyzers: [], network: "none", runs_when: "", notifies: "" }, next_ms: next, power: "ac" });

function harness(dues: Due[]) {
  let clock = 1_000;
  let tick: (() => void) | null = null;
  const ran: string[][] = [];
  const finished: number[] = [];
  let busy = false;
  const deps: DriverDeps = {
    due: vi.fn(async (): Promise<Due> => dues.shift() ?? { kind: "off" }),
    finished: vi.fn(async (t: number) => { finished.push(t); }),
    status: vi.fn(async () => status(true, clock + 24 * 3_600_000)),
    analyzers: () => [{ id: "a", name: "A", state: "approved" }, { id: "b", name: "B", state: "not_approved" }],
    run: vi.fn(async (list: { id: string }[]) => { ran.push(list.map((a) => a.id)); clock += 5_000; }),
    busy: () => busy,
    now: () => clock,
    every: (fn, ms) => { expect(ms).toBe(TICK_MS); tick = fn; return () => (tick = null); },
  };
  return { deps, ran, finished, fire: () => tick?.(), setBusy: (b: boolean) => (busy = b), stopped: () => tick === null };
}

describe("schedule wording and rules", () => {
  it("labels intervals and the next scan", () => {
    expect(intervalLabel(24)).toBe("Daily");
    expect(intervalLabel(168)).toBe("Weekly");
    expect(untilLabel(null, 0)).toBe("off");
    expect(untilLabel(30_000, 0)).toBe("due now");
    expect(untilLabel(5 * 60_000, 0)).toBe("in 5 min");
    expect(untilLabel(5 * 3_600_000, 0)).toBe("in 5 h");
    expect(untilLabel(7 * 86_400_000, 0)).toBe("in 7 days");
  });
  it("needs a fresh confirmation only when it widens", () => {
    const on = { ...DEFAULT_SCHEDULE, enabled: true, analyzers: ["a"] };
    expect(widens(DEFAULT_SCHEDULE, requestFrom(on))).toBe(true);
    expect(widens(on, requestFrom(on, { every_hours: 168 }))).toBe(false);
    expect(widens(on, requestFrom(on, { every_hours: 6 }))).toBe(true);
    expect(widens(on, requestFrom(on, { analyzers: ["a", "b"] }))).toBe(true);
    expect(widens(on, requestFrom(on, { skip_on_battery: false }))).toBe(true);
    expect(widens(on, requestFrom(on, { enabled: false }))).toBe(false);
  });
});

describe("ScheduleDriver (fake clock)", () => {
  it("runs only approved, due analyzers through the dock and reports back once", async () => {
    const h = harness([{ kind: "wait", next_ms: 5 }, { kind: "run", analyzers: ["a", "b"] }]);
    const d = new ScheduleDriver(h.deps);
    d.start();
    await d.tick();
    expect(h.ran).toEqual([]);
    await d.tick();
    expect(h.ran).toEqual([["a"]]);
    expect(h.finished).toEqual([1_000]);
    expect(d.lastRunAt).toBe(1_000);
    expect(d.enabled).toBe(true);
    d.dispose();
    expect(h.stopped()).toBe(true);
  });
  it("records skips (battery) and never overlaps a user's run", async () => {
    const h = harness([{ kind: "skip", reason: "On battery." }, { kind: "run", analyzers: ["a"] }]);
    const d = new ScheduleDriver(h.deps);
    await d.tick();
    expect(d.skipped).toBe("On battery.");
    h.setBusy(true);
    await d.tick();
    expect(h.deps.due).toHaveBeenCalledTimes(1);
    h.setBusy(false);
    await d.tick();
    expect(d.skipped).toBeNull();
    expect(h.ran).toEqual([["a"]]);
  });
  it("is off until turned on", async () => {
    const h = harness([]);
    const d = new ScheduleDriver(h.deps);
    await d.tick();
    expect(h.ran).toEqual([]);
    expect(h.finished).toEqual([]);
  });
});

describe("snooze limits", async () => {
  const { clampSnooze, dateBounds, untilFromDate, MAX_SNOOZE_MS, MIN_SNOOZE_MS } = await import("./snooze");
  const now = new Date(2026, 9, 26, 10, 0).getTime();
  it("clamps to 1 hour – 90 days", () => {
    expect(clampSnooze(now, now)).toBe(now + MIN_SNOOZE_MS);
    expect(clampSnooze(now + 10 * MAX_SNOOZE_MS, now)).toBe(now + MAX_SNOOZE_MS);
  });
  it("accepts custom dates from tomorrow to 90 days ahead only", () => {
    expect(dateBounds(now)).toEqual({ min: "2026-10-27", max: "2027-01-24" });
    expect(untilFromDate("2026-10-27", now)).toBe(new Date(2026, 9, 27, 23, 59, 59, 999).getTime());
    for (const bad of ["2026-10-26", "2027-01-25", "2026-02-30", "soon"]) expect(untilFromDate(bad, now)).toBeNull();
  });
});

describe("demo trust and schedule", async () => {
  const { DemoTrust } = await import("./trust-demo");
  const view = (id: string, network: string[] = [], state = "approved") => ({ id, name: id, kind: "cli", state, permissions: { read: [], network, network_reason: null, sends: [], spawns_mcp_servers: false } }) as never;
  it("shows changed and new skills first, and trusting clears the alert", () => {
    const t = new DemoTrust();
    const r = t.report();
    expect(r.skills[0].state).toBe("changed");
    expect(r.skills[0].changes?.modified[0].path).toBe("SKILL.md");
    expect([r.changed, r.untrusted]).toEqual([1, 1]);
    t.set(r.skills[0].subject.id, true, "checked");
    expect(t.report().changed).toBe(0);
    expect(new DemoTrust(true).report().skills).toEqual([]);
  });
  it("needs confirmation and offline analyzers to turn the schedule on", () => {
    const t = new DemoTrust();
    const views = [view("offline"), view("online", ["api.osv.dev"])];
    const req = { enabled: true, every_hours: 24, analyzers: ["offline"], mcp_opt_in: [], skip_on_battery: true, notify: true, quiet: { start: 22, end: 7 }, confirmed: false };
    expect(() => t.setSchedule(views, req)).toThrow(/confirm/);
    expect(() => t.setSchedule(views, { ...req, analyzers: ["online"], confirmed: true })).toThrow(/network/);
    const st = t.setSchedule(views, { ...req, confirmed: true });
    expect(st.schedule.enabled).toBe(true);
    expect(st.next_ms).toBeGreaterThan(Date.now());
    expect(t.due(views).kind).toBe("wait");
  });
});
