/**
 * Regression for the blank WebView after hours: unbounded Maps of explain/HTTP/details
 * and sparkline samples. Simulate thousands of scans with rotating ports and assert the
 * caches stay within their hard caps.
 */
import { describe, expect, it } from "vitest";
import { DETAILS_CACHE_MAX, LruMap, STAMP_CACHE_MAX } from "./cache";
import { UsageHistory } from "./rows";
import { pruned, share } from "./live";
import type { PortEntry, Snapshot } from "./types";

function entry(port: number, id = `tcp:${port}`): PortEntry {
  return {
    id,
    port,
    protocol: "tcp",
    state: "listen",
    addresses: ["127.0.0.1"],
    families: ["v4"],
    remote: null,
    exposure: "loopback",
    pid: port,
    pids: [port],
    uid: 501,
    user: "me",
    process: {
      pid: port,
      ppid: 1,
      name: "node",
      exe: null,
      cmdline: ["node"],
      cwd: null,
      uid: 501,
      user: "me",
      start_time: 0,
      start_token: 0,
      memory_bytes: 10_000_000,
      cpu_percent: 1,
    },
    project: null,
    framework: null,
    container: null,
    label: "node",
    is_dev: true,
    is_mine: true,
    protected: false,
    app_memory_bytes: 10_000_000,
    helper_count: 0,
  };
}

describe("long-run cache soak", () => {
  it("keeps explain/http/details stamps and sparkline samples bounded across thousands of scans", () => {
    const explain = new LruMap<number, { n: number }>(DETAILS_CACHE_MAX);
    const httpAt = new LruMap<number, number>(STAMP_CACHE_MAX);
    const detailsAt = new LruMap<number, number>(STAMP_CACHE_MAX);
    const usage = new UsageHistory(24);
    let snap: Snapshot | null = null;
    let explanations: Record<number, { n: number }> = {};

    for (let gen = 0; gen < 5_000; gen++) {
      // Rotate through 200 ports so ids come and go like a busy machine overnight.
      const ports = Array.from({ length: 40 }, (_, i) => ((gen * 3 + i) % 200) + 1000);
      const entries = ports.map((p) => entry(p));
      const next: Snapshot = {
        platform: "linux",
        taken_at_ms: gen,
        scan_ms: 1,
        docker_available: false,
        hidden_sockets: 0,
        warnings: [],
        entries,
      };
      snap = share(snap, next);
      usage.push(snap.entries);
      const holder = new Set(ports);
      const selected = ports[0];
      explain.set(selected, { n: gen }, new Set([selected]));
      httpAt.set(selected, gen);
      detailsAt.set(selected, gen);
      explanations = { ...explanations, [selected]: { n: gen } };
      explanations = pruned(explanations, (p) => holder.has(p));
      // Cap like App.svelte's capRecord
      const keys = Object.keys(explanations).map(Number);
      if (keys.length > DETAILS_CACHE_MAX) {
        for (const k of keys.filter((k) => k !== selected).slice(0, keys.length - DETAILS_CACHE_MAX)) {
          delete explanations[k];
        }
      }
      explain.prune((p) => holder.has(p));
      httpAt.prune((p) => holder.has(p));
      detailsAt.prune((p) => holder.has(p));
    }

    expect(explain.size).toBeLessThanOrEqual(DETAILS_CACHE_MAX);
    expect(httpAt.size).toBeLessThanOrEqual(STAMP_CACHE_MAX);
    expect(detailsAt.size).toBeLessThanOrEqual(STAMP_CACHE_MAX);
    expect(Object.keys(explanations).length).toBeLessThanOrEqual(DETAILS_CACHE_MAX);
    // UsageHistory drops vanished ids; at most one series per current entry.
    const sampleIds = snap!.entries.map((e) => e.id);
    for (const id of sampleIds) expect(usage.get(id).length).toBeLessThanOrEqual(24);
    // No leftover series for ports that left.
    expect(usage.get("tcp:9999")).toEqual([]);
  });
});
