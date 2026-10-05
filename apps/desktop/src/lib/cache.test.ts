import { describe, expect, it } from "vitest";
import { LruMap } from "./cache";

describe("LruMap", () => {
  it("evicts the least-recently-used key past the cap", () => {
    const m = new LruMap<number, string>(2);
    m.set(1, "a");
    m.set(2, "b");
    m.set(3, "c");
    expect(m.has(1)).toBe(false);
    expect(m.get(2)).toBe("b");
    expect(m.get(3)).toBe("c");
  });

  it("keeps a preferred key even when it is the oldest", () => {
    const m = new LruMap<number, string>(2);
    m.set(1, "a");
    m.set(2, "b");
    m.set(3, "c", new Set([1]));
    expect(m.has(1)).toBe(true);
    expect(m.has(2)).toBe(false);
    expect(m.has(3)).toBe(true);
  });

  it("prune drops keys the predicate rejects", () => {
    const m = new LruMap<number, string>(8);
    m.set(1, "a");
    m.set(2, "b");
    m.prune((k) => k === 2);
    expect([...m.keys()]).toEqual([2]);
  });
});
