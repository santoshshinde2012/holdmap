import { describe, expect, it, vi } from "vitest";
import { appCommands, nextProto, type CommandActions, type CommandState } from "./app-commands";
import { MOCK_SNAPSHOT } from "./mock";

const actions = () => new Proxy({}, { get: (t: Record<string, unknown>, k: string) => (t[k] ??= vi.fn()) }) as unknown as CommandActions & Record<string, ReturnType<typeof vi.fn>>;
const state = (over: Partial<CommandState> = {}): CommandState => ({
  view: "list", graphDetailsOpen: false, selected: null, queryPort: null, clusters: [], entries: MOCK_SNAPSHOT.entries, update: null,
  pinned: () => false, powerAvailable: true, native: false, activeSchedules: 0, mod: "⌘",
  filters: { query: "", all: false, proto: "any", dev: false, mine: false, exposed: false }, sideOpen: true, selectedGraphNode: null,
  version: "0.9.0", platform: "browser", recording: { active: false, elapsedMs: 0, hasResult: false, checking: false, supported: true },
  snapshotHasResult: false, notify: false, density: "comfortable", autostart: false, graphQuery: "", singleKeys: true, ...over,
});
const ids = (st: CommandState) => appCommands(st, actions()).map((c) => c.id);

describe("palette commands", () => {
  it("offers a reviewed stop for each of my stoppable ports and none for protected ones", () => {
    const act = actions();
    const cmds = appCommands(state(), act);
    const mine = MOCK_SNAPSHOT.entries.find((e) => (e.process || e.container) && !e.protected && e.is_mine)!;
    const stop = cmds.find((c) => c.id === `stop-${mine.id}`)!;
    expect(stop.subtitle).toBe("asks first");
    stop.run();
    expect(act.stop).toHaveBeenCalledWith(mine);
    for (const e of MOCK_SNAPSHOT.entries.filter((x) => x.protected)) expect(cmds.some((c) => c.id === `stop-${e.id}`)).toBe(false);
  });

  it("keeps list filters and sorting to the list", () => {
    expect(ids(state())).toContain("dev");
    expect(ids(state({ view: "home" }))).not.toContain("dev");
    expect(ids(state({ view: "agents" }))).not.toContain("sort-g");
    expect(ids(state({ view: "graph" }))).not.toContain("density");
  });

  it("offers expanding collapsed port groups on the list only (their rows are out of keyboard reach)", () => {
    expect(ids(state())).not.toContain("expand-groups");
    expect(ids(state({ view: "home", collapsedGroups: 2 }))).not.toContain("expand-groups");
    const act = actions();
    const cmd = appCommands(state({ collapsedGroups: 2 }), act).find((c) => c.id === "expand-groups")!;
    expect(cmd.title).toBe("Expand collapsed port groups");
    cmd.run();
    expect(act.expandGroups).toHaveBeenCalledOnce();
  });

  it("only shows modifier and Esc hints when single keys are off", () => {
    const cmds = appCommands(state({ singleKeys: false }), actions());
    expect(cmds.find((c) => c.id === "dev")?.shortcut).toBeUndefined();
    expect(cmds.find((c) => c.id === "refresh")?.shortcut).toEqual(["⌘", "R"]);
    expect(cmds.find((c) => c.id === "clear")?.shortcut).toEqual(["Esc"]);
  });

  it("checks a typed port first and swaps recording commands by state", () => {
    expect(appCommands(state({ queryPort: 3000 }), actions())[0].id).toBe("check");
    expect(ids(state({ recording: { active: true, elapsedMs: 5000, hasResult: false, checking: false, supported: true } }))).toEqual(expect.arrayContaining(["screen-stop", "snapshot-recording"]));
    expect(ids(state({ recording: { active: false, elapsedMs: 0, hasResult: true, checking: false, supported: true } }))).toContain("screen-review");
  });

  it("filters through the app's own actions", () => {
    const act = actions();
    appCommands(state(), act).find((c) => c.id === "dev")!.run();
    expect(act.setFilters).toHaveBeenCalledWith({ dev: true });
    expect([nextProto("any"), nextProto("tcp"), nextProto("udp")]).toEqual(["tcp", "udp", "any"]);
  });
});

describe("palette agent and place commands", () => {
  const agents = [
    { id: "a", name: "Codex CLI", memory_bytes: 200 << 20, place: "api", stoppable: true },
    { id: "b", name: "Codex CLI", memory_bytes: 900 << 20, place: "web", stoppable: false },
  ];
  it("goes to Home and the Agents sections (Agents itself is ⇧A), but not to the view already open", () => {
    const act = actions();
    const cmds = appCommands(state({ view: "home" }), act);
    expect(cmds.map((c) => c.id)).toEqual(expect.arrayContaining(["go-capabilities", "go-flow", "go-insights"]));
    expect(cmds.some((c) => c.id === "go-home")).toBe(false);
    cmds.find((c) => c.id === "go-capabilities")!.run();
    expect(act.go).toHaveBeenCalledWith("capabilities");
    expect(cmds.find((c) => c.id === "go-insights")!.title).toBe("Go to Security");
  });
  it("names the agent using the most RAM and opens it", () => {
    const act = actions();
    const top = appCommands(state({ agents }), act).find((c) => c.id === "agent-top-ram")!;
    expect(top.title).toBe("Most RAM: Codex CLI");
    expect(top.subtitle).toBe("900 MB");
    top.run();
    expect(act.openAgent).toHaveBeenCalledWith("b");
  });
  it("opens each agent, told apart by its place, and offers a reviewed stop only when it has ports to stop", () => {
    const act = actions();
    const cmds = appCommands(state({ agents }), act);
    expect(cmds.find((c) => c.id === "agent-a")!.subtitle).toBe("200 MB RAM · api");
    expect(cmds.find((c) => c.id === "agent-b")!.subtitle).toBe("900 MB RAM · web");
    const stop = cmds.find((c) => c.id === "agent-stop-a")!;
    expect(stop.subtitle).toBe("asks first");
    stop.run();
    expect(act.stopAgent).toHaveBeenCalledWith("a");
    expect(cmds.some((c) => c.id === "agent-stop-b")).toBe(false);
  });
});
