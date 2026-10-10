import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { cadenceText, SETTINGS_SECTIONS, THEME_ICON, THEME_LABEL } from "./settings";

describe("settings", () => {
  it("describes the cadence the backend uses (2.5×, at least 10 s)", () => {
    expect(cadenceText(4)).toBe("Every 4 s while the window is open, every 10 s in the background.");
    expect(cadenceText(10)).toMatch(/every 25 s/);
    expect(cadenceText(0)).toMatch(/^Every 1 s/);
    expect(SETTINGS_SECTIONS.map((s) => s.id)).toEqual(["general", "appearance", "notifications", "scanning", "power", "about"]);
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
});
