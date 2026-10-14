// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import GraphView from "../GraphView.svelte";
import { mockTopology } from "../../lib/mock";
import { FORCE_NODE_LIMIT } from "../../lib/graph";

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  window.matchMedia ??= ((media: string) => ({ matches: false, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
});
afterEach(() => { cleanup(); localStorage.clear(); });

describe("large graph controls", () => {
  it("shows Layered and explains the disabled Force control when a saved Force graph grows", () => {
    localStorage.setItem("holdmap.layout", "force");
    const graph = mockTopology();
    graph.nodes = Array.from({ length: FORCE_NODE_LIMIT + 1 }, (_, index) => ({ ...graph.nodes[0], id: `large-${index}`, cluster: null, ports: [] }));
    graph.edges = [];
    graph.clusters = [];
    const view = render(GraphView, { graph, selectedNode: null, dark: false, reduced: true, onselect() {}, onstopcluster() {} });
    expect(view.getByRole("radio", { name: "Layered" }).getAttribute("aria-checked")).toBe("true");
    const force = view.getByRole("radio", { name: "Force" }) as HTMLButtonElement;
    expect(force.disabled).toBe(true);
    expect(force.getAttribute("aria-checked")).toBe("false");
    expect(force.title).toContain(`up to ${FORCE_NODE_LIMIT} services`);
  });
});

describe("graph chrome", () => {
  it("explains an unconnected graph and offers All services", () => {
    const graph = mockTopology();
    graph.edges = [];
    graph.clusters = [];
    const view = render(GraphView, { graph, selectedNode: null, dark: false, reduced: true, onselect() {}, onstopcluster() {}, all: false, ontoggleall() {} });
    expect(view.getByRole("status").textContent).toMatch(/aren't connected/i);
    expect(view.getByRole("button", { name: "Show all services" })).toBeTruthy();
  });

  it("disables External when there are no remote hosts", () => {
    const graph = mockTopology();
    graph.nodes = graph.nodes.filter((n) => n.kind !== "external");
    graph.edges = graph.edges.filter((e) => e.kind !== "outbound");
    const view = render(GraphView, { graph, selectedNode: null, dark: false, reduced: true, onselect() {}, onstopcluster() {} });
    const ext = view.getByRole("button", { name: "External" }) as HTMLButtonElement;
    expect(ext.disabled).toBe(true);
    expect(ext.getAttribute("aria-pressed")).toBe("false");
    expect(ext.title).toMatch(/no outbound/i);
  });

  it("points at All services when the filtered graph is empty", () => {
    const graph = { ...mockTopology(), nodes: [], edges: [], clusters: [] };
    const view = render(GraphView, { graph, selectedNode: null, dark: false, reduced: true, onselect() {}, onstopcluster() {}, all: false, ontoggleall() {} });
    expect(view.getByText("No connected project services")).toBeTruthy();
    expect(view.getByRole("button", { name: "Show all services" })).toBeTruthy();
  });

  it("hides the legend and counts so the canvas can use the full width", async () => {
    const view = render(GraphView, { graph: mockTopology(), selectedNode: null, dark: false, reduced: true, onselect() {}, onstopcluster() {} });
    expect(view.container.querySelector(".legend")).toBeTruthy();
    expect(view.container.querySelector(".stats")).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Legend" }));
    expect(view.container.querySelector(".legend")).toBeNull();
    expect(view.container.querySelector(".stats")).toBeNull();
    expect(view.getByRole("button", { name: "Legend" }).getAttribute("aria-pressed")).toBe("false");
  });
});
