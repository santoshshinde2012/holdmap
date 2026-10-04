import { describe, expect, it } from "vitest";
import { pruned, share, withKey } from "./live";

describe("share", () => {
  const a = () => ({ entries: [{ id: "tcp:1", port: 1, process: { pid: 5, memory_bytes: 10 } }, { id: "tcp:2", port: 2, process: null }], taken_at_ms: 1 });

  it("returns the previous object when nothing changed", () => {
    const prev = a();
    expect(share(prev, a())).toBe(prev);
  });

  it("keeps unchanged items and rebuilds only the changed path", () => {
    const prev = a();
    const next = a();
    next.entries[0].process!.memory_bytes = 11;
    next.taken_at_ms = 2;
    const out = share(prev, next);
    expect(out).not.toBe(prev);
    expect(out).toEqual(next);
    expect(out.entries[1]).toBe(prev.entries[1]);
    expect(out.entries[0]).not.toBe(prev.entries[0]);
  });

  it("matches id-keyed items across a reorder", () => {
    const prev = a();
    const next = a();
    next.entries.reverse();
    const out = share(prev, next);
    expect(out.entries[0]).toBe(prev.entries[1]);
    expect(out.entries[1]).toBe(prev.entries[0]);
    expect(out.entries).not.toBe(prev.entries);
  });

  it("handles added and removed keys, nulls and primitives", () => {
    expect(share({ a: 1 } as Record<string, number>, { a: 1, b: 2 })).toEqual({ a: 1, b: 2 });
    const p = { a: 1, b: 2 } as Record<string, number>;
    expect(share(p, { a: 1 })).toEqual({ a: 1 });
    expect(share(null, { a: 1 })).toEqual({ a: 1 });
    expect(share({ a: null }, { a: { x: 1 } })).toEqual({ a: { x: 1 } });
    expect(share(3, 3)).toBe(3);
    expect(share([1, 2], [1, 2, 3])).toEqual([1, 2, 3]);
  });
});

describe("withKey / pruned", () => {
  it("keep the same record when nothing changes", () => {
    const r: Record<number, string> = { 1: "a", 2: "b" };
    expect(withKey(r, 1, "a")).toBe(r);
    expect(withKey(r, 1, "z")).toEqual({ 1: "z", 2: "b" });
    expect(pruned(r, () => true)).toBe(r);
    expect(pruned(r, (k) => k === 2)).toEqual({ 2: "b" });
  });
});

import { freshness } from "./live";
describe("freshness", () => {
  it("stays Live through an ordinary scan gap and only reports a real stall", () => {
    expect(freshness(null, 3)).toEqual({ text: "starting…", stale: false });
    for (const ago of [0, 3, 4, 5, 7]) expect(freshness(ago, 3)).toEqual({ text: "Live", stale: false });
    expect(freshness(8, 3)).toEqual({ text: "8s ago", stale: false });
    expect(freshness(12, 3)).toEqual({ text: "12s ago", stale: true });
    expect(freshness(20, 10).text).toBe("Live");
  });
});

import { dataAge, shouldPoll } from "./live";
describe("dataAge / shouldPoll", () => {
  it("counts from a scan in flight, so a running scan keeps the header Live", () => {
    expect(dataAge(10_000, null, null)).toBeNull();
    expect(dataAge(13_000, 1_000, null)).toBe(12);
    expect(dataAge(13_000, 1_000, 12_500)).toBe(1);
    expect(dataAge(13_000, 12_000, 2_000)).toBe(1); // an older in-flight start never makes it worse
  });
  it("only polls from the window when the watcher's pushes stop", () => {
    const base = { pushed: true, all: false, nowMs: 10_000, intervalMs: 4_000 };
    expect(shouldPoll({ ...base, takenAtMs: 8_000 })).toBe(false);
    expect(shouldPoll({ ...base, takenAtMs: 3_000 })).toBe(true);
    expect(shouldPoll({ ...base, takenAtMs: null })).toBe(true);
    expect(shouldPoll({ ...base, all: true, takenAtMs: 9_900 })).toBe(true);
    expect(shouldPoll({ ...base, pushed: false, takenAtMs: 9_900 })).toBe(true);
  });
});
