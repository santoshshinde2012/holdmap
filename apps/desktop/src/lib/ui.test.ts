import { describe, expect, it } from "vitest";
import { ALL_BRANDS, brandFor, contrast, initials } from "./frameworks";
import { fuzzyScore, rank, type Command } from "./palette";
import { MOCK_SNAPSHOT } from "./mock";

const byPort = (p: number) => MOCK_SNAPSHOT.entries.find((e) => e.port === p)!;

describe("framework brands", () => {
  it("maps known frameworks", () => {
    expect(brandFor(byPort(3000)).glyph).toBe("N");
    expect(brandFor(byPort(6379)).glyph).toBe("Rd");
    expect(brandFor(byPort(5432)).kind).toBe("container");
    expect(brandFor(byPort(631)).kind).toBe("hidden");
  });
  it("derives initials for unknown names", () => {
    expect(initials("sand-egress-tun")).toBe("SE");
    expect(initials("x11vnc")).toBe("X1");
  });
  it("every tile colour passes WCAG AA with white text", () => {
    for (const b of ALL_BRANDS) expect(contrast(b.bg, b.fg), b.bg).toBeGreaterThanOrEqual(4.5);
  });
});

describe("command palette ranking", () => {
  const cmd = (title: string, keywords = ""): Command => ({ id: title, title, keywords, group: "Actions", run: () => {} });
  const cmds = [cmd("Stop :3000 — shop-web"), cmd("Refresh now"), cmd("Toggle dev servers filter", "dev"), cmd("Theme: dark")];
  it("prefers prefix and word-start matches", () => {
    expect(fuzzyScore("ref", "Refresh now")).toBeGreaterThan(fuzzyScore("ref", "Toggle dev servers filter"));
    expect(fuzzyScore("zzz", "Refresh now")).toBe(-1);
  });
  it("ranks the obvious command first", () => {
    expect(rank("stop 3000", cmds)[0].title).toContain("Stop :3000");
    expect(rank("dark", cmds)[0].title).toBe("Theme: dark");
    expect(rank("", cmds)).toHaveLength(4);
  });
});
