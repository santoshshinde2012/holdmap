import { describe, expect, it } from "vitest";
import { describeStep, groupOf, httpSummary, humanBytes, humanDuration, matches, stopTarget, type Filters } from "./format";
import { MOCK_SNAPSHOT, mockDevServers, mockHttp, mockPlan } from "./mock";

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
  it("keeps row stop targets specific to their transport even when the port number matches", () => {
    const tcp = byPort(3000);
    expect(stopTarget(tcp)).toBe("3000/tcp");
    expect(stopTarget({ ...tcp, protocol: "udp" })).toBe("3000/udp");
  });
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

describe("httpSummary", () => {
  it("shows status, title, redirect and server", () => {
    expect(httpSummary({ port: 3000, status: 200, reason: "OK", title: "Acme", server: null, location: null, elapsed_ms: 2 })).toBe("200 OK · “Acme”");
    expect(httpSummary({ port: 80, status: 302, reason: "Found", title: null, server: "nginx", location: "/login", elapsed_ms: 2 })).toBe("302 Found → /login · nginx");
  });
  it("mock databases don't speak HTTP", () => {
    const db = MOCK_SNAPSHOT.entries.find((e) => e.framework?.category === "database");
    if (db) expect(mockHttp(db.port)).toBeNull();
  });
});

describe("stop all dev servers (mock)", () => {
  it("plans every dev server of yours and nothing protected", () => {
    const dev = mockDevServers();
    expect(dev.length).toBeGreaterThan(1);
    expect(dev.every((e) => e.is_dev && e.is_mine && !e.protected && !e.container)).toBe(true);
    const plan = mockPlan("dev:all", false);
    expect(plan.blocked).toBeNull();
    expect(plan.summary).toContain(`Stop ${dev.length} dev servers`);
    const sig = plan.steps.find((s) => s.action === "signal_processes");
    expect(sig && sig.action === "signal_processes" ? sig.processes.length : 0).toBe(dev.length);
  });
});
