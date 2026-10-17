// @vitest-environment jsdom
import { flushSync } from "svelte";
import { beforeEach, describe, expect, it } from "vitest";
import { AppPrefs, PANE_DEFAULT, PANE_MAX } from "./app-prefs.svelte";

const create = () => {
  let prefs!: AppPrefs;
  const stop = $effect.root(() => { prefs = new AppPrefs(); });
  flushSync();
  return { prefs, stop };
};

describe("app prefs", () => {
  beforeEach(() => localStorage.clear());

  it("starts from documented defaults", () => {
    const { prefs, stop } = create();
    expect([prefs.view, prefs.theme, prefs.sort, prefs.homeLayout, prefs.homeTimeline, prefs.inventoryOn, prefs.paneWidth, prefs.sideOpen])
      .toEqual(["list", "system", "group", "cards", true, false, PANE_DEFAULT, true]);
    stop();
  });

  it("reads saved choices, ignores bad ones and clamps the pane", () => {
    localStorage.setItem("holdmap.view", "home");
    localStorage.setItem("holdmap.theme", "neon");
    localStorage.setItem("holdmap.home.layout", "map");
    localStorage.setItem("holdmap.pane", "9999");
    localStorage.setItem("holdmap.agents.picture", "project");
    const { prefs, stop } = create();
    expect([prefs.view, prefs.theme, prefs.homeLayout, prefs.paneWidth, prefs.picture]).toEqual(["home", "system", "map", PANE_MAX, "context"]);
    stop();
  });

  it("writes every change back", () => {
    const { prefs, stop } = create();
    prefs.view = "agents";
    prefs.homeTimeline = false;
    prefs.collapsed = new Set(["apps"]);
    prefs.singleKeyShortcuts = false;
    flushSync();
    expect(localStorage.getItem("holdmap.view")).toBe("agents");
    expect(localStorage.getItem("holdmap.home.timeline")).toBe("0");
    expect(localStorage.getItem("holdmap.collapsed")).toBe('["apps"]');
    expect(localStorage.getItem("holdmap.singleKeyShortcuts")).toBe("0");
    stop();
  });
});
