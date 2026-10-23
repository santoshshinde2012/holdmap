import { describe, expect, it } from "vitest";
import { MAX_DURATION_MS } from "./screen-record";
import {
  DEFAULT_PLAN, DISK_MP4_LIMIT_BYTES, LEGACY_PLAN, MEMORY_MP4_LIMIT_BYTES, PRESETS, clampPlan, estimateLines, estimatePlan, formatChoice, formatLength,
  planWantsGif, planWantsMp4, withDuration, withFormat, withPreset,
} from "./screen-record-plan";

const MIB = 1024 * 1024;

describe("recording presets", () => {
  it("shares a short GIF clip and records a long MP4 session", () => {
    expect(PRESETS.clip).toMatchObject({ formats: ["gif"], maxDurationMs: 15_000, gifMaxBytes: 10 * MIB });
    expect(PRESETS.session).toMatchObject({ formats: ["mp4"], maxDurationMs: 30 * 60_000 });
    expect(DEFAULT_PLAN).toBe(PRESETS.session);
    expect(planWantsMp4(PRESETS.clip)).toBe(false);
    expect(planWantsGif(PRESETS.clip)).toBe(true);
    expect(formatChoice(LEGACY_PLAN)).toBe("both");
  });

  it("leaves a preset behind as soon as a detail changes", () => {
    expect(withFormat(PRESETS.clip, "both")).toMatchObject({ preset: "custom", formats: ["mp4", "gif"], maxDurationMs: 15_000 });
    expect(withDuration(PRESETS.session, 60_000)).toMatchObject({ preset: "custom", formats: ["mp4"], maxDurationMs: 60_000 });
    expect(withPreset("clip", PRESETS.session)).toEqual(PRESETS.clip);
    expect(withPreset("custom", PRESETS.clip)).toMatchObject({ preset: "custom", maxDurationMs: 15_000 });
    expect(withPreset("clip", PRESETS.session)).not.toBe(PRESETS.clip);
  });

  it("clamps hostile or broken plans into supported limits", () => {
    const plan = clampPlan({ ...PRESETS.clip, formats: [], maxDurationMs: 10 * MAX_DURATION_MS, gifFps: 99, gifMaxEdge: 1, gifMaxBytes: 1e12, mp4Bitrate: Number.NaN });
    expect(plan).toMatchObject({ formats: ["mp4"], maxDurationMs: MAX_DURATION_MS, gifFps: 15, gifMaxEdge: 160, gifMaxBytes: 128 * MIB, mp4Bitrate: 3_000_000 });
    expect(clampPlan({ ...PRESETS.session, maxDurationMs: 0 }).maxDurationMs).toBe(1_000);
  });
});

describe("recording estimates", () => {
  it("runs a 30 minute desktop MP4 to the full length within the disk limit", () => {
    const estimate = estimatePlan(PRESETS.session, "native");
    expect(estimate.cutShort).toBe(false);
    expect(estimate.mp4!.maxBytes).toBe(Math.round(3_000_000 / 8 * 1800));
    expect(estimate.mp4!.maxBytes).toBeLessThan(DISK_MP4_LIMIT_BYTES);
    expect(estimate.mp4!.typicalBytes).toBeLessThan(estimate.mp4!.maxBytes / 4);
    expect(estimate.gif).toBeNull();
  });

  it("warns that a browser keeps the MP4 in memory and can stop sooner", () => {
    const estimate = estimatePlan(PRESETS.session, "browser");
    expect(estimate.cutShort).toBe(true);
    expect(estimate.mp4!.maxBytes).toBe(MEMORY_MP4_LIMIT_BYTES);
    expect(estimate.maxDurationMs).toBeGreaterThan(2.5 * 60_000);
    expect(estimate.maxDurationMs).toBeLessThan(4 * 60_000);
    expect(estimateLines(PRESETS.session, "browser", (n) => `${n} B`).join(" ")).toMatch(/in memory/);
  });

  it("states the GIF budget and never claims an MP4 for a clip", () => {
    const lines = estimateLines(PRESETS.clip, "native", (n) => `${Math.round(n / MIB)} MB`);
    expect(lines[0]).toMatch(/^GIF: at most 10 MB, up to 10 frames per second/);
    expect(lines.join(" ")).not.toMatch(/MP4/);
    expect(lines.at(-1)).toBe("Stops by itself after 15 s.");
  });

  it("names lengths in plain units", () => {
    expect(formatLength(15_000)).toBe("15 s");
    expect(formatLength(300_000)).toBe("5 min");
    expect(formatLength(3_600_000)).toBe("1 hour");
  });
});
