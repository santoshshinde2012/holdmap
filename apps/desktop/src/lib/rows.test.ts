import { describe, expect, it } from "vitest";
import { MOCK_SNAPSHOT } from "./mock";
import {
  ROW_HEIGHT, UsageHistory, entryMemory, memoryHeavy, memoryHint, parseCollapsed, parseDensity, rowBadges, rowFramework, rowLabel, rowMeta, rowStatus, sparkPoints, splitBadges, toggled,
  type RowBadge,
} from "./rows";

const byPort = (p: number) => MOCK_SNAPSHOT.entries.find((e) => e.port === p)!;
const badge = (id: string): RowBadge => ({ id, label: id, tone: "neutral", tip: id });

describe("row status", () => {
  it("distinguishes listening, UDP, hidden and busy", () => {
    expect(rowStatus(byPort(3000)).status).toBe("listening");
    expect(rowStatus(byPort(8125)).status).toBe("bound");
    expect(rowStatus(byPort(631)).status).toBe("hidden");
    expect(rowStatus(byPort(3000), true)).toEqual({ status: "busy", label: "Stopping…" });
    expect(rowStatus(byPort(3000)).label).toBe("TCP · listening");
  });
});

describe("row badges", () => {
  it("orders by decision value: exposure and protection before context and trivia", () => {
    const ids = rowBadges(byPort(5432), 2).map((b) => b.id);
    expect(ids[0]).toBe("exposed");
    expect(ids).toEqual(["exposed", "container", "links", "memory"]);
    expect(rowBadges(byPort(5000)).map((b) => b.id)).toEqual(["exposed", "protected"]);
  });
  it("never shows more than two chips; the rest fold into +N with a tooltip", () => {
    const three = splitBadges([badge("a"), badge("b"), badge("c")]);
    expect(three.shown.map((b) => b.id)).toEqual(["a"]);
    expect(three.rest).toHaveLength(2);
    expect(three.overflow).toBe("b · c");
    const two = splitBadges([badge("a"), badge("b")]);
    expect(two.shown).toHaveLength(2);
    expect(two.overflow).toBeNull();
    for (let n = 0; n < 7; n++) {
      const s = splitBadges(Array.from({ length: n }, (_, i) => badge(String(i))));
      expect(s.shown.length + (s.overflow ? 1 : 0)).toBeLessThanOrEqual(2);
      expect(s.shown.length + s.rest.length).toBe(n);
    }
  });
});

describe("row meta and labels", () => {
  it("shows process, PID and branch; container image for containers", () => {
    expect(rowMeta(byPort(3000))).toMatchObject({ owner: "node", pid: 43000, branch: "feat/checkout" });
    expect(rowMeta(byPort(5432)).owner).toBe("postgres:16");
    expect(rowMeta(byPort(631)).owner).toBe("owner hidden");
  });
  it("omits a framework that repeats the title", () => {
    expect(rowFramework(byPort(3000))).toBe("Next.js");
    expect(rowFramework(byPort(6379))).toBeNull();
  });
  it("builds a screen-reader label with the important flags", () => {
    const e = byPort(8000);
    expect(rowLabel(e, { pinned: true, badges: rowBadges(e) })).toBe("Port 8000 tcp, ml-service, Django, pinned, exposed to network");
  });
});

describe("density and collapse persistence", () => {
  it("defaults to comfortable 44px; compact is 36px", () => {
    expect(parseDensity(null)).toBe("comfortable");
    expect(parseDensity("bogus")).toBe("comfortable");
    expect(parseDensity("compact")).toBe("compact");
    expect(ROW_HEIGHT).toEqual({ comfortable: 44, compact: 36 });
  });
  it("parses collapsed groups defensively and toggles immutably", () => {
    expect([...parseCollapsed('["dev",1,"system"]')]).toEqual(["dev", "system"]);
    expect(parseCollapsed("{nope").size).toBe(0);
    const a = new Set(["dev"]);
    const b = toggled(a, "dev");
    expect(a.has("dev")).toBe(true);
    expect(b.has("dev")).toBe(false);
    expect(toggled(b, "x").has("x")).toBe(true);
  });
});

describe("usage history", () => {
  const e = (id: string, cpu?: number) => ({ id, process: cpu === undefined ? null : ({ cpu_percent: cpu } as never) });
  it("keeps a bounded rolling window per entry and forgets vanished ones", () => {
    const h = new UsageHistory(3);
    for (const v of [1, 2, 3, 4]) h.push([e("a", v), e("b", v * 2)]);
    expect(h.get("a")).toEqual([2, 3, 4]);
    h.push([e("a", 5)]);
    expect(h.get("b")).toEqual([]);
    h.push([e("a"), e("c", Number.NaN)]);
    expect(h.get("a")).toEqual([3, 4, 5]);
    expect(h.get("c")).toEqual([]);
  });
  it("draws a sparkline inside its box", () => {
    expect(sparkPoints([1], 32, 14)).toBe("");
    const pts = sparkPoints([0, 10, 5], 32, 14).split(" ").map((p) => p.split(",").map(Number));
    expect(pts[0][0]).toBe(0);
    expect(pts[2][0]).toBe(32);
    for (const [, y] of pts) { expect(y).toBeGreaterThanOrEqual(0); expect(y).toBeLessThanOrEqual(14); }
    expect(pts[1][1]).toBeLessThan(pts[0][1]); // higher CPU → higher on screen
  });
});

describe("memory trend", () => {
  it("scales between min and max and stays flat on noise", async () => {
    const { rangePoints } = await import("./rows");
    expect(rangePoints([100, 200], 10, 10)).toBe("0,9 10,1");
    expect(rangePoints([1000, 1001, 1000], 10, 10)).toBe("0,5 5,5 10,5");
    expect(rangePoints([5], 10, 10)).toBe("");
  });
});

import type { PortEntry } from "./types";

const base = (over: Partial<PortEntry> = {}): PortEntry =>
  ({
    id: "tcp:1",
    port: 1,
    protocol: "tcp",
    state: "listen",
    addresses: ["127.0.0.1"],
    families: [],
    remote: null,
    exposure: "loopback",
    pid: 1,
    pids: [1],
    uid: null,
    user: null,
    process: { pid: 1, ppid: null, name: "node", exe: null, cmdline: [], cwd: null, uid: null, user: null, start_time: 0, start_token: 0, memory_bytes: 10 * 1024 * 1024 },
    project: null,
    framework: null,
    container: null,
    label: "node",
    is_dev: true,
    is_mine: true,
    protected: false,
    ...over,
  }) as PortEntry;

describe("app memory helpers", () => {
  it("prefers app_memory_bytes over the process alone", () => {
    const e = base({ app_memory_bytes: 300 * 1024 * 1024, helper_count: 3 });
    expect(entryMemory(e)).toBe(300 * 1024 * 1024);
    expect(memoryHeavy(e)).toBe(true);
    expect(memoryHint(e)).toMatch(/3 helpers/);
  });

  it("stays quiet for small processes", () => {
    expect(memoryHint(base())).toBeNull();
    expect(memoryHeavy(base())).toBe(false);
  });
});
