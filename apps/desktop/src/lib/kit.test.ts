// Pure logic behind the UI kit and the details pane.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { nextIndex, typeahead } from "./roving";
import { validateHost, validateLabel, validatePort, validateRange } from "./validate";
import { cliCommands, detailTabs, resolveTab, stopState } from "./detail";
import { cadenceText, SETTINGS_SECTIONS } from "./settings";
import { place } from "./tooltip";
import { codeSpans } from "./text";
import { contrast } from "./frameworks";
import { MOCK_SNAPSHOT, mockExplain, mockPlan } from "./mock";

const entry = (port: number) => MOCK_SNAPSHOT.entries.find((e) => e.port === port)!;

describe("roving focus", () => {
  it("moves and loops along the orientation", () => {
    expect(nextIndex(0, "ArrowRight", 3)).toBe(1);
    expect(nextIndex(2, "ArrowRight", 3)).toBe(0);
    expect(nextIndex(0, "ArrowLeft", 3)).toBe(2);
    expect(nextIndex(0, "ArrowDown", 3)).toBeNull();
    expect(nextIndex(0, "ArrowDown", 3, { orientation: "vertical" })).toBe(1);
    expect(nextIndex(1, "Home", 3)).toBe(0);
    expect(nextIndex(0, "End", 3)).toBe(2);
    expect(nextIndex(0, "x", 3)).toBeNull();
  });
  it("skips disabled items and respects loop=false", () => {
    expect(nextIndex(0, "ArrowRight", 3, { disabled: [false, true, false] })).toBe(2);
    expect(nextIndex(2, "ArrowRight", 3, { loop: false })).toBe(2);
    expect(nextIndex(0, "Home", 3, { disabled: [true, false, false] })).toBe(1);
  });
  it("type-ahead finds the next match after the current item", () => {
    const l = ["Grouped", "Cluster", "Port", "Newest", "Memory"];
    expect(typeahead(l, "p", 0)).toBe(2);
    expect(typeahead(l, "M", 0)).toBe(4);
    expect(typeahead(l, "z", 0)).toBeNull();
    expect(typeahead(l, "", 0)).toBeNull();
  });
});

describe("validation", () => {
  it("ports", () => {
    expect(validatePort("3000")).toBeNull();
    expect(validatePort(65535)).toBeNull();
    expect(validatePort("")).toMatch(/Enter/);
    expect(validatePort("0")).toMatch(/1 to 65535/);
    expect(validatePort("70000")).toMatch(/1 to 65535/);
    expect(validatePort("30a")).toMatch(/whole numbers/);
  });
  it("ranges and labels", () => {
    expect(validateRange(4, 1, 60)).toBeNull();
    expect(validateRange(null, 1, 60, " s")).toMatch(/1 to 60 s/);
    expect(validateRange(1.5, 1, 60)).toMatch(/whole/);
    expect(validateRange(61, 1, 60)).toMatch(/1–60/);
    expect(validateLabel("x".repeat(40))).toBeNull();
    expect(validateLabel("x".repeat(41))).toMatch(/40/);
  });
  it("hosts mirror the core validator (no ssh option injection)", () => {
    for (const ok of ["devbox", "user@10.0.0.5", "deploy@staging.internal:2222", "[::1]", "me@[fe80::1]:22", "my_host-1.local"]) expect(validateHost(ok), ok).toBeNull();
    for (const bad of ["", "-oProxyCommand=x", "a@-b", "host;rm -rf", "host:0", "host:99999", "host:22x", "[::1", "@host", "a b", "user@", "$(id)"]) expect(validateHost(bad), bad).not.toBeNull();
  });
});

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
    expect(c.map((x) => x.cmd)).toContain("portwise stop 3000 --dry-run");
    expect(new Set(c.map((x) => x.cmd)).size).toBe(c.length);
    expect(cliCommands(entry(631), null).some((x) => x.cmd.includes("stop"))).toBe(false);
  });
});

describe("settings, tooltip and text helpers", () => {
  it("describes the cadence the backend uses (2.5×, at least 10 s)", () => {
    expect(cadenceText(4)).toBe("Every 4 s while the window is open, every 10 s in the background.");
    expect(cadenceText(10)).toMatch(/every 25 s/);
    expect(cadenceText(0)).toMatch(/^Every 1 s/);
    expect(SETTINGS_SECTIONS.map((s) => s.id)).toEqual(["general", "appearance", "notifications", "scanning", "about"]);
  });
  it("places tooltips above, flips below near the top and clamps to the viewport", () => {
    expect(place({ top: 100, bottom: 120, left: 100, width: 40 }, { width: 60, height: 20 }, 800, 600)).toEqual({ top: 74, left: 90 });
    expect(place({ top: 4, bottom: 24, left: 100, width: 40 }, { width: 60, height: 20 }, 800, 600).top).toBe(30);
    expect(place({ top: 100, bottom: 120, left: 790, width: 10 }, { width: 100, height: 20 }, 800, 600).left).toBe(696);
    expect(place({ top: 100, bottom: 120, left: 0, width: 10 }, { width: 100, height: 20 }, 800, 600).left).toBe(4);
  });
  it("splits backtick code spans", () => {
    expect(codeSpans("run `sudo portwise stop 631`.")).toEqual([{ code: false, text: "run " }, { code: true, text: "sudo portwise stop 631" }, { code: false, text: "." }]);
    expect(codeSpans("plain")).toEqual([{ code: false, text: "plain" }]);
  });
});

describe("design tokens meet WCAG AA", () => {
  const css = readFileSync(fileURLToPath(new URL("../app.css", import.meta.url)), "utf8");
  const block = (sel: string) => css.slice(css.indexOf(sel), css.indexOf("}", css.indexOf(sel)));
  const tok = (b: string, n: string) => b.match(new RegExp(`--${n}:\\s*(#[0-9a-fA-F]{6})`))![1];
  for (const [theme, sel] of [["light", ":root {"], ["dark", ':root[data-theme="dark"] {']] as const) {
    const b = block(sel);
    it(`${theme}: placeholder, hint and button text contrast`, () => {
      for (const bg of ["input-bg", "surface", "bg"]) {
        expect(contrast(tok(b, "faint"), tok(b, bg)), `faint on ${bg}`).toBeGreaterThanOrEqual(4.5);
        expect(contrast(tok(b, "muted"), tok(b, bg)), `muted on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
      expect(contrast(tok(b, "accent-fg"), tok(b, "accent"))).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tok(b, "danger-fg"), tok(b, "danger"))).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tok(b, "danger"), tok(b, "surface")), "danger text").toBeGreaterThanOrEqual(4.5);
    });
  }
});
