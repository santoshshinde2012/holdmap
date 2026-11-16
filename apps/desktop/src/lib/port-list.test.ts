import { describe, expect, it } from "vitest";
import { MOCK_SNAPSHOT } from "./mock";
import { bundleView, clearedFilters, filtersActive, portSections, portStats, queryPort, reachablePorts, visiblePorts } from "./port-list";
import type { Filters } from "./format";

const none: Filters = { query: "", all: false, proto: "any", dev: false, mine: false, exposed: false };
const entries = MOCK_SNAPSHOT.entries;

describe("port list view model", () => {
  it("filters without mutating and orders by the chosen sort", () => {
    const before = entries.map((e) => e.id);
    const byPort = visiblePorts(entries, none, "port");
    expect(entries.map((e) => e.id)).toEqual(before);
    expect(byPort.map((e) => e.port)).toEqual([...byPort.map((e) => e.port)].sort((a, b) => a - b));
    const dev = visiblePorts(entries, { ...none, dev: true }, "port");
    expect(dev.length).toBeGreaterThan(0);
    expect(dev.every((e) => e.is_dev)).toBe(true);
  });

  it("puts pinned ports first and drops empty sections", () => {
    const visible = visiblePorts(entries, none, "group");
    const sections = portSections(visible, new Set([visible[visible.length - 1].port]), "group", null);
    expect(sections[0].id).toBe("pinned");
    expect(sections.every((s) => s.items.length > 0)).toBe(true);
    expect(sections.flatMap((s) => s.items).length).toBe(visible.length);
    const flat = portSections(visible, new Set(), "port", null);
    expect(flat).toHaveLength(1);
    expect(flat[0].title).toBe("");
  });

  it("skips collapsed titled sections for keyboard reach", () => {
    const sections = portSections(visiblePorts(entries, none, "group"), new Set(), "group", null);
    const reach = reachablePorts(sections, new Set([sections[0].id]));
    expect(reach.length).toBe(sections.slice(1).reduce((n, s) => n + s.items.length, 0));
  });

  it("folds repeated non-dev sockets of one process until they are opened or searched", () => {
    const base = entries.find((e) => e.process && !e.is_dev)!;
    const clones = [0, 1, 2, 3].map((i) => ({ ...base, id: `bundle-${i}`, port: 41000 + i, is_dev: false, container: null, protected: false, is_mine: true, framework: null, process: { ...base.process!, pid: 4242, name: "Chrome Helper" } }));
    const sections = portSections(clones, new Set(), "group", null);
    const folded = bundleView(sections, new Set(), false);
    expect(folded.hidden.size).toBe(3);
    expect(folded.more.get(clones[0].id)).toMatchObject({ key: "4242", count: 3, open: false });
    expect(reachablePorts(sections, new Set(), folded.hidden).map((e) => e.id)).toEqual([clones[0].id]);
    expect(bundleView(sections, new Set(["4242"]), false).hidden.size).toBe(0);
    expect(bundleView(sections, new Set(), true).hidden.size).toBe(0);
    const devs = clones.map((e) => ({ ...e, is_dev: true }));
    expect(bundleView(portSections(devs, new Set(), "group", null), new Set(), false).hidden.size).toBe(0);
  });

  it("counts, port queries and filter state", () => {
    const s = portStats(entries);
    expect(s.total).toBe(entries.length);
    expect(s.exposed).toBe(entries.filter((e) => e.exposure === "all_interfaces").length);
    expect(queryPort(":3000")).toBe(3000);
    expect(queryPort(" 8080 ")).toBe(8080);
    expect(queryPort("0")).toBeNull();
    expect(queryPort("70000")).toBeNull();
    expect(queryPort("vite")).toBeNull();
    expect(filtersActive(none)).toBe(false);
    expect(filtersActive({ ...none, proto: "udp" })).toBe(true);
    expect(filtersActive(clearedFilters({ ...none, query: "x", dev: true, all: true }))).toBe(false);
    expect(clearedFilters({ ...none, all: true }).all).toBe(true);
  });
});
