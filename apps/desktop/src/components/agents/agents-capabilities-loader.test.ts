// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AgentsView from "../AgentsView.svelte";
import { mockAgents } from "../../lib/mock";

const workspace = vi.hoisted(() => {
  let reject!: (error: Error) => void;
  const pending = new Promise<never>((_resolve, fail) => { reject = fail; });
  return { load: vi.fn(() => pending), reject };
});
vi.mock("./AgentCapabilitiesView.svelte", () => workspace.load());

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  Element.prototype.scrollIntoView ??= () => {};
  window.matchMedia ??= ((media: string) => ({ matches: false, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
});
afterEach(() => { cleanup(); localStorage.clear(); });

describe("optional capability workspace loading", () => {
  it("defers the module until requested and safely handles rejection after leaving with reload recovery", async () => {
    const onrefreshcapabilities = vi.fn();
    const props = { report: { ...mockAgents(), agents: [] }, entries: [], selectedAgent: null, dark: false, reduced: true,
      onselectagent() {}, onentry() {}, onreveal() {}, oneditor() {}, onstop() {}, onstopall() {}, onrefreshcapabilities };
    const view = render(AgentsView, { ...props, section: "overview" });
    await tick();
    expect(workspace.load).not.toHaveBeenCalled();
    expect(view.queryByText("Loading MCP & skills…")).toBeNull();

    await view.rerender({ ...props, section: "capabilities" });
    expect(view.getByRole("status").textContent).toContain("Loading MCP & skills…");
    await waitFor(() => expect(workspace.load).toHaveBeenCalledOnce());
    await view.rerender({ ...props, section: "overview" });
    workspace.reject(new Error("private-chunk-failure-fixture"));
    await tick();
    expect(view.queryByRole("alert")).toBeNull();
    expect(view.container.textContent).toContain("No agents or developer tools running");

    await view.rerender({ ...props, section: "capabilities" });
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Couldn't load MCP & skills"));
    expect(view.container.textContent).not.toContain("private-chunk-failure-fixture");
    expect(view.getByRole("button", { name: "Reload app" })).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Try again" }));
    // A browser can cache a failed module URL; the reload action must remain available.
    await waitFor(() => expect(view.getByRole("button", { name: "Reload app" })).toBeTruthy());
    expect(onrefreshcapabilities).not.toHaveBeenCalled();
  });
});
