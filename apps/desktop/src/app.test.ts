// @vitest-environment jsdom
// Regression for the blinking details pane: every scan used to throw the explanations away, so
// the selected port's Overview fell back to its skeleton until `explain` answered again.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { ActionPlan, Explanation, HttpInfo, Snapshot, StopReport } from "./lib/types";
import type { ScheduledStopRequest } from "./lib/schedules";
import { MOCK_SNAPSHOT } from "./lib/mock";
// Compile the lazy graph view during module setup, outside the action tests' time budget.
import "./components/AgentsView.svelte";

const calls = vi.hoisted(() => ({
  explain: 0, http: 0, scan: 0, scanDelayMs: 0,
  scanModes: [] as boolean[],
  scanError: null as string | null,
  scanReply: null as Snapshot | null,
  scanAllReply: null as Snapshot | null,
  explainError: null as string | null,
  deferExplain: false,
  explainPending: [] as { port: number; resolve: (value: Explanation) => void; result: Explanation }[],
  deferHttp: false,
  httpPending: [] as ((value: HttpInfo | null) => void)[],
  stopTargets: [] as string[],
  stopCalls: 0,
  stopManyCalls: [] as { agentId: string; requests: ScheduledStopRequest[] }[],
  stopManyError: null as string | null,
  agentPortsAll: false,
  planCalls: 0,
  historyFailureAt: 0,
  stopFailureAt: 0,
  restarts: 0,
  deferPlan: false,
  planPending: [] as { target: string; result: ActionPlan; resolve: (value: ActionPlan) => void }[],
  configError: null as string | null,
  events: new Map<string, (payload: unknown) => void>(),
}));
vi.mock("./lib/api", async (orig) => {
  const real = await orig<typeof import("./lib/api")>();
  const { mockAgents } = await import("./lib/mock");
  const slow = <T,>(v: T, ms: number) => new Promise<T>((r) => setTimeout(() => r(v), ms));
  const simulatedStop = async (target: string): Promise<StopReport> => {
    calls.stopTargets.push(target);
    const history_error = calls.stopTargets.length === calls.historyFailureAt ? "Restart history is read-only" : null;
    const failed = calls.stopTargets.length === calls.stopFailureAt;
    return slow({ target, success: !failed, freed: true, ports_still_busy: [], signalled: [43000], escalated: false, survivors: [], elapsed_ms: 50, log: [`port ${target} is free`], error: failed ? "A reviewed process step failed" : null, history_error }, 50);
  };
  return {
    ...real,
    // Each scan is a fresh object graph, with the memory figure changing like a real poll.
    scan: async (all: boolean) => {
      calls.scan++;
      calls.scanModes.push(all);
      if (calls.scanDelayMs) await slow(null, calls.scanDelayMs);
      if (calls.scanError) throw new Error(calls.scanError);
      const s = structuredClone((all ? calls.scanAllReply : null) ?? calls.scanReply ?? await real.scan(all));
      s.taken_at_ms = Date.now();
      s.entries.forEach((e) => e.process && (e.process.memory_bytes += calls.scan * 4096));
      return s;
    },
    explain: async (port: number) => {
      calls.explain++;
      if (calls.explainError) throw new Error(calls.explainError);
      const result = structuredClone(await real.explain(port));
      return calls.deferExplain ? new Promise<Explanation>((resolve) => calls.explainPending.push({ port, resolve, result })) : slow(result, 400);
    },
    http: async (port: number) => { calls.http++; return calls.deferHttp ? new Promise<HttpInfo | null>((resolve) => calls.httpPending.push(resolve)) : real.http(port); },
    plan: async (target: string, force: boolean, allowProtected = false) => {
      calls.planCalls++;
      const result = await real.plan(target, force, allowProtected);
      return calls.deferPlan ? new Promise<ActionPlan>((resolve) => calls.planPending.push({ target, result, resolve })) : result;
    },
    stop: async (target: string) => { calls.stopCalls++; return simulatedStop(target); },
    stopMany: async (agentId: string, requests: ScheduledStopRequest[]) => {
      calls.stopManyCalls.push({ agentId, requests });
      if (calls.stopManyError) throw new Error(calls.stopManyError);
      const reports: StopReport[] = [];
      for (const request of requests) {
        const report = await simulatedStop(request.target);
        reports.push(report);
        if (!report.success || !report.freed || report.history_error) break;
      }
      return reports;
    },
    agents: async () => {
      const report = calls.scanReply ? mockAgents(calls.scanReply) : await real.agents();
      if (calls.agentPortsAll && calls.scanReply) report.agents.find((agent) => agent.name === "Cursor")!.ports = calls.scanReply.entries.map((entry) => ({ entry_id: entry.id, port: entry.port, protocol: entry.protocol, exposure: entry.exposure, pid: entry.pid, process: entry.process?.name ?? null, label: entry.label, role: "dev_server", project: entry.project?.name ?? null, framework: entry.framework?.name ?? null }));
      return report;
    },
    restartStopped: async () => { calls.restarts++; return { command: "node server.js", pid: 1234, log: "/tmp/test.log" }; },
    appInfo: async () => ({ ...await real.appInfo(), platform: "linux" }),
    getConfig: async () => { if (calls.configError) throw new Error(calls.configError); return real.getConfig(); },
    onEvent: async <T,>(name: string, callback: (payload: T) => void) => {
      calls.events.set(name, callback as (payload: unknown) => void);
      const unlisten = await real.onEvent(name, callback);
      return () => { calls.events.delete(name); unlisten(); };
    },
  };
});
import App from "./App.svelte";

beforeAll(() => {
  window.matchMedia ??= ((q: string) => ({ matches: q === "(prefers-reduced-motion: reduce)", media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  const anim = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(anim);
  Element.prototype.scrollTo ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
  window.requestAnimationFrame ??= (f) => setTimeout(() => f(0), 16) as unknown as number;
});
// The first-run welcome is covered on its own; other tests start as a returning user.
beforeEach(() => localStorage.setItem("holdmap.onboarded", "1"));
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  localStorage.clear();
  calls.deferHttp = false;
  calls.httpPending = [];
  calls.events.clear();
  calls.scanDelayMs = 0;
  calls.scanModes = [];
  calls.scanError = null;
  calls.scanReply = null;
  calls.scanAllReply = null;
  calls.explainError = null;
  calls.deferExplain = false;
  calls.explainPending = [];
  calls.stopTargets = [];
  calls.stopCalls = 0;
  calls.stopManyCalls = [];
  calls.stopManyError = null;
  calls.agentPortsAll = false;
  calls.planCalls = 0;
  calls.historyFailureAt = 0;
  calls.stopFailureAt = 0;
  calls.restarts = 0;
  calls.deferPlan = false;
  calls.planPending = [];
  calls.configError = null;
});

describe("App partial stop outcomes", () => {
  it("preserves the freed outcome and skips automatic restart after a history write failure", async () => {
    vi.useFakeTimers();
    calls.historyFailureAt = 1;
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.container.querySelector<HTMLElement>('[id^="row-tcp:3000"]')!);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("button", { name: "Restart" }));
    await vi.advanceTimersByTimeAsync(100);
    await fireEvent.click(view.getByRole("button", { name: /Restart :3000/ }));
    await vi.advanceTimersByTimeAsync(100);
    const dialog = view.getByRole("alertdialog", { name: "Port 3000 is free" });
    expect(dialog.textContent).toContain("Restart history couldn't be saved");
    expect(dialog.textContent).toContain("Restart history is read-only");
    expect(dialog.textContent).toContain("Automatic restart was skipped");
    expect(calls.restarts).toBe(0);
    expect(calls.stopTargets).toHaveLength(1);
    expect(view.getByRole("button", { name: "Copy restart command" })).toBeTruthy();
    await vi.advanceTimersByTimeAsync(1200);
    expect(view.getByRole("alertdialog", { name: "Port 3000 is free" })).toBe(dialog);
  });

  it("stops an agent batch after a history write failure and names only the freed port", async () => {
    vi.useFakeTimers();
    calls.historyFailureAt = 1;
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("radio", { name: /^Agents/ }));
    await vi.dynamicImportSettled();
    await vi.advanceTimersByTimeAsync(600);
    const cursor = view.getByRole("article", { name: "Cursor" });
    await fireEvent.click(cursor.querySelector<HTMLButtonElement>("button.head") ?? cursor.querySelector<HTMLButtonElement>("button")!);
    await fireEvent.click(view.getByRole("button", { name: "Stop all (2)" }));
    await vi.advanceTimersByTimeAsync(100);
    await fireEvent.click(view.getByRole("button", { name: /Stop 2 ports/ }));
    await vi.advanceTimersByTimeAsync(100);
    const dialog = view.getByRole("alertdialog", { name: "Port 3000 is free" });
    expect(dialog.textContent).toContain("The remaining 1 reviewed port was not stopped");
    expect(dialog.textContent).toContain("1 of 2 ports confirmed free");
    expect(dialog.textContent).toContain("1 attempted · 1 not attempted");
    expect(calls.stopTargets).toHaveLength(1);
    expect(calls.stopManyCalls).toHaveLength(1);
    expect(calls.stopCalls).toBe(0);
    expect(view.queryByRole("heading", { name: "2 ports are free" })).toBeNull();
  });

  it("dispatches shared-process agent listeners through one reviewed batch", async () => {
    vi.useFakeTimers();
    calls.scanReply = structuredClone(MOCK_SNAPSHOT);
    const first = calls.scanReply.entries.find((entry) => entry.port === 3000)!;
    const second = calls.scanReply.entries.find((entry) => entry.port === 5173)!;
    second.pid = first.pid;
    second.pids = [...first.pids];
    second.process = structuredClone(first.process);
    second.id = `tcp:5173:${first.pid}:LISTEN`;
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("radio", { name: /^Agents/ }));
    await vi.dynamicImportSettled();
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("article", { name: "Cursor" }).querySelector<HTMLButtonElement>("button.head")!);
    await fireEvent.click(view.getByRole("button", { name: "Stop all (2)" }));
    await vi.advanceTimersByTimeAsync(100);
    await fireEvent.click(view.getByRole("button", { name: /Stop 2 ports/ }));
    await vi.advanceTimersByTimeAsync(150);
    expect(view.getByRole("alertdialog", { name: "2 ports are free" })).toBeTruthy();
    expect(calls.stopManyCalls).toHaveLength(1);
    expect(calls.stopManyCalls[0].requests.map((request) => request.target)).toEqual(["3000/tcp", "5173/tcp"]);
    expect(calls.stopCalls).toBe(0);
  });

  it("shows a rejected batch's unattempted targets and requires a new review before retrying", async () => {
    vi.useFakeTimers();
    calls.stopManyError = "The agent's ownership changed. Review a new stop plan.";
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("radio", { name: /^Agents/ }));
    await vi.dynamicImportSettled();
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("article", { name: "Cursor" }).querySelector<HTMLButtonElement>("button.head")!);
    await fireEvent.click(view.getByRole("button", { name: "Stop all (2)" }));
    await vi.advanceTimersByTimeAsync(100);
    await fireEvent.click(view.getByRole("button", { name: /Stop 2 ports/ }));
    await vi.advanceTimersByTimeAsync(100);
    const failed = view.getByRole("alertdialog");
    expect(failed.textContent).toContain("The agent's ownership changed");
    expect(failed.textContent).toContain("0 of 2 ports confirmed free");
    expect(failed.textContent).toContain("0 attempted · 2 not attempted");
    expect(calls.stopTargets).toEqual([]);
    const previousHandles = calls.stopManyCalls[0].requests.map((request) => request.confirmation_id);
    calls.stopManyError = null;
    await fireEvent.click(view.getByRole("button", { name: "Review again" }));
    await vi.advanceTimersByTimeAsync(100);
    expect(calls.stopManyCalls).toHaveLength(1);
    expect(calls.stopTargets).toEqual([]);
    await fireEvent.click(view.getByRole("button", { name: /Stop 2 ports/ }));
    await vi.advanceTimersByTimeAsync(150);
    expect(calls.stopManyCalls).toHaveLength(2);
    expect(calls.stopManyCalls[1].requests.map((request) => request.confirmation_id)).not.toEqual(previousHandles);
  });

  it("explains the batch limit before requesting any plans and leaves every listener available", async () => {
    vi.useFakeTimers();
    calls.agentPortsAll = true;
    calls.scanReply = structuredClone(MOCK_SNAPSHOT);
    const template = calls.scanReply.entries.find((entry) => entry.port === 3000)!;
    calls.scanReply.entries = Array.from({ length: 33 }, (_, index) => ({ ...structuredClone(template), id: `tcp:${20000 + index}:43000:LISTEN`, port: 20000 + index }));
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("radio", { name: /^Agents/ }));
    await vi.dynamicImportSettled();
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("article", { name: "Cursor" }).querySelector<HTMLButtonElement>("button.head")!);
    await fireEvent.click(view.getByRole("button", { name: "Stop all (33)" }));
    await vi.advanceTimersByTimeAsync(100);
    expect(view.container.textContent).toContain("Too many ports for one stop");
    expect(view.container.textContent).toContain("Stop selected services individually");
    expect(calls.planCalls).toBe(0);
    expect(calls.stopManyCalls).toHaveLength(0);
    expect(view.queryByRole("alertdialog")).toBeNull();
  });

  it.each(["restart", "agent batch"])("does not continue %s when a failed step still leaves the port free", async (action) => {
    vi.useFakeTimers();
    calls.stopFailureAt = 1;
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    if (action === "restart") {
      await fireEvent.click(view.container.querySelector<HTMLElement>('[id^="row-tcp:3000"]')!);
      await vi.advanceTimersByTimeAsync(600);
      await fireEvent.click(view.getByRole("button", { name: "Restart" }));
      await vi.advanceTimersByTimeAsync(100);
      await fireEvent.click(view.getByRole("button", { name: /Restart :3000/ }));
    } else {
      await fireEvent.click(view.getByRole("radio", { name: /^Agents/ }));
      await vi.dynamicImportSettled();
      await vi.advanceTimersByTimeAsync(600);
      await fireEvent.click(view.getByRole("article", { name: "Cursor" }).querySelector<HTMLButtonElement>("button.head")!);
      await fireEvent.click(view.getByRole("button", { name: "Stop all (2)" }));
      await vi.advanceTimersByTimeAsync(100);
      await fireEvent.click(view.getByRole("button", { name: /Stop 2 ports/ }));
    }
    await vi.advanceTimersByTimeAsync(100);
    const dialog = view.getByRole("alertdialog");
    expect(dialog.textContent).toContain("A reviewed process step failed");
    expect(dialog.textContent).toContain("The port is now free");
    expect(dialog.querySelector("[data-primary]")?.textContent?.trim()).toBe("Close");
    expect(view.queryByRole("button", { name: "Done" })).toBeNull();
    expect(calls.stopTargets).toHaveLength(1);
    expect(calls.restarts).toBe(0);
    if (action === "agent batch") {
      expect(calls.stopManyCalls).toHaveLength(1);
      expect(calls.stopCalls).toBe(0);
    }
  });
});

describe("App action review freshness", () => {
  it("allows cancelling a pending review and never replaces its newer review with an old reply", async () => {
    vi.useFakeTimers();
    calls.deferPlan = true;
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("button", { name: "Stop port 3000" }));
    await vi.advanceTimersByTimeAsync(100);
    expect(view.getByRole("dialog", { name: "Reviewing stop for :3000…" })).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
    await fireEvent.click(view.getByRole("button", { name: "Stop port 3001" }));
    await vi.advanceTimersByTimeAsync(100);
    expect(calls.planPending).toHaveLength(2);
    const [older, current] = calls.planPending;
    current.resolve(current.result);
    await vi.advanceTimersByTimeAsync(100);
    const reviewed = view.getByRole("alertdialog");
    expect(reviewed.textContent).toContain(":3001");
    older.resolve(older.result);
    await vi.advanceTimersByTimeAsync(100);
    expect(view.getByRole("alertdialog")).toBe(reviewed);
    expect(view.queryByRole("button", { name: /Stop :3000/ })).toBeNull();
    expect(calls.stopTargets).toHaveLength(0);
  });

  it("ignores a plan completed after its review was cancelled with Escape", async () => {
    vi.useFakeTimers();
    calls.deferPlan = true;
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("button", { name: "Stop port 3000" }));
    await vi.advanceTimersByTimeAsync(100);
    await fireEvent.keyDown(view.getByRole("dialog"), { key: "Escape" });
    calls.planPending[0].resolve(calls.planPending[0].result);
    await vi.advanceTimersByTimeAsync(300);
    expect(view.queryByRole("dialog")).toBeNull();
    expect(view.queryByRole("alertdialog")).toBeNull();
    expect(calls.stopTargets).toHaveLength(0);
  });

  it("asks for a fresh review when the listener is replaced while its plan is loading", async () => {
    vi.useFakeTimers();
    calls.deferPlan = true;
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("button", { name: "Stop port 3000" }));
    await vi.advanceTimersByTimeAsync(100);
    const replacement = structuredClone(MOCK_SNAPSHOT);
    replacement.taken_at_ms = Date.now();
    replacement.entries.find((entry) => entry.port === 3000)!.process!.start_token++;
    calls.events.get("snapshot")!(replacement);
    await vi.advanceTimersByTimeAsync(100);
    calls.planPending[0].resolve(calls.planPending[0].result);
    await vi.advanceTimersByTimeAsync(100);
    expect(view.queryByRole("alertdialog")).toBeNull();
    expect(view.container.textContent).toContain("The port's owner changed");
    expect(calls.stopTargets).toHaveLength(0);
  });
});

describe("App live polling", () => {
  it("keeps scanning and offers settings recovery after a bootstrap config failure", async () => {
    vi.useFakeTimers();
    calls.configError = "Saved settings cannot be read";
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    expect(view.container.querySelector('[id^="row-tcp:3000"]')).not.toBeNull();
    expect(view.container.textContent).toContain("Couldn't load settings");
    await fireEvent.click(view.getByRole("button", { name: "Settings" }));
    expect(view.getByRole("dialog", { name: "Settings unavailable" }).textContent).toContain("Saved settings cannot be read");
    calls.configError = null;
    await fireEvent.click(view.getByRole("button", { name: "Try again" }));
    await vi.advanceTimersByTimeAsync(300);
    expect(view.getByRole("dialog", { name: "Settings" })).toBeTruthy();
    expect(view.queryByRole("dialog", { name: "Settings unavailable" })).toBeNull();
    await fireEvent.click(view.getByRole("tab", { name: "About" }));
    expect(view.container.textContent).toContain("Desktop app on linux");
  });

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

  it("retains rows and details after a failed scan, and offers a visible retry that cannot be duplicated", async () => {
    vi.useFakeTimers();
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    const row = view.container.querySelector<HTMLElement>('[id^="row-tcp:3000"]')!;
    await fireEvent.click(row);
    await vi.advanceTimersByTimeAsync(600);
    const headline = view.container.querySelector("aside.pane .headline");
    calls.scanError = "Socket scan permission denied";
    await fireEvent.click(view.getByRole("button", { name: "Refresh" }));
    await vi.advanceTimersByTimeAsync(50);
    const banner = view.getByRole("region", { name: "Scan status" });
    expect(banner.textContent).toContain("Socket scan permission denied");
    expect(banner.textContent).toContain("Showing the last successful scan");
    expect(view.container.querySelector(".live")?.textContent).toContain("Scan failed");
    expect(view.container.querySelector('[id^="row-tcp:3000"]')).toBe(row);
    expect(view.container.querySelector("aside.pane .headline")).toBe(headline);

    calls.scanError = null;
    calls.scanDelayMs = 1000;
    await fireEvent.click(view.getByRole("button", { name: "Try again" }));
    const scanCalls = calls.scan;
    const retry = view.getByRole("button", { name: "Retrying…" }) as HTMLButtonElement;
    expect(retry.disabled).toBe(true);
    expect(view.container.querySelector(".live")?.textContent).toContain("Retrying…");
    await fireEvent.click(retry);
    expect(calls.scan).toBe(scanCalls);
    await vi.advanceTimersByTimeAsync(1400);
    expect(view.queryByRole("region", { name: "Scan status" })).toBeNull();
    expect(view.container.querySelector(".live")?.textContent?.trim()).toBe("Live");
  });

  it("uses the completed manual scan for its success feedback", async () => {
    vi.useFakeTimers();
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    calls.scanReply = { ...structuredClone(MOCK_SNAPSHOT), entries: [structuredClone(MOCK_SNAPSHOT.entries[0])], scan_ms: 17 };
    await fireEvent.click(view.getByRole("button", { name: "Refresh" }));
    await vi.advanceTimersByTimeAsync(100);
    expect(view.container.querySelector(".toast.info")?.textContent).toContain("1 port · scanned in 17 ms");
  });

  it("revalidates numeric-port availability on a new scan and supports a failed check's retry", async () => {
    vi.useFakeTimers();
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.input(view.getByRole("searchbox", { name: "Search ports" }), { target: { value: "65000" } });
    await vi.advanceTimersByTimeAsync(150);
    expect(view.getByRole("heading", { name: "Checking port 65000…" })).toBeTruthy();
    await vi.advanceTimersByTimeAsync(500);
    const headline = view.getByRole("heading", { name: "Port 65000 is free" });
    calls.events.get("snapshot")!({ ...structuredClone(MOCK_SNAPSHOT), taken_at_ms: Date.now() });
    await vi.advanceTimersByTimeAsync(50);
    expect(view.getByRole("heading", { name: "Port 65000 is free" })).toBe(headline);
    expect(view.container.textContent).toContain("Checking availability…");
    await vi.advanceTimersByTimeAsync(600);
    expect(view.getByRole("heading", { name: "Port 65000 is free" })).toBe(headline);

    calls.explainError = "Port availability check failed";
    calls.events.get("snapshot")!({ ...structuredClone(MOCK_SNAPSHOT), taken_at_ms: Date.now() });
    await vi.advanceTimersByTimeAsync(200);
    expect(view.queryByRole("heading", { name: "Port 65000 is free" })).toBeNull();
    expect(view.getByRole("heading", { name: "Couldn't check port 65000" })).toBeTruthy();
    expect(view.container.textContent).toContain("Port availability check failed");
    calls.explainError = null;
    await fireEvent.click(view.getByRole("button", { name: "Check again" }));
    await vi.advanceTimersByTimeAsync(650);
    expect(view.getByRole("heading", { name: "Port 65000 is free" })).toBeTruthy();

    calls.scanError = "Socket scan failed";
    await fireEvent.click(view.getByRole("button", { name: "Refresh" }));
    await vi.advanceTimersByTimeAsync(100);
    expect(view.queryByRole("heading", { name: "Port 65000 is free" })).toBeNull();
    expect(view.getByRole("heading", { name: "Port status unavailable" })).toBeTruthy();
  });

  it("ignores an availability reply from an older scan", async () => {
    vi.useFakeTimers();
    calls.deferExplain = true;
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.input(view.getByRole("searchbox", { name: "Search ports" }), { target: { value: "65000" } });
    await vi.advanceTimersByTimeAsync(200);
    expect(calls.explainPending).toHaveLength(1);
    calls.events.get("snapshot")!({ ...structuredClone(MOCK_SNAPSHOT), taken_at_ms: Date.now() });
    await vi.advanceTimersByTimeAsync(200);
    expect(calls.explainPending).toHaveLength(1);
    const old = calls.explainPending[0];
    old.resolve(old.result);
    await vi.advanceTimersByTimeAsync(50);
    expect(calls.explainPending).toHaveLength(2);
    expect(view.queryByRole("heading", { name: "Port 65000 is free" })).toBeNull();
    const current = calls.explainPending[1];
    current.resolve({ ...current.result, status: "busy", headline: "A new service grabbed this port" });
    await vi.advanceTimersByTimeAsync(50);
    expect(view.container.textContent).toContain("A new service grabbed this port");
    expect(view.queryByRole("heading", { name: "Port 65000 is free" })).toBeNull();
    expect(view.container.textContent).toContain("A new service grabbed this port");
  });

  it("bounds slow availability checks to one request and one latest follow-up", async () => {
    vi.useFakeTimers();
    calls.deferExplain = true;
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    const search = view.getByRole("searchbox", { name: "Search ports" });
    await fireEvent.input(search, { target: { value: "65000" } });
    await vi.advanceTimersByTimeAsync(200);
    for (let index = 0; index < 8; index++) {
      calls.events.get("snapshot")!({ ...structuredClone(MOCK_SNAPSHOT), taken_at_ms: Date.now() });
      await vi.advanceTimersByTimeAsync(200);
    }
    await fireEvent.input(search, { target: { value: "65001" } });
    await vi.advanceTimersByTimeAsync(200);
    await fireEvent.input(search, { target: { value: "65002" } });
    await vi.advanceTimersByTimeAsync(200);
    expect(calls.explainPending.map((request) => request.port)).toEqual([65000]);
    calls.explainPending[0].resolve(calls.explainPending[0].result);
    await vi.advanceTimersByTimeAsync(100);
    expect(calls.explainPending.map((request) => request.port)).toEqual([65000, 65002]);
    calls.explainPending[1].resolve(calls.explainPending[1].result);
    await vi.advanceTimersByTimeAsync(100);
    expect(view.getByRole("heading", { name: "Port 65002 is free" })).toBeTruthy();
  });

  it("rescans the latest socket mode when it changes during an in-flight scan", async () => {
    vi.useFakeTimers();
    calls.scanDelayMs = 1000;
    calls.scanReply = { ...structuredClone(MOCK_SNAPSHOT), entries: [structuredClone(MOCK_SNAPSHOT.entries[0])] };
    calls.scanAllReply = structuredClone(MOCK_SNAPSHOT);
    const view = render(App);
    await vi.advanceTimersByTimeAsync(100);
    await fireEvent.click(view.getByRole("radio", { name: "All sockets" }));
    expect(calls.scanModes).toEqual([false]);
    await vi.advanceTimersByTimeAsync(1100);
    expect(calls.scanModes).toEqual([false, true]);
    expect(view.container.querySelector("[role=option]")).toBeNull();
    await vi.advanceTimersByTimeAsync(1100);
    expect(view.getByRole("radio", { name: "All sockets" }).getAttribute("aria-checked")).toBe("true");
    expect(view.getAllByRole("option")).toHaveLength(MOCK_SNAPSHOT.entries.length);
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

describe("App overview and first run", () => {
  it("answers which agent is running in two clicks from the overview", async () => {
    vi.useFakeTimers();
    const { mockAgents } = await import("./lib/mock");
    const agent = mockAgents().agents[0];
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("radio", { name: /^Home/ }));
    await vi.advanceTimersByTimeAsync(300);
    const overview = view.getByRole("region", { name: "Overview" });
    const row = [...overview.querySelectorAll<HTMLButtonElement>(".map-list button")].find((b) => b.textContent?.includes(agent.name))!;
    await fireEvent.click(row);
    await vi.advanceTimersByTimeAsync(300);
    expect(view.getByRole("radio", { name: /^Agents/ }).getAttribute("aria-checked")).toBe("true");
    expect(localStorage.getItem("holdmap.view")).toBe("agents");
  });

  it("offers the welcome once, and skipping changes nothing else", async () => {
    vi.useFakeTimers();
    localStorage.removeItem("holdmap.onboarded");
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    const dialog = view.getByRole("dialog", { name: "Welcome to holdmap" });
    expect(dialog).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Skip" }));
    await vi.advanceTimersByTimeAsync(300);
    expect(view.queryByRole("dialog", { name: "Welcome to holdmap" })).toBeNull();
    expect(localStorage.getItem("holdmap.onboarded")).toBe("1");
    expect(view.container.querySelector("#port-list")).not.toBeNull();
  });

  it("starts on the chosen view after the welcome", async () => {
    vi.useFakeTimers();
    localStorage.removeItem("holdmap.onboarded");
    const view = render(App);
    await vi.advanceTimersByTimeAsync(600);
    await fireEvent.click(view.getByRole("button", { name: "Start" }));
    await vi.advanceTimersByTimeAsync(300);
    expect(view.getByRole("region", { name: "Overview" })).toBeTruthy();
  });
});
