import { describe, expect, it } from "vitest";
import { decideKey, type KeyContext, type KeyState } from "./app-shortcuts";

const ctx = (over: Partial<KeyContext> = {}): KeyContext => ({ blocked: false, navigation: true, command: false, character: true, editing: false, control: false, ...over } as KeyContext);
const state = (over: Partial<KeyState> = {}): KeyState => ({
  view: "list", singleKeys: true, modalOpen: false, snapshotBusy: false, flowView: false,
  focus: { inSearch: false, recordButton: false, snapshotButton: false, graphService: false },
  graph: { detailsOrSelection: false, query: false }, agents: { query: false },
  list: { selected: true, filtersActive: false, hasRows: true },
  selection: { present: true, stoppable: true, openable: true }, graphCluster: false, ...over,
});
const intent = (key: string, c = ctx(), st = state(), shift = false) => decideKey(key, shift, c, st)?.intent ?? null;

describe("app shortcuts", () => {
  it("opens the palette, settings and search with the modifier", () => {
    const cmd = ctx({ command: true, character: false });
    expect(intent("k", cmd)).toEqual({ do: "palette" });
    expect(intent(",", cmd)).toEqual({ do: "settings" });
    expect(intent("f", cmd)).toEqual({ do: "search" });
    expect(intent("/")).toEqual({ do: "search" });
  });

  it("stops the selected port through the plan with Backspace, forcing with Shift", () => {
    expect(decideKey("Backspace", false, ctx(), state())).toEqual({ intent: { do: "stop", force: false }, prevent: true });
    expect(intent("Delete", ctx(), state(), true)).toEqual({ do: "stop", force: true });
    expect(decideKey("Backspace", false, ctx(), state({ selection: { present: true, stoppable: false, openable: false } }))).toBeNull();
  });

  it("leaves keys alone in dialogs, native controls and other text fields", () => {
    expect(decideKey("j", false, ctx(), state({ modalOpen: true }))).toBeNull();
    expect(decideKey("g", false, ctx({ control: true }), state())).toBeNull();
    expect(decideKey("g", false, ctx({ editing: true }), state())).toBeNull();
    expect(decideKey("g", false, ctx({ blocked: true }), state())).toBeNull();
  });

  it("drops single-key shortcuts when they are turned off but keeps modifiers and Esc", () => {
    const off = state({ singleKeys: false });
    expect(decideKey("g", false, ctx(), off)).toBeNull();
    expect(intent("r", ctx({ command: true, character: false }), off)).toEqual({ do: "refresh" });
    expect(intent("Escape", ctx(), off)).toEqual({ do: "list-deselect" });
  });

  it("flips the Home layout with V only on Home", () => {
    expect(intent("v", ctx(), state({ view: "home" }))).toEqual({ do: "toggle-home-layout" });
    expect(intent("v", ctx(), state())).toBeNull();
  });

  it("keeps list-only filters to the list", () => {
    expect(intent("d")).toEqual({ do: "toggle-dev" });
    expect(intent("d", ctx(), state({ view: "graph" }))).toBeNull();
  });

  it("moves by view and keeps Flow's own arrows", () => {
    expect(intent("ArrowDown")).toEqual({ do: "move", delta: 1 });
    expect(intent("PageUp")).toEqual({ do: "move", delta: -10 });
    expect(intent("ArrowRight", ctx(), state({ view: "graph", list: { selected: false, filtersActive: false, hasRows: true } }))).toEqual({ do: "move", delta: 1 });
    expect(decideKey("ArrowDown", false, ctx(), state({ view: "agents", flowView: true }))).toBeNull();
    expect(decideKey("Home", false, ctx(), state({ list: { selected: false, filtersActive: false, hasRows: false } }))).toBeNull();
  });

  it("resolves Escape from the innermost state outwards", () => {
    expect(intent("Escape", ctx({ editing: true }), state({ focus: { inSearch: true, recordButton: false, snapshotButton: false, graphService: false } }))).toEqual({ do: "blur-search" });
    expect(intent("Escape", ctx(), state({ view: "graph", graph: { detailsOrSelection: true, query: true } }))).toEqual({ do: "graph-close-details" });
    expect(intent("Escape", ctx(), state({ view: "graph", graph: { detailsOrSelection: false, query: true } }))).toEqual({ do: "graph-clear-query" });
    expect(decideKey("Escape", false, ctx(), state({ view: "graph" }))).toBeNull();
    expect(intent("Escape", ctx(), state({ view: "agents", agents: { query: true } }))).toEqual({ do: "agents-clear-query" });
    expect(intent("Escape", ctx(), state({ list: { selected: true, filtersActive: true, hasRows: true } }))).toEqual({ do: "clear-filters" });
    expect(intent("Escape", ctx(), state({ snapshotBusy: true, modalOpen: true }))).toEqual({ do: "cancel-snapshot" });
  });

  it("lets Enter reach buttons in Agents and opens the drawer in the list without suppressing it", () => {
    expect(decideKey("Enter", false, ctx(), state({ view: "agents" }))).toEqual({ intent: null, prevent: false });
    expect(decideKey("Enter", false, ctx(), state())).toEqual({ intent: { do: "open-drawer" }, prevent: false });
    expect(decideKey("Enter", false, ctx(), state({ view: "graph" }))).toEqual({ intent: { do: "graph-open-selected", blur: false }, prevent: true });
  });

  it("stops a graph cluster with S only when one is selected", () => {
    expect(intent("s", ctx(), state({ view: "graph", graphCluster: true }))).toEqual({ do: "stop-cluster" });
    expect(intent("s", ctx(), state({ view: "graph" }))).toBeNull();
  });
});
