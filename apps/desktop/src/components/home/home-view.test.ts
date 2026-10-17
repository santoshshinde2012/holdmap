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
  it("opens an agent and its ports in one click, and expands its MCP servers inline", async () => {
    const p = props();
    const view = render(HomeView, p);
    const agent = p.report.agents.find((a) => a.product === "cursor")!;
    await fireEvent.click(view.getByRole("button", { name: /^Cursor,/ }));
    expect(p.onopenagent).toHaveBeenCalledWith(agent.id, "overview");
    const port = agent.ports[0];
    await fireEvent.click(view.getAllByRole("button", { name: new RegExp(`^Port ${port.port}`) })[0]);
    expect(p.onopenentry).toHaveBeenCalledWith(port.entry_id);
    expect(view.queryByText("legacy-browser")).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: /^MCP servers: 1 running of 3/ }));
    expect(view.getByText("legacy-browser")).toBeTruthy();
    expect(view.getByRole("button", { name: "Hide Cursor MCP servers" }).getAttribute("aria-expanded")).toBe("true");
  });

  it("expands and collapses the focused agent with the arrow keys", async () => {
    const view = render(HomeView, props());
    const name = view.getByRole("button", { name: /^Claude Code,/ });
    await fireEvent.keyDown(name, { key: "ArrowRight" });
    expect(view.getByRole("button", { name: "Hide Claude Code MCP servers" })).toBeTruthy();
    await fireEvent.keyDown(name, { key: "ArrowLeft" });
    expect(view.getByRole("button", { name: "Show Claude Code MCP servers" })).toBeTruthy();
  });

  it("says scanning instead of showing zeros or dashes before the first scan", () => {
    const view = render(HomeView, props({ entries: null, report: null }));
    expect(view.getAllByText("scanning…").length).toBeGreaterThan(0);
    expect(view.container.textContent).not.toContain("—");
    expect(view.queryByRole("button", { name: /listening ports$/ })).toBeNull();
    expect(view.getByText("Waiting for the first scan…")).toBeTruthy();
  });

  it("makes every summary figure open its view and counts running MCP servers", async () => {
    const p = props();
    const view = render(HomeView, p);
    await fireEvent.click(view.getByRole("button", { name: /listening ports$/ }));
    expect(p.onview).toHaveBeenCalledWith("list");
    expect(view.getByRole("button", { name: /^1 of \d+ MCP running$/ })).toBeTruthy();
    expect(view.container.textContent).not.toContain("—");
  });

  it("explains products whose configuration isn't supported instead of 'not read'", () => {
    const view = render(HomeView, props());
    expect(view.getAllByText("Not supported")[0].getAttribute("title")).toMatch(/not supported for Docker Desktop/);
  });

  it("offers the inventory opt-in only while configuration isn't read", async () => {
    const oninventory = vi.fn();
    const off = render(HomeView, props({ report: mockAgents(), oninventory }));
    await fireEvent.click(off.getByRole("button", { name: "Turn on" }));
    expect(oninventory).toHaveBeenCalled();
    expect(off.getAllByRole("button", { name: "Read config" }).length).toBeGreaterThan(0);
    cleanup();
    const on = render(HomeView, props({ report: mockAgents(), oninventory, inventory: true }));
    expect(on.queryByRole("button", { name: "Turn on" })).toBeNull();
  });

  it("reviews a stop plan from attention without stopping anything itself", async () => {
    const onstopagent = vi.fn();
    const onstopentry = vi.fn();
    const view = render(HomeView, props({ onstopagent, onstopentry }));
    const buttons = view.getAllByRole("button", { name: "Review stop plan" });
    expect(buttons.length).toBeGreaterThan(0);
    for (const b of buttons) await fireEvent.click(b);
    expect(onstopagent.mock.calls.length + onstopentry.mock.calls.length).toBe(buttons.length);
  });

  it("lists activity with relative times and a watching baseline, and moves between rows with j/k", async () => {
    const changes: ChangeEvent[] = [{ id: "c1", at: 1_000, kind: "port_opened", title: ":5173 opened", detail: "web · PID 1", target: { view: "list", entryId: "x" } }];
    const view = render(HomeView, props({ changes, now: 1_000 + 4 * 60_000, since: 1_000 }));
    expect(view.getByText(":5173 opened")).toBeTruthy();
    expect(view.getByText("4m ago")).toBeTruthy();
    expect(view.getByText(/^Watching since/)).toBeTruthy();
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
