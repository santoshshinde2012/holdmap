// Which Security view is showing. Shared so the run dock's "View results" can switch to it and
// so the choice survives remounts (the Agents layout swaps when the window narrows).
export type SecurityTab = "findings" | "results" | "analyzers" | "graph";
export const securityTab = $state<{ value: SecurityTab | null; pipeline: string | null }>({ value: null, pipeline: null });
