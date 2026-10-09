// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { prefGet, prefSet } from "./prefs";

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
});
