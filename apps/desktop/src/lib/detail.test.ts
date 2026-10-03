import { describe, expect, it } from "vitest";
import { cliCommands, detailTabs, glance, resolveTab, stopState } from "./detail";
import { MOCK_SNAPSHOT, mockExplain, mockPlan } from "./mock";

const entry = (port: number) => MOCK_SNAPSHOT.entries.find((e) => e.port === port)!;

describe("details pane logic", () => {
  it("offers tabs by what the entry has", () => {
    const web = entry(3000);
    const ids = (t: { id: string }[]) => t.map((x) => x.id);
    expect(ids(detailTabs(web, mockExplain(3000), { deps: 1, users: 0, cluster: false }))).toEqual(["overview", "connections", "process", "network", "commands"]);
    const hidden = entry(631);
    expect(ids(detailTabs(hidden, mockExplain(631), { deps: 0, users: 0, cluster: false }))).toEqual(["overview", "network", "commands"]);
    expect(resolveTab("process", detailTabs(hidden, null, { deps: 0, users: 0, cluster: false }))).toBe("overview");
    expect(resolveTab("network", detailTabs(hidden, null, { deps: 0, users: 0, cluster: false }))).toBe("network");
  });
  it("flags exposure and blocked plans on the tabs", () => {
    const t = detailTabs(entry(631), mockExplain(631), { deps: 0, users: 0, cluster: false });
    expect(t[0].alert).toBe(true);
    const exposed = detailTabs(entry(8000), null, { deps: 0, users: 0, cluster: false });
    expect(exposed.find((x) => x.id === "network")?.alert).toBe(true);
  });
  it("derives what the footer may do", () => {
    expect(stopState(entry(3000), mockPlan("3000", false))).toEqual({ stoppable: true, overridable: false, reason: null });
    const vs = stopState(entry(49152), mockPlan("49152", false));
    expect(vs.stoppable).toBe(false);
    expect(vs.overridable).toBe(true);
    expect(vs.reason).toMatch(/Protected/);
    expect(stopState(entry(5000), mockPlan("5000", false)).overridable).toBe(false);
    expect(stopState(entry(631), null).reason).toMatch(/admin/);
  });
  it("lists CLI equivalents without duplicates", () => {
    const c = cliCommands(entry(3000), mockExplain(3000));
    expect(c.map((x) => x.cmd)).toContain("portwise stop 3000 --dry-run");
    expect(new Set(c.map((x) => x.cmd)).size).toBe(c.length);
    expect(cliCommands(entry(631), null).some((x) => x.cmd.includes("stop"))).toBe(false);
  });
});

describe("at a glance", () => {
  it("counts ports and lists exposed ones, owned ones first", () => {
    const g = glance(MOCK_SNAPSHOT.entries, 2);
    expect(g.total).toBe(MOCK_SNAPSHOT.entries.length);
    expect(g.dev).toBe(MOCK_SNAPSHOT.entries.filter((e) => e.is_dev).length);
    expect(g.exposedCount).toBe(MOCK_SNAPSHOT.entries.filter((e) => e.exposure === "all_interfaces").length);
    expect(g.exposed.length).toBeLessThanOrEqual(2);
    expect(g.exposed.every((e) => e.exposure === "all_interfaces")).toBe(true);
    const ports = g.exposed.map((e) => e.port);
    expect(ports).toEqual([...ports].sort((a, b) => a - b));
  });
});
