// The build's deepmerge is vendor/deepmerge (an npm override): deepmerge 4.3.1's API without its
// prototype poisoning (GHSA-gcqx-w48m-w2h4). These pin both its behaviour and its safety.
import { describe, expect, it } from "vitest";
import deepmerge from "deepmerge";

type Any = Record<string, unknown>;

describe("vendored deepmerge", () => {
  it("merges like deepmerge 4.3.1: nested objects, arrays concatenated, inputs untouched", () => {
    const x = { a: { b: 1 }, list: [1], keep: "x" };
    const y = { a: { c: 2 }, list: [2], add: true };
    const out = deepmerge<Any, Any>(x, y);
    expect(out).toEqual({ a: { b: 1, c: 2 }, list: [1, 2], keep: "x", add: true });
    expect(out.a).not.toBe(x.a);
    expect(x).toEqual({ a: { b: 1 }, list: [1], keep: "x" });
    // A type mismatch takes the source, cloned.
    expect(deepmerge<Any, Any>({ a: [1] }, { a: { b: 1 } })).toEqual({ a: { b: 1 } });
  });

  it("honours every option vite-plugin-svelte and others pass", () => {
    // vite-plugin-svelte replaces arrays instead of concatenating them.
    const replace = { arrayMerge: (target: unknown[], source: unknown[]) => source ?? target };
    expect(deepmerge<Any, Any>({ list: [1, 2], o: { a: 1 } }, { list: [3], o: { b: 2 } }, replace)).toEqual({ list: [3], o: { a: 1, b: 2 } });
    const inner = { deep: true };
    expect((deepmerge<Any, Any>({}, { inner }, { clone: false }) as Any).inner).toBe(inner);
    expect((deepmerge<Any, Any>({}, { inner }) as Any).inner).not.toBe(inner);
    const joined = deepmerge<Any, Any>({ name: "a", o: { x: 1 } }, { name: "b", o: { y: 2 } }, { customMerge: (key) => (key === "name" ? (a: string, b: string) => `${a}+${b}` : undefined) });
    expect(joined.o).toEqual({ x: 1, y: 2 });
    // Dates and regular expressions are values, not objects to merge.
    const when = new Date(0);
    expect(deepmerge<Any, Any>({ when: new Date(1) }, { when }).when).toBe(when);
    class Atom { constructor(readonly v: number) {} }
    const atom = new Atom(1);
    expect(deepmerge<Any, Any>({ a: { v: 0 } }, { a: atom }, { isMergeableObject: (v) => !!v && typeof v === "object" && !(v instanceof Atom) }).a).toBe(atom);
    expect(deepmerge.all([{ a: 1 }, { b: 2 }, { a: 3 }])).toEqual({ a: 3, b: 2 });
    expect(() => deepmerge.all({} as unknown as object[])).toThrow("first argument should be an array");
    const tag = Symbol("tag");
    expect((deepmerge<Any, Any>({}, { [tag]: 1 }) as Any)[tag as unknown as string]).toBe(1);
  });

  it("never lets a merged key reach a prototype", () => {
    const evil = JSON.parse('{"__proto__": {"polluted": "yes"}, "a": 1}');
    const out = deepmerge<Any, Any>({}, evil);
    expect(Object.getPrototypeOf(out)).toBe(Object.prototype);
    expect(out.polluted).toBeUndefined();
    expect(Object.keys(out)).toEqual(["a"]);
    expect(({} as Any).polluted).toBeUndefined();
    // Nested, and on the target side.
    const nested = deepmerge<Any, Any>({ inner: {} }, JSON.parse('{"inner": {"__proto__": {"x": 1}}}'));
    expect(Object.getPrototypeOf(nested.inner)).toBe(Object.prototype);
    expect((nested.inner as Any).x).toBeUndefined();
    expect(deepmerge<Any, Any>(JSON.parse('{"__proto__": {"y": 1}}'), {}).y).toBeUndefined();
    // "constructor" and "prototype" stay plain data on the result.
    const data = deepmerge<Any, Any>({}, { constructor: { prototype: { z: 1 } } });
    expect(({} as Any).z).toBeUndefined();
    expect(Object.getPrototypeOf(data)).toBe(Object.prototype);
  });
});
