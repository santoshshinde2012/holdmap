import { describe, expect, it } from "vitest";
import { createFlowLoader } from "./live";

function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

describe("Flow request generations", () => {
  it("shares in-flight requests and drops a response after pause or navigation", async () => {
    const first = deferred<number>(), second = deferred<number>();
    const applied: number[] = [], errors: (string | null)[] = [], busy: boolean[] = [];
    let calls = 0;
    const loader = createFlowLoader(() => (++calls === 1 ? first.promise : second.promise), (value) => applied.push(value), (value) => errors.push(value), (value) => busy.push(value));
    const old = loader.request();
    expect(loader.request()).toBe(old);
    await Promise.resolve();
    loader.invalidate();
    const fresh = loader.request();
    await Promise.resolve();
    first.resolve(1);
    await old;
    expect(applied).toEqual([]);
    expect(busy.at(-1)).toBe(true);
    second.resolve(2);
    await fresh;
    expect(applied).toEqual([2]);
    expect(errors).toEqual([null]);
    expect(busy.at(-1)).toBe(false);
  });

  it("ignores a stale failure while reporting a current failure without clearing data", async () => {
    const first = deferred<number>(), second = deferred<number>();
    const applied: number[] = [], errors: (string | null)[] = [];
    let calls = 0;
    const loader = createFlowLoader(() => (++calls === 1 ? first.promise : second.promise), (value) => applied.push(value), (value) => errors.push(value), () => {});
    const old = loader.request();
    await Promise.resolve();
    loader.invalidate();
    const fresh = loader.request();
    await Promise.resolve();
    first.reject(new Error("stale"));
    await old;
    expect(errors).toEqual([]);
    second.reject(new Error("current recorder unavailable"));
    await fresh;
    expect(errors).toEqual(["current recorder unavailable"]);
    expect(applied).toEqual([]);
  });
});
