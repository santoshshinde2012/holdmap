// Snooze lengths: presets plus a custom date, never past 90 days (the backend clamps too).
export const MIN_SNOOZE_MS = 3_600_000;
export const MAX_SNOOZE_DAYS = 90;
export const MAX_SNOOZE_MS = MAX_SNOOZE_DAYS * 86_400_000;
export const SNOOZE_PRESETS = [
  { days: 1, label: "1 day" },
  { days: 7, label: "7 days" },
  { days: 30, label: "30 days" },
] as const;

/** A snooze end kept between 1 hour and 90 days from `now`. */
export const clampSnooze = (until: number, now: number) => Math.min(Math.max(until, now + MIN_SNOOZE_MS), now + MAX_SNOOZE_MS);

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
/** The `min` and `max` for the custom date picker: tomorrow to 90 days ahead (local dates). */
export function dateBounds(now: number): { min: string; max: string } {
  const d = new Date(now);
  const min = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1);
  const max = new Date(d.getFullYear(), d.getMonth(), d.getDate() + MAX_SNOOZE_DAYS);
  return { min: iso(min), max: iso(max) };
}
/** End of a picked local date (23:59:59.999), or null if it's outside the bounds. */
export function untilFromDate(value: string, now: number): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!m) return null;
  const end = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), 23, 59, 59, 999);
  if (end.getMonth() !== Number(m[2]) - 1) return null;
  const { min, max } = dateBounds(now);
  return value < min || value > max ? null : end.getTime();
}
/** "until Fri, 30 Oct" for a snoozed finding. */
export const untilText = (ms: number) => `until ${new Date(ms).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })}`;
