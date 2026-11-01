import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { cadenceText, nextTrayItems, SETTINGS_SECTIONS, THEME_ICON, THEME_LABEL, TRAY_ITEM_OPTIONS, trayItemOptions, trayWidgetName } from "./settings";

describe("settings", () => {
  it("describes the cadence the backend uses (2.5×, at least 10 s)", () => {
    expect(cadenceText(4)).toBe("Every 4 s while the window is open, every 10 s in the background.");
    expect(cadenceText(10)).toMatch(/every 25 s/);
    expect(cadenceText(0)).toMatch(/^Every 1 s/);
    expect(SETTINGS_SECTIONS.map((s) => s.id)).toEqual(["general", "appearance", "notifications", "scanning", "analyzers", "power", "about"]);
  });

  it("gives every theme its own icon that exists and isn't a device/server glyph", () => {
    const icons = readFileSync(new URL("../components/Icon.svelte", import.meta.url), "utf8");
    const values = Object.values(THEME_ICON);
    expect(new Set(values).size).toBe(values.length);
    for (const name of values) {
      expect(icons).toMatch(new RegExp(`\\b"?${name}"?: "`));
      expect(["monitor", "server"]).not.toContain(name);
    }
    expect(THEME_LABEL.system).toBe("match system");
  });

  it("offers the tray style only where text can sit beside the icon", () => {
    expect(trayWidgetName("macos", true)).toBe("Menu bar");
    expect(trayWidgetName("linux", true)).toBe("Panel");
    expect(trayWidgetName("windows", true)).toBeNull();
    expect(trayWidgetName("linux", false)).toBeNull();
    expect(trayWidgetName("macos", undefined)).toBeNull();
    expect(trayWidgetName("browser", false)).toBe("Menu bar");
    // Same ids and order as `Config::TRAY_ITEMS`; Linux's label holds text only.
    expect(TRAY_ITEM_OPTIONS.map((o) => o.value)).toEqual(["icon", "agents", "needs", "cpu", "dev", "cpu_history", "agent_bars"]);
    expect(trayItemOptions("linux").map((o) => o.value)).toEqual(["agents", "needs", "cpu", "dev", "cpu_history"]);
    expect(trayItemOptions("macos")).toHaveLength(7);
  });

  it("turns menu-bar items on and off in menu-bar order, never down to none", () => {
    expect(nextTrayItems(["needs", "agents"], "icon", true)).toEqual(["icon", "agents", "needs"]);
    expect(nextTrayItems(["agents", "needs"], "agents", false)).toEqual(["needs"]);
    expect(nextTrayItems(["needs"], "needs", false)).toEqual(["icon"]);
  });
});
