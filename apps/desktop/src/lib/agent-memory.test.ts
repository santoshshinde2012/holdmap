import { describe, expect, it } from "vitest";
import { bytesOrUnknown, filterPlaces, fleetMemory, headlineLabel, memorySummary, observed, placeEntries, placesSummary, processNoun, shareText, trendSparkline, trendView } from "./agent-memory";
import { mockAgents } from "./mock-agents";
import * as mockPlaces from "./mock-places";
import type { Agent, AgentMemoryReport, CapabilityPlace } from "./types";

const unknown = { value: null, unknown: "smaps_rollup was unreadable" };

describe("memory figures", () => {
  it("does not turn an unknown counter into zero", () => {
    expect(observed(unknown)).toBeNull();
    expect(bytesOrUnknown(unknown)).toContain("unknown");
    expect(bytesOrUnknown(unknown)).not.toMatch(/^0/);
    expect(observed({ value: 5, unknown: "both" })).toBeNull();
    expect(shareText({ value: 50_000, unknown: null })).toBe("5.0% of machine RAM");
  });

  it("exports the headline and the unknown reason without inventing a peak", () => {
    const agent = { name: "Cursor", memory_bytes: 100 } as Agent;
    const memory = {
      headline_bytes: 100,
      headline_meaning: "Sum of Linux VmRSS samples.",
      share_ppm: unknown,
      resident: { value: 100, unknown: null },
      physical_footprint: unknown,
      private_bytes: unknown,
      shared_bytes: unknown,
      proportional: unknown,
      compressed: unknown,
      virtual_bytes: unknown,
      swap_bytes: unknown,
      commit_bytes: unknown,
      peak_resident: { value: null, unknown: "peaks are not simultaneous" },
      peak_physical: unknown,
      groups: [{ role: "agent", processes: 1, scan_bytes: 100 }],
      processes: [],
      trend: { samples: [{ at_ms: 1, bytes: 100 }], direction: "unknown", unknown: "only one sample", rule: "band" },
      limits: ["not copied from the headline"],
    } as unknown as AgentMemoryReport;
    const text = memorySummary(agent, memory);
    expect(text).toContain("100 B");
    expect(text).toContain("smaps_rollup");
    expect(text).toContain("not simultaneous");
    expect(text).not.toContain("Peak resident: 0");
  });

  it("computes sparkline points and time span only from retained samples", () => {
    expect(trendSparkline([{ at_ms: 1000, bytes: 100 }])).toBeNull();
    const spark = trendSparkline([
      { at_ms: 0, bytes: 100 },
      { at_ms: 300_000, bytes: 200 },
      { at_ms: 600_000, bytes: 300 },
    ]);
    expect(spark?.points).toBe("2,15 36,9 70,3");
    const tv = trendView({
      samples: [
        { at_ms: 0, bytes: 1024 * 1024 },
        { at_ms: 600_000, bytes: 2 * 1024 * 1024 },
      ],
      direction: "rising",
      unknown: null,
      rule: "10% band",
    });
    expect(tv.sampleCount).toBe(2);
    expect(tv.span).toBe("10m");
    expect(tv.delta).toBe("+1.0 MB");
  });

  it("lists location metadata and not a file body", () => {
    const edited = Date.UTC(2026, 10, 13, 8, 0, 0);
    const places: CapabilityPlace[] = [
      {
        id: "claude-code:skills:user:.claude/skills",
        kind: "skills",
        scope: "user",
        rank: 30,
        path: "/home/dev/.claude/skills",
        status: "present",
        file_count: 1,
        byte_size: 40,
        candidates: ["review"],
        entries: [{ name: "review", kind: "directory", bytes: null, modified_unix_ms: edited }],
        present: true,
        symlink_target: null,
      },
      {
        id: "claude-code:rules:local",
        kind: "rules",
        scope: "local",
        rank: 10,
        path: "/home/dev/app/.claude/settings.local.json",
        status: "missing",
        candidates: [],
        present: false,
        symlink_target: null,
      },
      {
        id: "claude-code:builtin",
        kind: "builtin",
        scope: "builtin",
        rank: 50,
        path: "",
        status: "present",
        candidates: ["Read", "Edit"],
        present: true,
        symlink_target: null,
      },
    ];
    const text = placesSummary(places);
    expect(text).toContain("/home/dev/.claude/skills");
    expect(text).toContain("review");
    expect(text).toContain("not proof");
    expect(text).toContain("modified time unknown");
    expect(text).toContain("not on disk");
    expect(text).toContain("2 documented tools");
    expect(text).not.toContain("rank");
    const entries = placeEntries(places[0]);
    expect(entries[0]).toMatchObject({ name: "review", kindLabel: "dir", bytes: null });
    expect(entries[0].modified).toBeTruthy();
  });

  it("sums agent headlines and names the largest without inventing a machine share", () => {
    const report = mockAgents();
    const fleet = fleetMemory(report);
    expect(fleet?.fromProcessTable).toBe(false);
    expect(fleet?.share).toMatch(/2\d\.\d% of machine RAM/);
    expect(fleet?.top?.name).toBe("Cursor");
    expect(processNoun(1)).toBe("1 process");
    expect(processNoun(2)).toBe("2 processes");
    const places = mockAgents(undefined, true, mockPlaces).agents.find((agent) => agent.product === "cursor")!.capabilities!.locations!;
    expect(filterPlaces(places, "present").every((place) => place.status === "present")).toBe(true);
    expect(filterPlaces(places, "skills").every((place) => place.kind === "skills")).toBe(true);
    expect(placesSummary(places)).not.toContain("sk-");
  });

  it("shows how layers combine, memory index lines and ignored rule files", () => {
    const base = { candidates: [], present: true, symlink_target: null, status: "present" as const, modified_unix_ms: null };
    const text = placesSummary([
      { ...base, id: "a", kind: "agent_memory", scope: "user", rank: 30, label: "Auto memory", combine: "join", path: "/u/.claude/projects/-w/memory", file_count: 2, byte_size: 10, line_count: 236, load_limit_lines: 200 },
      { ...base, id: "b", kind: "rules", scope: "project", rank: 20, combine: "join", path: "/w/.cursor/rules", file_count: 1, byte_size: 5, ignored_count: 2, candidates: ["style"] },
      { ...base, id: "c", kind: "secret_store", scope: "user", rank: 30, combine: "not_config", path: "/u/.codex/auth.json" },
      { ...base, id: "d", kind: "session_data", scope: "user", rank: 30, combine: "not_config", path: "/u/.claude/projects", file_count: 9, byte_size: 900 },
    ]);
    expect(text).toContain("User Agent memory (all files load)");
    expect(text).toContain("MEMORY.md 236 lines, only the first 200 load");
    expect(text).toContain("2 ignored");
    expect(text).toContain("/u/.codex/auth.json · present · not opened");
    expect(text).toContain("User Session data (not configuration)");
  });

  it("names the headline counter and says when it is mixed", () => {
    const memory = { headline_counter: "physical_footprint", headline_label: "", headline_mixed: null } as unknown as AgentMemoryReport;
    expect(headlineLabel(memory)).toBe("Physical footprint");
    expect(headlineLabel({ ...memory, headline_label: "Mixed: physical footprint and resident (rss)" })).toMatch(/^Mixed/);
  });
});
