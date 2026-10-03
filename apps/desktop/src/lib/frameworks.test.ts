import { describe, expect, it } from "vitest";
import { ALL_BRANDS, brandFor, contrast, initials } from "./frameworks";
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
