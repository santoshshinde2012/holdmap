/** What to record and how much: presets, limits and a size estimate, shared by every backend. */
import { MAX_DURATION_MS } from "./screen-record";

export type RecordingFormat = "mp4" | "gif";
export type RecordingPresetId = "clip" | "session" | "custom";
export type FormatChoice = "mp4" | "gif" | "both";

export type RecordingPlan = {
  preset: RecordingPresetId;
  formats: RecordingFormat[];
  maxDurationMs: number;
  /** Target frame rate and longest edge for the GIF; its size budget bounds the rest. */
  gifFps: number;
  gifMaxEdge: number;
  gifMaxBytes: number;
  mp4Bitrate: number;
};

const MIB = 1024 * 1024;
/** Where finished MP4 bytes live: a browser tab keeps them in memory, the desktop app on disk. */
export const MEMORY_MP4_LIMIT_BYTES = 128 * MIB;
export const DISK_MP4_LIMIT_BYTES = 2 * 1024 * MIB;
/** Rough share of the bitrate ceiling that a mostly still app window really uses (measured 15-20%). */
const TYPICAL_MP4_SHARE = 0.15;

export const DURATION_CHOICES_MS = [15_000, 30_000, 60_000, 120_000, 300_000, 900_000, 1_800_000, 3_600_000] as const;

export const PRESETS: Record<Exclude<RecordingPresetId, "custom">, RecordingPlan> = {
  clip: { preset: "clip", formats: ["gif"], maxDurationMs: 15_000, gifFps: 10, gifMaxEdge: 800, gifMaxBytes: 10 * MIB, mp4Bitrate: 3_000_000 },
  session: { preset: "session", formats: ["mp4"], maxDurationMs: 1_800_000, gifFps: 8, gifMaxEdge: 640, gifMaxBytes: 25 * MIB, mp4Bitrate: 3_000_000 },
};

/** Behaviour of callers that pass no plan: both files, two minutes, as before. */
export const LEGACY_PLAN: RecordingPlan = {
  preset: "custom", formats: ["mp4", "gif"], maxDurationMs: 120_000, gifFps: 8, gifMaxEdge: 640, gifMaxBytes: 25 * MIB, mp4Bitrate: 5_000_000,
};

export const DEFAULT_PLAN: RecordingPlan = PRESETS.session;

export function clampPlan(plan: RecordingPlan): RecordingPlan {
  const formats = (["mp4", "gif"] as const).filter((format) => plan.formats.includes(format));
  return {
    ...plan,
    formats: formats.length ? formats : ["mp4"],
    maxDurationMs: Math.min(MAX_DURATION_MS, Math.max(1_000, Math.round(plan.maxDurationMs) || 1_000)),
    gifFps: Math.min(15, Math.max(1, Math.round(plan.gifFps) || 8)),
    gifMaxEdge: Math.min(960, Math.max(160, Math.round(plan.gifMaxEdge) || 640)),
    gifMaxBytes: Math.min(128 * MIB, Math.max(MIB, Math.round(plan.gifMaxBytes) || 10 * MIB)),
    mp4Bitrate: Math.min(8_000_000, Math.max(500_000, Math.round(plan.mp4Bitrate) || 3_000_000)),
  };
}

export const planWantsMp4 = (plan: RecordingPlan) => plan.formats.includes("mp4");
export const planWantsGif = (plan: RecordingPlan) => plan.formats.includes("gif");

export function formatChoice(plan: RecordingPlan): FormatChoice {
  return planWantsMp4(plan) && planWantsGif(plan) ? "both" : planWantsGif(plan) ? "gif" : "mp4";
}

export function withPreset(id: RecordingPresetId, current: RecordingPlan): RecordingPlan {
  return id === "custom" ? { ...current, preset: "custom" } : { ...PRESETS[id] };
}

/** Changing a detail leaves the preset behind: the plan becomes "custom". */
export function withFormat(plan: RecordingPlan, choice: FormatChoice): RecordingPlan {
  const formats: RecordingFormat[] = choice === "both" ? ["mp4", "gif"] : [choice];
  return { ...plan, preset: "custom", formats };
}

export function withDuration(plan: RecordingPlan, maxDurationMs: number): RecordingPlan {
  return { ...plan, preset: "custom", maxDurationMs };
}

export function formatLength(ms: number): string {
  if (ms < 60_000) return `${Math.round(ms / 1000)} s`;
  const minutes = Math.round(ms / 60_000);
  return minutes === 60 ? "1 hour" : `${minutes} min`;
}

export type PlanEstimate = {
  /** Longest the recording can run: the chosen length, cut short when MP4 would outgrow its storage. */
  maxDurationMs: number;
  mp4: { typicalBytes: number; maxBytes: number } | null;
  gif: { maxBytes: number } | null;
  /** Why a recording stops before the chosen length, or null. */
  cutShort: boolean;
};

export function estimatePlan(plan: RecordingPlan, backend: "native" | "browser" | null): PlanEstimate {
  const limit = backend === "native" ? DISK_MP4_LIMIT_BYTES : MEMORY_MP4_LIMIT_BYTES;
  const bytesPerMs = plan.mp4Bitrate / 8 / 1000;
  // Measured: the browser's software encoder ran about 1.8x over its target on pure noise.
  const worstBytesPerMs = bytesPerMs * (backend === "native" ? 1.2 : 1.8);
  const fitsMs = planWantsMp4(plan) ? Math.floor((limit - 8 * MIB) / worstBytesPerMs) : Number.POSITIVE_INFINITY;
  const maxBytes = Math.min(limit, Math.round(bytesPerMs * plan.maxDurationMs));
  return {
    // The worst case: a screen in constant motion at the bitrate ceiling.
    maxDurationMs: Math.min(plan.maxDurationMs, fitsMs),
    cutShort: fitsMs < plan.maxDurationMs,
    mp4: planWantsMp4(plan) ? { typicalBytes: Math.min(limit, Math.round(bytesPerMs * plan.maxDurationMs * TYPICAL_MP4_SHARE)), maxBytes } : null,
    gif: planWantsGif(plan) ? { maxBytes: plan.gifMaxBytes } : null,
  };
}

export const PRESET_SUMMARY: Record<RecordingPresetId, string> = {
  clip: "A short looping GIF for chats and issues: up to 15 seconds and 10 MB.",
  session: "A full MP4 walkthrough, best for long runs. Pause and resume to skip the boring parts.",
  custom: "Choose the format and length yourself.",
};

/** One plain sentence per file the plan will make, so the size is known before recording. */
export function estimateLines(plan: RecordingPlan, backend: "native" | "browser" | null, humanBytes: (n: number) => string): string[] {
  const estimate = estimatePlan(plan, backend);
  const lines: string[] = [];
  if (estimate.mp4) lines.push(`MP4: usually ${humanBytes(estimate.mp4.typicalBytes)}, at most ${humanBytes(estimate.mp4.maxBytes)}.`);
  if (estimate.gif) lines.push(`GIF: at most ${humanBytes(estimate.gif.maxBytes)}, up to ${plan.gifFps} frames per second. Frames thin out if it would grow bigger.`);
  lines.push(`Stops by itself after ${formatLength(plan.maxDurationMs)}.`);
  if (estimate.cutShort) lines.push(`This window keeps the MP4 in memory and stops it at ${humanBytes(MEMORY_MP4_LIMIT_BYTES)}: a screen in constant motion lasts about ${formatLength(estimate.maxDurationMs)}, a mostly still one the full length.`);
  return lines;
}
