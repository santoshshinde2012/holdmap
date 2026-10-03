import { describe, expect, it } from "vitest";
import { cadenceText, SETTINGS_SECTIONS } from "./settings";

describe("settings", () => {
  it("describes the cadence the backend uses (2.5×, at least 10 s)", () => {
    expect(cadenceText(4)).toBe("Every 4 s while the window is open, every 10 s in the background.");
    expect(cadenceText(10)).toMatch(/every 25 s/);
    expect(cadenceText(0)).toMatch(/^Every 1 s/);
    expect(SETTINGS_SECTIONS.map((s) => s.id)).toEqual(["general", "appearance", "notifications", "scanning", "about"]);
  });
});
