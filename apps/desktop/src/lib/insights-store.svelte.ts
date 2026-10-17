// Shared analyzer state for every surface (Settings, Home, Agents). Reads are cheap; runs only
// happen for approved analyzers, on Run now or when their inputs change (throttled).
import * as api from "./api";
import type { AnalyzerView, Insights } from "./analyzers";

/** Least time between automatic run-on-change checks. */
export const CHANGE_CHECK_MS = 5 * 60_000;

export class InsightsStore {
  insights = $state<Insights | null>(null);
  analyzers = $state<AnalyzerView[]>([]);
  error = $state<string | null>(null);
  private lastCheck = -Infinity;
  private checking = false;

  constructor(private readonly source: Pick<typeof api, "analyzers" | "insights" | "runChangedAnalyzers"> = api) {}

  async refresh(): Promise<void> {
    try {
      const [analyzers, insights] = await Promise.all([this.source.analyzers(), this.source.insights()]);
      this.analyzers = analyzers;
      this.insights = insights;
      this.error = null;
    } catch (e) {
      this.error = String(e);
    }
  }

  /** After a fresh agents scan: re-run approved analyzers whose subjects changed. */
  async afterAgents(now = Date.now()): Promise<string[]> {
    if (this.checking || now - this.lastCheck < CHANGE_CHECK_MS) return [];
    if (!this.analyzers.some((a) => a.state === "approved")) return [];
    this.checking = true;
    this.lastCheck = now;
    try {
      const ran = await this.source.runChangedAnalyzers();
      if (ran.length) await this.refresh();
      return ran;
    } catch {
      return [];
    } finally {
      this.checking = false;
    }
  }
}

export const insightsStore = new InsightsStore();
