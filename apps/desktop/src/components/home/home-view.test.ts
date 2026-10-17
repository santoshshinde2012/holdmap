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

const props = (over: Record<string, unknown> = {}) => ({ entries: MOCK_SNAPSHOT.entries, report: mockAgents(undefined, true), onopenagent: vi.fn(), onopenentry: vi.fn(), onview: vi.fn(), ...over });
type View = { container: HTMLElement };
const mapAgent = (view: View, name: string) => view.container.querySelector<SVGGElement>(`.map g.agent[aria-label^="${name},"]`)!;
const lane = (view: View, name: string) => within(view.container).getAllByRole("button", { name: new RegExp(`^${name}, up `) })[0];

describe("HomeView", () => {
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

  it("opens ports from the map and the lanes", async () => {
    const p = props();
    const view = render(HomeView, p);
    const cursor = p.report.agents.find((a) => a.product === "cursor")!;
    const port = cursor.ports[0];
    await fireEvent.click(view.container.querySelector(`.map g.sat[aria-label^="Port :${port.port}"]`)!);
    expect(p.onopenentry).toHaveBeenCalledWith(port.entry_id);
    await fireEvent.click(view.getAllByRole("button", { name: new RegExp(`^Port ${port.port} `) })[0]);
    expect(p.onopenentry).toHaveBeenCalledTimes(2);
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
});
