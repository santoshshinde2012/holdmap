// @vitest-environment jsdom
// Regression for the blinking details pane: every scan used to throw the explanations away, so
// the selected port's Overview fell back to its skeleton until `explain` answered again.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { HttpInfo, Snapshot } from "./lib/types";
import { MOCK_SNAPSHOT } from "./lib/mock";

const calls = vi.hoisted(() => ({ explain: 0, http: 0, scan: 0, scanDelayMs: 0, deferHttp: false, httpPending: [] as ((value: HttpInfo | null) => void)[], events: new Map<string, (payload: unknown) => void>() }));
vi.mock("./lib/api", async (orig) => {
  const real = await orig<typeof import("./lib/api")>();
  const slow = <T,>(v: T, ms: number) => new Promise<T>((r) => setTimeout(() => r(v), ms));
  return {
    ...real,
    // Each scan is a fresh object graph, with the memory figure changing like a real poll.
    scan: async (all: boolean) => { calls.scan++; if (calls.scanDelayMs) await slow(null, calls.scanDelayMs); const s = structuredClone(await real.scan(all)); s.entries.forEach((e) => e.process && (e.process.memory_bytes += calls.scan * 4096)); return s; },
    explain: async (port: number) => { calls.explain++; return slow(structuredClone(await real.explain(port)), 400); },
    http: async (port: number) => { calls.http++; return calls.deferHttp ? new Promise<HttpInfo | null>((resolve) => calls.httpPending.push(resolve)) : real.http(port); },
    onEvent: async <T,>(name: string, callback: (payload: T) => void) => {
      calls.events.set(name, callback as (payload: unknown) => void);
      const unlisten = await real.onEvent(name, callback);
      return () => { calls.events.delete(name); unlisten(); };
    },
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
afterEach(() => { cleanup(); vi.useRealTimers(); localStorage.clear(); calls.deferHttp = false; calls.httpPending = []; calls.events.clear(); calls.scanDelayMs = 0; });

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

  it("stays Live while a slow scan is in flight, and says how old the data is only when stalled", async () => {
    vi.useFakeTimers();
    const { container } = render(App);
    await vi.advanceTimersByTimeAsync(600);
    const live = () => container.querySelector(".live")!.textContent!.trim();
    expect(live()).toBe("Live");
    calls.scanDelayMs = 9_000; // the next scan (at the 4 s tick) takes 9 s
    await vi.advanceTimersByTimeAsync(12_000);
    expect(live()).toBe("Live"); // the last snapshot is 12 s old, but a scan is running
    calls.scanDelayMs = 60_000; // now one truly stalls
    await vi.advanceTimersByTimeAsync(16_000);
    expect(live()).toMatch(/^\d+s ago$/);
    calls.scanDelayMs = 0;
  });

  it("recovers a malformed saved view and theme to usable defaults", async () => {
    vi.useFakeTimers();
    localStorage.setItem("holdmap.view", "removed-view");
    localStorage.setItem("holdmap.theme", "invalid-theme");
    const { container } = render(App);
    await vi.advanceTimersByTimeAsync(600);
    expect(container.querySelector("#port-list")).not.toBeNull();
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem("holdmap.view")).toBe("list");
    expect(localStorage.getItem("holdmap.theme")).toBe("system");
  });

  it("loads and retains cluster grouping in the list without opening Graph", async () => {
    vi.useFakeTimers();
    localStorage.setItem("holdmap.sort", "cluster");
    const { container } = render(App);
    await vi.advanceTimersByTimeAsync(600);
    const grouped = () => [...container.querySelectorAll(".gh")].map((element) => element.textContent);
    expect(grouped().join(" ")).toContain("shop");
    expect(container.querySelector(".svelte-flow")).toBeNull();
    await vi.advanceTimersByTimeAsync(3500);
    expect(grouped().join(" ")).toContain("shop");
  });

  it("keeps the newest snapshot when pushes arrive out of order in one frame", async () => {
    vi.useFakeTimers();
    const { container } = render(App);
    await vi.advanceTimersByTimeAsync(600);
    const newer = structuredClone(MOCK_SNAPSHOT), older = structuredClone(MOCK_SNAPSHOT);
    newer.taken_at_ms = Date.now() + 100;
    older.taken_at_ms = Date.now() + 50;
    newer.entries = newer.entries.filter((entry) => entry.port !== 3000);
    calls.events.get("snapshot")!(newer);
    calls.events.get("snapshot")!(older);
    await vi.advanceTimersByTimeAsync(100);
    expect(container.querySelector('[id^="row-tcp:3000"]')).toBeNull();
  });

  it("reprobes a reused PID and ignores HTTP replies from its previous process", async () => {
    vi.useFakeTimers();
    calls.deferHttp = true;
    const { container } = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(container.querySelector<HTMLElement>('[id^="row-tcp:3000"]')!);
    await vi.advanceTimersByTimeAsync(100);
    expect(calls.httpPending).toHaveLength(1);
    const replacement: Snapshot = structuredClone(MOCK_SNAPSHOT);
    replacement.taken_at_ms = Date.now();
    replacement.entries.find((entry) => entry.port === 3000)!.process!.start_token++;
    calls.events.get("snapshot")!(replacement);
    await vi.advanceTimersByTimeAsync(100);
    expect(calls.httpPending).toHaveLength(2);
    const info: HttpInfo = { port: 3000, status: 200, reason: "OK", title: "Current service", server: null, location: null, elapsed_ms: 1 };
    calls.httpPending[1](info);
    await vi.advanceTimersByTimeAsync(500);
    expect(container.querySelector("aside.pane")?.textContent).toContain("Current service");
    calls.httpPending[0]({ ...info, title: "Replaced service" });
    await vi.advanceTimersByTimeAsync(100);
    expect(container.querySelector("aside.pane")?.textContent).toContain("Current service");
    expect(container.querySelector("aside.pane")?.textContent).not.toContain("Replaced service");
  });
});
