import { describe, expect, it } from "vitest";
import { describeStep, groupOf, humanBytes, humanDuration, matches, type Filters } from "./format";
import { MOCK_SNAPSHOT } from "./mock";

const base: Filters = { query: "", all: false, proto: "any", dev: false, mine: false, exposed: false };
const byPort = (p: number) => MOCK_SNAPSHOT.entries.find((e) => e.port === p)!;

describe("filters", () => {
  it("matches exact ports, prefixes and ranges", () => {
    expect(matches(byPort(3000), { ...base, query: ":3000" })).toBe(true);
    expect(matches(byPort(3001), { ...base, query: ":300" })).toBe(false);
    expect(matches(byPort(3001), { ...base, query: "300" })).toBe(true);
    expect(matches(byPort(5432), { ...base, query: "5000-6000" })).toBe(true);
    expect(matches(byPort(3000), { ...base, query: "5000-6000" })).toBe(false);
  });
  it("matches free text across project, framework and command", () => {
    expect(matches(byPort(3000), { ...base, query: "next shop" })).toBe(true);
    expect(matches(byPort(3000), { ...base, query: "postgres" })).toBe(false);
  });
  it("applies toggles", () => {
    expect(matches(byPort(6379), { ...base, dev: true })).toBe(false);
    expect(matches(byPort(8125), { ...base, proto: "udp" })).toBe(true);
    expect(matches(byPort(3000), { ...base, proto: "udp" })).toBe(false);
    expect(matches(byPort(8000), { ...base, exposed: true })).toBe(true);
  });
});

describe("grouping", () => {
  it("puts things in sensible groups", () => {
    expect(groupOf(byPort(3000))).toBe("dev");
    expect(groupOf(byPort(5432))).toBe("containers");
    expect(groupOf(byPort(6379))).toBe("data");
    expect(groupOf(byPort(5000))).toBe("system");
  });
});

describe("formatting", () => {
  it("formats durations and sizes", () => {
    expect(humanDuration(42)).toBe("42s");
    expect(humanDuration(3 * 3600 + 120)).toBe("3h 2m");
    expect(humanDuration(2 * 86400 + 3600)).toBe("2d 1h");
    expect(humanBytes(1536)).toBe("1.5 KB");
    expect(humanBytes(50 * 1024 * 1024)).toBe("50 MB");
  });
  it("describes steps", () => {
    expect(
      describeStep({ action: "verify_free", port: 3000, protocol: "tcp", timeout_ms: 3000 }),
    ).toBe("Check that TCP port 3000 is free (up to 3s)");
  });
});

describe("canOverride", () => {
  it("only offers 'stop anyway' for soft (overridable) protection", async () => {
    const { canOverride } = await import("./format");
    expect(canOverride({ blocked: { kind: "protected", message: "", overridable: true } })).toBe(true);
    expect(canOverride({ blocked: { kind: "protected", message: "" } })).toBe(false);
    expect(canOverride({ blocked: { kind: "protected", message: "", overridable: false } })).toBe(false);
    expect(canOverride({ blocked: { kind: "needs_elevation", message: "", overridable: true } })).toBe(false);
    expect(canOverride({ blocked: null })).toBe(false);
  });
});
