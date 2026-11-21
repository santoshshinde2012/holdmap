// @vitest-environment jsdom
import { cleanup, fireEvent, render, within } from "@testing-library/svelte";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import HomeView from "./HomeView.svelte";
import { MOCK_SNAPSHOT } from "../../lib/mock";
import { mockAgents } from "../../lib/mock-agents";

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  window.matchMedia ??= ((media: string) => ({ matches: true, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  const animate = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(animate);
  Element.prototype.scrollIntoView ??= vi.fn();
});
afterEach(cleanup);

const props = (over: Record<string, unknown> = {}) => ({ entries: MOCK_SNAPSHOT.entries, report: mockAgents(undefined, true), onopenagent: vi.fn(), onopenentry: vi.fn(), onview: vi.fn(), ...over });
const card = (view: View, name: string) => within(view.container).getByRole("button", { name: new RegExp(`^${name}, `) }).closest("article")!;
type View = { container: HTMLElement };

describe("HomeView on a quiet machine", () => {
  it("shows one calm message instead of an empty canvas", () => {
    const view = render(HomeView, props({ entries: [], report: { ...mockAgents(undefined, true), agents: [], configured: [] } }));
    expect(within(view.container).getByText("No agents or dev servers running")).toBeTruthy();
    expect(view.container.querySelector(".map")).toBeNull();
    expect(view.queryByRole("table", { name: "Agent lanes" })).toBeNull();
  });
});

describe("HomeView cards", () => {
  it("shows one card per agent with fixed counters, chips and an inline attention badge", () => {
    const p = props();
    const view = render(HomeView, p);
    expect(view.container.querySelectorAll("section[aria-label='Agents'] article.card")).toHaveLength(p.report.agents.length);
    const cursor = card(view, "Cursor");
    expect(within(cursor).getByText("Projects")).toBeTruthy();
    expect(within(cursor).getByRole("button", { name: /^MCP filesystem, running, unpinned$/ })).toBeTruthy();
    expect(within(cursor).getByRole("button", { name: /^Cursor, Standard, 1 need you$/ })).toBeTruthy();
    expect(view.container.querySelector(".map")).toBeNull();
    expect(view.queryByRole("table", { name: "Agent lanes" })).toBeNull();
  });

  it("answers who uses the most RAM on the card itself, and tells same-named agents apart", () => {
    const report = mockAgents(undefined, true);
    const cursor = report.agents.find((a) => a.name === "Cursor")!;
    const twin = { ...cursor, id: "agent:9", pid: 9, folders: [], parent: null, started_at: 0, cpu_unknown: "The system refused it." };
    const view = render(HomeView, props({ report: { ...report, agents: [...report.agents, twin] } }));
    const first = card(view, `Cursor \\(${cursor.folders[0].label}\\)`);
    expect(within(first).getByText("RAM")).toBeTruthy();
    expect(within(first).getByText(/\d (KB|MB|GB)$/)).toBeTruthy();
    expect(within(first).getByText(/^\d+(\.\d)?%$/)).toBeTruthy();
    // The twin has no folder or parent: its PID tells it apart, its CPU says unknown, and an
    // unreadable start time leaves the uptime out instead of "up 20000d".
    const second = card(view, "Cursor \\(PID 9\\)");
    expect(within(second).getByText("PID 9")).toBeTruthy();
    expect(within(second).getByText("unknown")).toBeTruthy();
    expect(second.textContent).not.toMatch(/up \d+d/);
  });

  it("filters cards from the home search without changing the summary", () => {
    const view = render(HomeView, props({ query: "claude code" }));
    expect(view.getAllByRole("article", { name: "Claude Code" }).length).toBeGreaterThan(0);
    expect(view.queryByRole("article", { name: "Docker Desktop" })).toBeNull();
    expect(view.getByRole("button", { name: /^Agents / })).toBeTruthy();
  });

  it("leaves MCP off the cards until config is turned on", () => {
    const report = mockAgents(undefined, true);
    const off = { ...report, agents: report.agents.map((a) => ({ ...a, capabilities: undefined })) };
    const view = render(HomeView, props({ report: off, oninventory: vi.fn() }));
    expect(view.container.textContent).not.toContain("Not read");
    expect(view.getByRole("button", { name: "Read local config" })).toBeTruthy();
  });

  it("marks unreadable configuration unknown with its reason, never zero", () => {
    const view = render(HomeView, props());
    const docker = card(view, "Docker Desktop");
    // One "MCP & skills: Not read" tile, and no per-card button: Home's header offers the single Read local config.
    expect(within(docker).getAllByText("Not read")).toHaveLength(1);
    expect(within(docker).queryByRole("button", { name: "Read config" })).toBeNull();
    expect(docker.querySelector("[title*='not supported']")).not.toBeNull();
    expect(docker.textContent).not.toMatch(/Not supported|No project folder/);
  });

  it("opens agents and ports from cards and sends stop through the plan", async () => {
    const onstopagent = vi.fn();
    const p = props({ onstopagent });
    const view = render(HomeView, p);
    const docker = p.report.agents.find((a) => a.product === "docker-desktop")!;
    await fireEvent.click(view.getByRole("button", { name: "Open Docker Desktop" }));
    expect(p.onopenagent).toHaveBeenCalledWith(docker.id);
    await fireEvent.click(view.getByRole("button", { name: "Review stop plan: Docker Desktop" }));
    expect(onstopagent).toHaveBeenCalledWith(docker.id);
    await fireEvent.click(within(card(view, "Docker Desktop")).getAllByRole("button", { name: /^Port / })[0]);
    await fireEvent.click(within(view.getByRole("menu")).getByRole("menuitem", { name: "Details" }));
    expect(p.onopenentry).toHaveBeenCalled();
  });

  it("stops a port from a card chip through the stop plan, never directly", async () => {
    const onstopentry = vi.fn();
    const p = props({ onstopentry });
    const view = render(HomeView, p);
    const cursor = p.report.agents.find((a) => a.product === "cursor")!;
    const port = cursor.ports[0];
    const chip = within(card(view, "Cursor")).getByRole("button", { name: new RegExp(`^Port ${port.port} `) });
    expect(chip.getAttribute("aria-haspopup")).toBe("menu");
    await fireEvent.click(chip);
    const menu = view.getByRole("menu", { name: new RegExp(`^:${port.port} `) });
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Stop…" }));
    expect(onstopentry).toHaveBeenCalledWith(port.entry_id);
    expect(view.queryByRole("menu")).toBeNull();
  });

  it("stops an unowned dev server from its card and offers no stop without an owner", async () => {
    const onstopentry = vi.fn();
    const entries = MOCK_SNAPSHOT.entries.map((e) => (e.port === 8125 ? { ...e, process: null, container: null, pid: null } : e));
    const view = render(HomeView, props({ onstopentry, entries }));
    const loose = view.getByRole("article", { name: "Unowned dev servers" });
    await fireEvent.click(within(loose).getByRole("button", { name: /^Port 8125 / }));
    expect((within(view.getByRole("menu")).getByRole("menuitem", { name: /^Stop…/ }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.keyDown(view.getByRole("menu"), { key: "Escape" });
    expect(view.queryByRole("menu")).toBeNull();
    await fireEvent.click(within(loose).getByRole("button", { name: /^Port 8000 / }));
    await fireEvent.click(within(view.getByRole("menu")).getByRole("menuitem", { name: "Stop…" }));
    expect(onstopentry).toHaveBeenCalledWith(entries.find((e) => e.port === 8000)!.id);
  });

  it("keeps Needs you compact: detail only on expand, and stop always goes through the plan", async () => {
    const onstopagent = vi.fn();
    const onstopentry = vi.fn();
    const view = render(HomeView, props({ onstopagent, onstopentry }));
    const title = view.getByRole("button", { name: "Docker Desktop is listening on all interfaces" });
    expect(view.queryByText(/can be reached from other machines/)).toBeNull();
    await fireEvent.click(title);
    expect(title.getAttribute("aria-expanded")).toBe("true");
    expect(view.getByText(/can be reached from other machines/)).toBeTruthy();
    await fireEvent.click(title);
    expect(title.getAttribute("aria-expanded")).toBe("false");
    const stops = view.getAllByRole("button", { name: /^Review stop plan:/ });
    for (const b of stops) await fireEvent.click(b);
    expect(onstopagent.mock.calls.length + onstopentry.mock.calls.length).toBe(stops.length);
  });

  it("shows no description sentences, zeros or dashes before the first scan", () => {
    const view = render(HomeView, props({ entries: null, report: null }));
    expect(view.container.textContent).not.toMatch(/—|Hover|focus an agent|Waiting/);
    expect(view.queryByRole("button", { name: /listening$/ })).toBeNull();
  });

  it("offers a terse inventory button only while configuration isn't read", async () => {
    const oninventory = vi.fn();
    const off = render(HomeView, props({ report: mockAgents(), oninventory }));
    await fireEvent.click(off.getByRole("button", { name: "Read local config" }));
    expect(oninventory).toHaveBeenCalled();
    cleanup();
    expect(render(HomeView, props({ oninventory })).queryByRole("button", { name: "Read local config" })).toBeNull();
  });
});

describe("HomeView when the agents lookup fails", () => {
  it("explains it on the cards with a retry, not an endless …", async () => {
    const retry = vi.fn();
    const view = render(HomeView, props({ report: null, agentsError: "holdmap isn't installed", onretryagents: retry }));
    expect(view.container.textContent).toContain("Couldn't look up the agents");
    await fireEvent.click(within(view.container).getByRole("button", { name: "Try again" }));
    expect(retry).toHaveBeenCalledOnce();
  });
});
