// @vitest-environment jsdom
import { cleanup, fireEvent, render, within } from "@testing-library/svelte";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import HomeView from "./HomeView.svelte";
import { MOCK_SNAPSHOT, mockAgents } from "../../lib/mock";

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  window.matchMedia ??= ((media: string) => ({ matches: true, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  const animate = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(animate);
  Element.prototype.scrollIntoView ??= vi.fn();
});
afterEach(cleanup);

const props = (over: Record<string, unknown> = {}) => ({ entries: MOCK_SNAPSHOT.entries, report: mockAgents(undefined, true), layout: "map" as "map" | "cards", onopenagent: vi.fn(), onopenentry: vi.fn(), onview: vi.fn(), ...over });
const card = (view: View, name: string) => within(view.container).getByRole("button", { name: new RegExp(`^${name}, `) }).closest("article")!;
type View = { container: HTMLElement };
const mapAgent = (view: View, name: string) => view.container.querySelector<SVGGElement>(`.map g.agent[aria-label^="${name},"]`)!;
const lane = (view: View, name: string) => within(view.container).getAllByRole("button", { name: new RegExp(`^${name}, up `) })[0];

describe("HomeView cards", () => {
  it("shows one card per agent with fixed counters, chips and an inline attention badge", () => {
    const p = props({ layout: "cards" });
    const view = render(HomeView, p);
    expect(view.container.querySelectorAll("section[aria-label='Agents'] article.card")).toHaveLength(p.report.agents.length);
    const cursor = card(view, "Cursor");
    expect(within(cursor).getByText("Projects")).toBeTruthy();
    expect(within(cursor).getByRole("button", { name: /^MCP filesystem, running, unpinned$/ })).toBeTruthy();
    expect(within(cursor).getByRole("button", { name: /^Cursor, Standard, 1 need you$/ })).toBeTruthy();
    expect(view.container.querySelector(".map")).toBeNull();
    expect(view.queryByRole("table", { name: "Agent lanes" })).toBeNull();
  });

  it("marks unreadable configuration unknown with its reason, never zero", () => {
    const view = render(HomeView, props({ layout: "cards" }));
    const docker = card(view, "Docker Desktop");
    expect(within(docker).getAllByText("unknown")).toHaveLength(2);
    expect(docker.querySelector("[title*='not supported']")).not.toBeNull();
    expect(docker.textContent).not.toMatch(/Not supported|No project folder/);
  });

  it("keeps the selection when switching between cards and map", async () => {
    const p = props({ layout: "cards" });
    const view = render(HomeView, p);
    await fireEvent.click(within(view.container).getByRole("button", { name: /^Claude Code, / }));
    expect(card(view, "Claude Code").classList.contains("selected")).toBe(true);
    expect(card(view, "Cursor").classList.contains("dim")).toBe(true);
    await view.rerender({ ...p, layout: "map" });
    expect(mapAgent(view, "Claude Code").getAttribute("aria-pressed")).toBe("true");
    expect(lane(view, "Claude Code").closest(".lane")!.classList.contains("active")).toBe(true);
  });

  it("opens agents and ports from cards and sends stop through the plan", async () => {
    const onstopagent = vi.fn();
    const p = props({ layout: "cards", onstopagent });
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
    const p = props({ layout: "cards", onstopentry });
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
    const view = render(HomeView, props({ layout: "cards", onstopentry, entries }));
    const loose = view.getByRole("article", { name: "Unowned dev servers" });
    await fireEvent.click(within(loose).getByRole("button", { name: /^Port 8125 / }));
    expect((within(view.getByRole("menu")).getByRole("menuitem", { name: /^Stop…/ }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.keyDown(view.getByRole("menu"), { key: "Escape" });
    expect(view.queryByRole("menu")).toBeNull();
    await fireEvent.click(within(loose).getByRole("button", { name: /^Port 8000 / }));
    await fireEvent.click(within(view.getByRole("menu")).getByRole("menuitem", { name: "Stop…" }));
    expect(onstopentry).toHaveBeenCalledWith(entries.find((e) => e.port === 8000)!.id);
  });

  it("offers the layout toggle as a radio group", async () => {
    const onlayout = vi.fn();
    const view = render(HomeView, props({ layout: "cards", onlayout }));
    const group = view.getByRole("radiogroup", { name: "Home layout" });
    await fireEvent.click(within(group).getByRole("radio", { name: /^Map/ }));
    expect(onlayout).toHaveBeenCalledWith("map");
  });
});

describe("HomeView map", () => {
  it("selects an agent in the map and lights it in the lanes; a second click opens it", async () => {
    const p = props();
    const view = render(HomeView, p);
    const claude = p.report.agents.find((a) => a.product === "claude-code")!;
    await fireEvent.click(mapAgent(view, "Claude Code"));
    expect(mapAgent(view, "Claude Code").getAttribute("aria-pressed")).toBe("true");
    expect(lane(view, "Claude Code").closest(".lane")!.classList.contains("active")).toBe(true);
    expect(lane(view, "Cursor").closest(".lane")!.classList.contains("dim")).toBe(true);
    await fireEvent.click(mapAgent(view, "Claude Code"));
    expect(p.onopenagent).toHaveBeenCalledWith(claude.id);
  });

  it("hovering a lane highlights the same agent in the map and its attention items", async () => {
    const view = render(HomeView, props());
    await fireEvent.mouseEnter(lane(view, "Docker Desktop").closest(".lane")!);
    expect(mapAgent(view, "Cursor").closest(".cluster")!.classList.contains("dim")).toBe(true);
    expect(mapAgent(view, "Docker Desktop").closest(".cluster")!.classList.contains("dim")).toBe(false);
    const items = [...view.container.querySelectorAll(".needs li")];
    expect(items.some((li) => !li.classList.contains("dim") && li.textContent!.includes("Docker Desktop"))).toBe(true);
    expect(items.some((li) => li.classList.contains("dim") && li.textContent!.includes("Cursor"))).toBe(true);
  });

  it("gives map port nodes and timeline pins Details and Stop…, both through the plan", async () => {
    const onstopentry = vi.fn();
    const p = props({ onstopentry });
    const view = render(HomeView, p);
    const cursor = p.report.agents.find((a) => a.product === "cursor")!;
    const port = cursor.ports[0];
    await fireEvent.click(view.container.querySelector(`.map g.sat[aria-label^="Port :${port.port}"]`)!);
    await fireEvent.click(within(view.getByRole("menu")).getByRole("menuitem", { name: "Details" }));
    expect(p.onopenentry).toHaveBeenCalledWith(port.entry_id);
    await fireEvent.keyDown(view.container.querySelector(`.map g.sat[aria-label^="Port :${port.port}"]`)!, { key: "Enter" });
    await fireEvent.click(within(view.getByRole("menu")).getByRole("menuitem", { name: "Stop…" }));
    expect(onstopentry).toHaveBeenCalledWith(port.entry_id);
    const pin = within(view.getByRole("table", { name: "Agent lanes" })).getByRole("button", { name: new RegExp(`^Port ${port.port} `) });
    await fireEvent.click(pin);
    await fireEvent.click(within(view.getByRole("menu")).getByRole("menuitem", { name: "Stop…" }));
    expect(onstopentry).toHaveBeenCalledTimes(2);
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
    await fireEvent.click(off.getByRole("button", { name: "Read MCP config" }));
    expect(oninventory).toHaveBeenCalled();
    cleanup();
    expect(render(HomeView, props({ oninventory })).queryByRole("button", { name: "Read MCP config" })).toBeNull();
  });

  it("moves between lanes with j/k and between map agents with the arrow keys; Esc clears the selection", async () => {
    const view = render(HomeView, props());
    const first = lane(view, "Docker Desktop");
    first.focus();
    await fireEvent.keyDown(first, { key: "j" });
    expect(document.activeElement).not.toBe(first);
    const a = mapAgent(view, "Claude Code");
    a.focus();
    await fireEvent.keyDown(a, { key: "ArrowRight" });
    expect(document.activeElement).not.toBe(a);
    await fireEvent.keyDown(document.activeElement!, { key: "Enter" });
    expect(view.container.querySelector('.map g.agent[aria-pressed="true"]')).not.toBeNull();
    await fireEvent.keyDown(view.getByRole("region", { name: "Overview" }), { key: "Escape" });
    expect(view.container.querySelector('.map g.agent[aria-pressed="true"]')).toBeNull();
  });

  it("collapses the timeline band through its toggle", async () => {
    const ontimeline = vi.fn();
    const p = props({ ontimeline });
    const view = render(HomeView, p);
    const toggle = view.getByRole("button", { name: /^Timeline/ });
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    await fireEvent.click(toggle);
    expect(ontimeline).toHaveBeenCalledWith(false);
    await view.rerender({ ...p, timeline: false });
    expect(view.queryByRole("table", { name: "Agent lanes" })).toBeNull();
    expect(view.getByRole("button", { name: /^Timeline/ }).getAttribute("aria-expanded")).toBe("false");
  });
});
