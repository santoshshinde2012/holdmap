import type { AgentsReport } from "./types";

/** Coalesce overlapping polls and publish only reports at least as new as the current one. */
export function createAgentReportLoader(
  fetchReport: () => Promise<AgentsReport>,
  current: () => AgentsReport | null,
  publish: (report: AgentsReport) => void,
): () => Promise<void> {
  let pending: Promise<void> | null = null;
  return () => {
    if (pending) return pending;
    pending = fetchReport().then((report) => {
      if (report.taken_at_ms >= (current()?.taken_at_ms ?? 0)) publish(report);
    }).finally(() => { pending = null; });
    return pending;
  };
}
