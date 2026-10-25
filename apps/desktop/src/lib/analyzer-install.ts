// In-app installs of an analyzer's program. The desktop app runs only the recipes compiled
// into holdmap (crates/holdmap-analyzers/src/install.rs): an exact, pinned package installed by
// a package manager the user already has, into holdmap's own folder, with a cleared environment
// and a time limit. Nothing installs until the user agrees in the install dialog, and the
// analyzer stays off until they approve it. This file holds the types, the dialog's state
// machine (pure, unit-tested) and the browser demo's simulation.

export type InstallMethod = "uv_tool" | "go_install" | "manual";
export interface Toolchain { name: string; path: string | null; help_url: string }
export interface InstallRecord { analyzer: string; version: string; binary: string; sha256: string; installed_at_ms: number; method: InstallMethod; package: string }

/** Mirrors `holdmap_analyzers::install::InstallPlan`. Built without running anything. */
export interface InstallPlan {
  analyzer: string;
  name: string;
  method: InstallMethod;
  package: string;
  version: string;
  source: string;
  verification: string;
  target_dir: string;
  network: string[];
  reads: string[];
  run_network: string[];
  toolchain: Toolchain | null;
  command: string[];
  fallback: string | null;
  safest: string | null;
  help_url: string;
  can_install: boolean;
  blocked: string | null;
  installed: InstallRecord | null;
}

export type InstallEvent =
  | { kind: "started"; command: string[] }
  | { kind: "log"; line: string }
  | { kind: "verifying" }
  | { kind: "done"; record: InstallRecord }
  | { kind: "failed"; reason: string }
  | { kind: "cancelled" };

// ---- Dialog state ----

export type InstallPhase = "review" | "installing" | "verifying" | "done" | "failed" | "cancelled";
export interface InstallState {
  phase: InstallPhase;
  log: string[];
  record: InstallRecord | null;
  reason: string | null;
  startedAt: number | null;
}

export const MAX_LOG = 400;

export function initialInstall(): InstallState {
  return { phase: "review", log: [], record: null, reason: null, startedAt: null };
}

/** Fold one install update into the dialog's state. Once finished, later updates are ignored. */
export function reduceInstall(s: InstallState, e: InstallEvent, now = Date.now()): InstallState {
  if (s.phase === "done" || s.phase === "failed" || s.phase === "cancelled") return s;
  switch (e.kind) {
    case "started": return { ...s, phase: "installing", startedAt: s.startedAt ?? now, log: [`$ ${e.command.join(" ")}`] };
    case "log": return s.log.length >= MAX_LOG ? s : { ...s, log: [...s.log, e.line] };
    case "verifying": return { ...s, phase: "verifying" };
    case "done": return { ...s, phase: "done", record: e.record };
    case "failed": return { ...s, phase: "failed", reason: e.reason };
    case "cancelled": return { ...s, phase: "cancelled", reason: "You cancelled the install. Nothing was changed outside holdmap's folder." };
  }
}

/** Start or restart (Retry) an install: clear the log and reason. */
export function beginInstall(now = Date.now()): InstallState {
  return { phase: "installing", log: [], record: null, reason: null, startedAt: now };
}

export const PHASE_LABEL: Record<InstallPhase, string> = {
  review: "Review",
  installing: "Installing",
  verifying: "Checking the version",
  done: "Installed",
  failed: "Install failed",
  cancelled: "Cancelled",
};

export const METHOD_LABEL: Record<InstallMethod, string> = {
  uv_tool: "uv (Python tools)",
  go_install: "Go",
  manual: "Install it yourself",
};
