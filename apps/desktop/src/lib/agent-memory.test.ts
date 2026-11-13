import { describe, expect, it } from "vitest";
import { bytesOrUnknown, memorySummary, observed, placesSummary, shareText } from "./agent-memory";
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

  it("lists location metadata and not a file body", () => {
    const places: CapabilityPlace[] = [{
      id: "claude-code:skills:user:.claude/skills",
      kind: "skills",
      scope: "user",
      rank: 30,
      path: "/home/dev/.claude/skills",
      status: "present",
      file_count: 1,
      byte_size: 40,
      candidates: ["review"],
      present: true,
      symlink_target: null,
    }];
    const text = placesSummary(places);
    expect(text).toContain("/home/dev/.claude/skills");
    expect(text).toContain("review");
    expect(text).toContain("not proof");
  });
});
