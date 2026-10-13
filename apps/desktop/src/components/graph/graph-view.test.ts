// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
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
