/**
 * The analyzer graphs pull in Svelte Flow, so they load only when someone opens
 * "How it works". One shared promise; a failed load can be retried.
 */
export type AnalyzerGraphs = typeof import("./analyzer-graphs");

let pending: Promise<AnalyzerGraphs> | null = null;

export function loadAnalyzerGraphs(): Promise<AnalyzerGraphs> {
  pending ??= import("./analyzer-graphs").catch((e) => {
    pending = null;
    throw e;
  });
  return pending;
}
