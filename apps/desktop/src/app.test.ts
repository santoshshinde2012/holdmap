// @vitest-environment jsdom
// Regression for the blinking details pane: every scan used to throw the explanations away, so
// the selected port's Overview fell back to its skeleton until `explain` answered again.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";

const calls = vi.hoisted(() => ({ explain: 0, http: 0, scan: 0 }));
vi.mock("./lib/api", async (orig) => {
  const real = await orig<typeof import("./lib/api")>();
  const slow = <T,>(v: T, ms: number) => new Promise<T>((r) => setTimeout(() => r(v), ms));
  return {
    ...real,
    // Each scan is a fresh object graph, with the memory figure changing like a real poll.
    scan: async (all: boolean) => { calls.scan++; const s = structuredClone(await real.scan(all)); s.entries.forEach((e) => e.process && (e.process.memory_bytes += calls.scan * 4096)); return s; },
    explain: async (port: number) => { calls.explain++; return slow(structuredClone(await real.explain(port)), 400); },
    http: async (port: number) => { calls.http++; return real.http(port); },
  };
});
import App from "./App.svelte";

beforeAll(() => {
  window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  const anim = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(anim);
  Element.prototype.scrollTo ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
  window.requestAnimationFrame ??= (f) => setTimeout(() => f(0), 16) as unknown as number;
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe("App live polling", () => {
  it("keeps the selected port's details on screen across scans", async () => {
    vi.useFakeTimers();
    const { container } = render(App);
    await vi.advanceTimersByTimeAsync(600);
    const row = container.querySelector<HTMLElement>("#row-tcp\\:3000") ?? container.querySelector<HTMLElement>("[role=option]")!;
    await fireEvent.click(row);
    await vi.advanceTimersByTimeAsync(1000);
    const pane = container.querySelector("aside.pane")!;
    const headline = pane.querySelector(".headline")!;
    expect(headline).not.toBeNull();
    const explains = calls.explain;

    let skeleton = 0;
    const mo = new MutationObserver(() => { if (pane.querySelector(".sk") || !pane.querySelector(".headline")) skeleton++; });
    mo.observe(pane, { childList: true, subtree: true });
    // Three scans at the default 3 s cadence, stepped finely enough to catch a flash.
    for (let i = 0; i < 100; i++) await vi.advanceTimersByTimeAsync(100);
    mo.disconnect();

    expect(calls.scan).toBeGreaterThanOrEqual(3);
    expect(calls.explain).toBeGreaterThan(explains); // still revalidated in the background…
    expect(skeleton).toBe(0); // …without ever falling back to the skeleton
    expect(pane.querySelector(".headline")).toBe(headline); // or remounting the summary
  });
});
