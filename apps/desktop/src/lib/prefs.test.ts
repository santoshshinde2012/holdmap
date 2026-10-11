// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { prefGet, prefGetBoolean, prefSet } from "./prefs";

afterEach(() => {
  localStorage.clear();
});

describe("prefs", () => {
  it("reads holdmap keys", () => {
    localStorage.setItem("holdmap.theme", "dark");
    expect(prefGet("theme")).toBe("dark");
  });

  it("migrates a legacy pw key once", () => {
    localStorage.setItem("pw.view", "graph");
    expect(prefGet("view")).toBe("graph");
    expect(localStorage.getItem("holdmap.view")).toBe("graph");
    expect(localStorage.getItem("pw.view")).toBeNull();
  });

  it("prefers holdmap over pw", () => {
    localStorage.setItem("holdmap.sort", "port");
    localStorage.setItem("pw.sort", "group");
    expect(prefGet("sort")).toBe("port");
  });

  it("writes holdmap and clears pw", () => {
    localStorage.setItem("pw.onboarded", "1");
    prefSet("onboarded", "1");
    expect(localStorage.getItem("holdmap.onboarded")).toBe("1");
    expect(localStorage.getItem("pw.onboarded")).toBeNull();
  });

  it("uses a toggle's default and preserves the user's explicit choice", () => {
    expect(prefGetBoolean("singleKeyShortcuts", true)).toBe(true);
    prefSet("singleKeyShortcuts", "0");
    expect(prefGetBoolean("singleKeyShortcuts", true)).toBe(false);
    prefSet("singleKeyShortcuts", "1");
    expect(prefGetBoolean("singleKeyShortcuts", false)).toBe(true);
    prefSet("singleKeyShortcuts", "invalid");
    expect(prefGetBoolean("singleKeyShortcuts", true)).toBe(true);
    expect(prefGetBoolean("singleKeyShortcuts", false)).toBe(false);
  });
});
