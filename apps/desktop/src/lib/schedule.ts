// Scheduled rescans: types mirroring `holdmap_analyzers::schedule` and the wording the
// Settings section, confirm dialog and run dock share. The rules (eligibility, confirmation,
// battery, quiet hours, throttling) live in Rust; the browser demo keeps a small mirror.
export interface QuietHours { start: number; end: number }
export interface Schedule {
  enabled: boolean;
  every_hours: number;
  analyzers: string[];
  mcp_opt_in: string[];
  skip_on_battery: boolean;
  notify: boolean;
  quiet: QuietHours;
  confirmed_ms?: number | null;
  last_run_ms?: number | null;
  last_notified_ms?: number | null;
}
export interface PlannedAnalyzer { id: string; name: string; starts_mcp_server: boolean; blocked?: string | null }
export interface SchedulePlan { every_hours: number; analyzers: PlannedAnalyzer[]; network: string; runs_when: string; notifies: string }
export type PowerSource = "ac" | "battery" | "unknown";
export interface ScheduleStatus { schedule: Schedule; plan: SchedulePlan; next_ms: number | null; power: PowerSource }
export interface ScheduleRequest { enabled: boolean; every_hours: number; analyzers: string[]; mcp_opt_in: string[]; skip_on_battery: boolean; notify: boolean; quiet: QuietHours; confirmed: boolean }
export type Due = { kind: "off" } | { kind: "wait"; next_ms: number } | { kind: "skip"; reason: string } | { kind: "run"; analyzers: string[] };
export interface ScanOutcome { new_high: number; new_critical: number; changed_skills: number }
export interface ScheduleFinished { outcome: ScanOutcome; notice: { title: string; body: string } | null }

export const INTERVALS: { hours: number; label: string }[] = [
  { hours: 6, label: "Every 6 hours" },
  { hours: 12, label: "Every 12 hours" },
  { hours: 24, label: "Daily" },
  { hours: 168, label: "Weekly" },
];
export const DEFAULT_SCHEDULE: Schedule = { enabled: false, every_hours: 24, analyzers: [], mcp_opt_in: [], skip_on_battery: true, notify: true, quiet: { start: 22, end: 7 } };

export const intervalLabel = (h: number) => INTERVALS.find((i) => i.hours === h)?.label ?? `Every ${h} hours`;
export const hourLabel = (h: number) => `${String(h).padStart(2, "0")}:00`;

/** "in 5 h", "in 3 min", "now", for the next scan. */
export function untilLabel(next: number | null, now: number): string {
  if (next == null) return "off";
  const ms = next - now;
  if (ms <= 60_000) return "due now";
  const min = Math.round(ms / 60_000);
  if (min < 60) return `in ${min} min`;
  const h = Math.round(min / 60);
  return h < 48 ? `in ${h} h` : `in ${Math.round(h / 24)} days`;
}

/** Whether a change needs the plan confirmed again (same rule as Rust `schedule::apply`). */
export function widens(saved: Schedule, next: ScheduleRequest): boolean {
  if (!next.enabled) return false;
  return !saved.enabled || next.every_hours < saved.every_hours
    || next.analyzers.some((a) => !saved.analyzers.includes(a))
    || next.mcp_opt_in.some((a) => !saved.mcp_opt_in.includes(a))
    || (saved.skip_on_battery && !next.skip_on_battery);
}

export const requestFrom = (s: Schedule, over: Partial<ScheduleRequest> = {}): ScheduleRequest => ({
  enabled: s.enabled, every_hours: s.every_hours, analyzers: s.analyzers, mcp_opt_in: s.mcp_opt_in, skip_on_battery: s.skip_on_battery, notify: s.notify, quiet: s.quiet, confirmed: false, ...over,
});
