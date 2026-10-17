// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import HomeView from "./HomeView.svelte";
import { MOCK_SNAPSHOT, mockAgents } from "../../lib/mock";
import type { ChangeEvent } from "../../lib/changes";

beforeAll(() => {
  window.matchMedia ??= ((media: string) => ({ matches: true, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  const animate = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(animate);
  Element.prototype.scrollIntoView ??= vi.fn();
});
afterEach(cleanup);

const props = (over: Record<string, unknown> = {}) => ({ entries: MOCK_SNAPSHOT.entries, report: mockAgents(undefined, true), onopenagent: vi.fn(), onopenentry: vi.fn(), onview: vi.fn(), ...over });

describe("HomeView", () => {
  it("opens an agent, its configuration and its ports in one click", async () => {
    const p = props();
    const view = render(HomeView, p);
    const agent = p.report.agents[0];
    await fireEvent.click(view.getByRole("button", { name: new RegExp(`^${agent.name},`) }));
    expect(p.onopenagent).toHaveBeenCalledWith(agent.id, "overview");
    await fireEvent.click(view.getAllByRole("button", { name: /^MCP servers:/ })[0]);
    expect(p.onopenagent).toHaveBeenLastCalledWith(expect.any(String), "capabilities");
    const port = agent.ports[0];
    await fireEvent.click(view.getAllByRole("button", { name: new RegExp(`^Port ${port.port}`) })[0]);
    expect(p.onopenentry).toHaveBeenCalledWith(port.entry_id);
  });

  it("says scanning instead of showing zeros or dashes before the first scan", () => {
    const view = render(HomeView, props({ entries: null, report: null }));
    expect(view.getAllByText("scanning…").length).toBeGreaterThan(0);
    expect(view.queryByText("—")).toBeNull();
    expect(view.queryByRole("button", { name: /listening/ })).toBeNull();
    expect(view.getByText("Waiting for the first scan…")).toBeTruthy();
  });

  it("makes every summary number open its view and counts running MCP servers", async () => {
    const p = props();
    const view = render(HomeView, p);
    await fireEvent.click(view.getByRole("button", { name: /listening$/ }));
    expect(p.onview).toHaveBeenCalledWith("list");
    expect(view.getByRole("button", { name: /MCP running$/ }).textContent).toMatch(/^1\/\d+ MCP running$/);
  });

  it("offers the inventory opt-in only while configuration isn't read", async () => {
    const oninventory = vi.fn();
    const off = render(HomeView, props({ report: mockAgents(), oninventory }));
    await fireEvent.click(off.getByRole("button", { name: "Turn on" }));
    expect(oninventory).toHaveBeenCalled();
    cleanup();
    const on = render(HomeView, props({ report: mockAgents(), oninventory, inventory: true }));
    expect(on.queryByRole("button", { name: "Turn on" })).toBeNull();
  });

  it("lists recent changes with relative times and moves between rows with j/k", async () => {
    const changes: ChangeEvent[] = [{ id: "c1", at: 1_000, kind: "port_opened", title: ":5173 opened", detail: "web · PID 1", target: { view: "list", entryId: "x" } }];
    const view = render(HomeView, props({ changes, now: 1_000 + 4 * 60_000 }));
    expect(view.getByText(":5173 opened")).toBeTruthy();
    expect(view.getByText("4m ago")).toBeTruthy();
    const region = view.getByRole("region", { name: "Overview" });
    await fireEvent.keyDown(region, { key: "j" });
    const first = document.activeElement as HTMLElement;
    expect(first.hasAttribute("data-home-nav")).toBe(true);
    await fireEvent.keyDown(first, { key: "j" });
    expect(document.activeElement).not.toBe(first);
    await fireEvent.keyDown(document.activeElement!, { key: "k" });
    expect(document.activeElement).toBe(first);
  });
});
