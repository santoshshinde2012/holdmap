import type { PortEntry } from "./types";

/** Local, session-only schedules. Native actions are authorized and executed in Rust. */
export type ScheduleKind = "shutdown" | "port" | "agent";
export type ScheduleStatus = "pending" | "checking" | "running" | "completed" | "cancelled" | "needs_review" | "failed";

export interface ScheduledAction {
  id: string;
  kind: ScheduleKind;
  label: string;
  targets: string[];
  created_at_ms: number;
  due_at_ms: number;
  status: ScheduleStatus;
  message: string | null;
  cancellable: boolean;
}

export interface ScheduledStopRequest {
  target: string;
  confirmation_id: string;
  force: boolean;
  allow_protected: boolean;
}

export const SCHEDULE_PRESETS = [
  { value: 0, label: "Now" },
  { value: 900, label: "15 min" },
  { value: 1800, label: "30 min" },
  { value: 3600, label: "1 hour" },
  { value: 7200, label: "2 hours" },
] as const;

export function validateScheduleDelay(delaySecs: number): void {
  if (!Number.isSafeInteger(delaySecs) || delaySecs < 60 || delaySecs > 86_400) {
    throw new Error("Choose a time between 1 minute and 24 hours from now.");
  }
}

/** Resolve an absolute selection at the explicit confirmation, rather than reusing an old delay. */
export function resolveScheduleDelay(delaySecs: number, atMs: number | null = null, now = Date.now()): number {
  const resolved = atMs === null ? delaySecs : Math.ceil((atMs - now) / 1000);
  validateScheduleDelay(resolved);
  return resolved;
}

export function validateScheduledStops(requests: ScheduledStopRequest[]): void {
  if (!requests.length || requests.length > 32) throw new Error("Review between 1 and 32 local ports to schedule.");
  const targets = new Set<string>();
  const tokens = new Set<string>();
  for (const request of requests) {
    if (!/^\d{1,5}\/(tcp|udp)$/.test(request.target) || Number(request.target.split("/")[0]) < 1 || Number(request.target.split("/")[0]) > 65_535) {
      throw new Error("Schedules require a specific local port and protocol.");
    }
    if (!request.confirmation_id?.trim()) throw new Error("Review each stop plan before scheduling.");
    if (request.force || request.allow_protected) throw new Error("Force-stop requests and protected-process overrides cannot be scheduled.");
    if (targets.has(request.target) || tokens.has(request.confirmation_id)) throw new Error("Each scheduled port needs its own reviewed plan.");
    targets.add(request.target);
    tokens.add(request.confirmation_id);
  }
}

export function activeScheduleCount(schedules: ScheduledAction[]): number {
  return schedules.filter((schedule) => ["pending", "checking", "running"].includes(schedule.status)).length;
}

/** Local, unprotected listeners a schedule review can target. The review still authorizes the stop. */
export function schedulableEntries(entries: PortEntry[]): PortEntry[] {
  return entries
    .filter((entry) => !entry.remote && !entry.protected && !!(entry.process || entry.container))
    .sort((a, b) => Number(!!b.is_dev) - Number(!!a.is_dev) || a.port - b.port);
}

export const SCHEDULE_STATUS: Record<ScheduleStatus, string> = {
  pending: "Scheduled", checking: "Checking safety", running: "Running", completed: "Completed",
  cancelled: "Cancelled", needs_review: "Needs review", failed: "Failed",
};

export function scheduleDeadline(dueAtMs: number): string {
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit", second: "2-digit", timeZoneName: "short" }).format(dueAtMs);
}

export function scheduleCountdown(dueAtMs: number, now: number): string {
  const secs = Math.max(0, Math.ceil((dueAtMs - now) / 1000));
  if (!secs) return "Due now";
  if (secs >= 3600) return `In ${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
  if (secs >= 60) return `In ${Math.floor(secs / 60)}m ${secs % 60}s`;
  return `In ${secs}s`;
}

/** Pure browser simulation: no native imports, process access, or operating-system effects. */
export class DemoSchedules {
  private actions: ScheduledAction[] = [];
  private sequence = 0;
  private lastWall: number;
  private lastMono: number;
  private listeners = new Set<(actions: ScheduledAction[]) => void>();

  constructor(private wall: () => number = () => Date.now(), private monotonic: () => number = () => performance.now()) {
    this.lastWall = wall();
    this.lastMono = monotonic();
  }

  list(): ScheduledAction[] { return this.actions.map((action) => ({ ...action, targets: [...action.targets] })); }

  subscribe(listener: (actions: ScheduledAction[]) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  create(kind: ScheduleKind, label: string, targets: string[], delaySecs: number): ScheduledAction {
    validateScheduleDelay(delaySecs);
    this.tick();
    if (activeScheduleCount(this.actions) >= 32) throw new Error("Cancel a pending action before adding another schedule.");
    if (kind === "shutdown" && this.actions.some((action) => action.kind === "shutdown" && action.status === "pending")) {
      throw new Error("A shutdown is already scheduled. Cancel it before scheduling another.");
    }
    const now = this.wall();
    const action: ScheduledAction = {
      id: `demo-schedule-${++this.sequence}`, kind, label, targets: [...targets], created_at_ms: now,
      due_at_ms: now + delaySecs * 1000, status: "pending", message: null, cancellable: true,
    };
    this.actions.unshift(action);
    this.pruneHistory();
    this.emit();
    return { ...action, targets: [...action.targets] };
  }

  cancel(id: string): ScheduledAction {
    // Reconcile due times before acknowledging cancellation; never claim a running action was cancelled.
    this.tick();
    const action = this.actions.find((item) => item.id === id);
    if (!action || !action.cancellable || action.status !== "pending") throw new Error("This action can no longer be cancelled.");
    action.status = "cancelled";
    action.cancellable = false;
    action.message = "Cancelled before execution. No action was taken.";
    this.pruneHistory();
    this.emit();
    return { ...action, targets: [...action.targets] };
  }

  tick(): void {
    const now = this.wall();
    const mono = this.monotonic();
    const clockChanged = Math.abs((now - this.lastWall) - (mono - this.lastMono)) > 30_000;
    this.lastWall = now;
    this.lastMono = mono;
    let changed = false;
    for (const action of this.actions) {
      if (action.status !== "pending") continue;
      if (clockChanged || now - action.due_at_ms > 60_000) {
        action.status = "needs_review";
        action.message = clockChanged ? "The clock changed. Review and create a new schedule; nothing ran." : "The deadline was missed. Review and create a new schedule; nothing ran.";
        action.cancellable = false;
        changed = true;
      } else if (now >= action.due_at_ms) {
        action.status = "completed";
        action.message = "Simulation complete. Your computer, agents and ports were not changed.";
        action.cancellable = false;
        changed = true;
      }
    }
    if (changed) { this.pruneHistory(); this.emit(); }
  }

  private pruneHistory(): void {
    let finished = 0;
    this.actions = this.actions.filter((action) => ["pending", "checking", "running"].includes(action.status) || ++finished <= 64);
  }

  private emit(): void { for (const listener of this.listeners) listener(this.list()); }
}
