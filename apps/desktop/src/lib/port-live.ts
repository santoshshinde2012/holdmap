import type { PortEntry } from "./types";

/** All listeners on a numeric port, including PID reuse and container replacement. */
export function portHolderKeys(entries: readonly PortEntry[]): Map<number, string> {
  const byPort = new Map<number, string[]>();
  for (const entry of entries) {
    const holders = byPort.get(entry.port) ?? [];
    holders.push(JSON.stringify([entry.id, entry.pid, [...entry.pids].sort((a, b) => a - b), entry.process?.start_token ?? null, entry.process?.start_time ?? null, entry.container?.id ?? null]));
    byPort.set(entry.port, holders);
  }
  return new Map([...byPort].map(([port, holders]) => [port, holders.sort().join("|")]));
}
