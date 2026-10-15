import type { AgentsReport } from "./types";

/** Coalesce polls; a navigation change must never publish an obsolete inspection scope. */
export function createAgentReportLoader(
  fetchReport: (includeCapabilities?: boolean) => Promise<AgentsReport>,
  current: () => AgentsReport | null,
  publish: (report: AgentsReport) => void,
  includeCapabilities: () => boolean = () => false,
): () => Promise<void> {
  let pending: Promise<void> | null = null;
  return () => {
    if (pending) return pending;
    pending = (async () => {
      for (;;) {
        const inspection = includeCapabilities();
        let report: AgentsReport;
        try { report = await fetchReport(inspection); }
        catch (error) {
          if (inspection !== includeCapabilities()) continue;
          throw error;
        }
        if (inspection !== includeCapabilities()) continue;
        if (report.taken_at_ms >= (current()?.taken_at_ms ?? 0)) publish(report);
        return;
      }
    })().finally(() => { pending = null; });
    return pending;
  };
}
