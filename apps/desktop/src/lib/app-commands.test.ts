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
