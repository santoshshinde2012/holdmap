import { describe, expect, it } from "vitest";
import { canRestart, cliCommands, curlCommand, detailTabs, glance, killCommand, killHelpersCommand, peerLabel, projectFolder, resolveTab, stopState } from "./detail";
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
    expect(c.map((x) => x.cmd)).toContain("holdmap stop 3000 --dry-run");
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

describe("quick actions", () => {
  const e = MOCK_SNAPSHOT.entries.find((x) => x.port === 3000)!;
  it("builds copyable commands", () => {
    expect(curlCommand(e)).toBe("curl -i http://localhost:3000/");
    expect(killCommand(e, "macos")).toBe(`kill -TERM ${e.process!.pid}`);
    expect(killCommand(e, "windows")).toBe(`taskkill /PID ${e.process!.pid}`);
    const ctr = MOCK_SNAPSHOT.entries.find((x) => x.container)!;
    expect(killCommand(ctr, "macos")).toBe(`docker stop ${ctr.container!.name}`);
    expect(projectFolder(e)).toBe(e.project?.root ?? e.process?.cwd ?? null);
  });
  it("offers restart only for a stoppable process of yours", () => {
    expect(canRestart(e, mockPlan("3000", false))).toBe(e.is_mine && !e.protected);
    expect(canRestart(e, null)).toBe(false);
    const ctr = MOCK_SNAPSHOT.entries.find((x) => x.container)!;
    expect(canRestart(ctr, mockPlan(String(ctr.port), false))).toBe(false);
  });
  it("labels peers", () => {
    expect(peerLabel({ address: "127.0.0.1", connections: 3, process: "Google Chrome" })).toBe("Google Chrome ×3");
    expect(peerLabel({ address: "192.168.1.24", connections: 1, process: null })).toBe("192.168.1.24");
  });
});

describe("killHelpersCommand", () => {
  it("offers a pkill -P line only when there are unprotected helpers", () => {
    const e = {
      protected: false,
      container: null,
      helper_count: 3,
      process: { pid: 42 },
    } as any;
    expect(killHelpersCommand(e, "macos")).toBe("pkill -TERM -P 42");
    expect(killHelpersCommand({ ...e, helper_count: 0 }, "macos")).toBeNull();
    expect(killHelpersCommand({ ...e, protected: true }, "macos")).toBeNull();
    expect(killHelpersCommand(e, "windows")).toBeNull();
  });
});
