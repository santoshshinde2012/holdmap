// Skill trust: types mirroring `holdmap_analyzers::trust`. File contents never appear anywhere:
// only names, sizes, hashes and times.
import type { Subject } from "./analyzers";

export type TrustState = "changed" | "new" | "unreadable" | "trusted";
export interface FileChange { path: string; size_before?: number | null; size_after?: number | null }
export interface ChangeSummary { added: FileChange[]; removed: FileChange[]; modified: FileChange[]; added_total: number; removed_total: number; modified_total: number }
export interface SkillTrustView {
  subject: Subject;
  state: TrustState;
  sha256?: string | null;
  file_count: number;
  total_bytes: number;
  modified_ms?: number | null;
  partial: boolean;
  first_seen_ms: number;
  seen_ms: number;
  changed_since_last_scan: boolean;
  trusted_ms?: number | null;
  reason?: string | null;
  changes?: ChangeSummary | null;
  error?: string | null;
}
export interface TrustReport { skills: SkillTrustView[]; changed: number; untrusted: number; scanned_ms?: number | null }

export const TRUST_LABEL: Record<TrustState, string> = { changed: "Changed since trusted", new: "Not trusted yet", unreadable: "Couldn't read", trusted: "Trusted" };
export const alerts = (s: TrustState) => s === "changed" || s === "new";

export function bytes(n: number | null | undefined): string {
  if (n == null) return "";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
