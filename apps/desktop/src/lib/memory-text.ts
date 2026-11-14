// An agent's RAM as one line of text, for cards that skim (Home, Agents). Kept apart from the
// full memory breakdown so the first screen doesn't load it. Unknown stays unknown, never zero.
import { humanBytes } from "./format";
import type { Agent, AgentMemoryReport, AgentsReport, ObservedCount } from "./types";

/** The observed integer, only when a reason was not also recorded. */
export function observed(figure: ObservedCount | undefined): number | null {
  if (!figure || figure.unknown || figure.value == null) return null;
  return figure.value;
}

/** An agent's memory as text: its size, or "unknown" when no process in its tree gave a
 * reading (another account's process), so a zero is never shown as a size. */
export function agentMemoryText(agent: Pick<Agent, "memory_bytes">, memory: AgentMemoryReport | null): string {
  const none = !!memory && memory.processes.length > 0 && memory.processes.every((row) => row.scan_bytes === 0 && observed(row.reading?.resident) === null && observed(row.reading?.physical_footprint) === null);
  return none ? "unknown" : humanBytes(agent.memory_bytes);
}

export function memoryFor(report: AgentsReport | null | undefined, agentId: string): AgentMemoryReport | null {
  return report?.memory?.find((row) => row.agent_id === agentId) ?? null;
}
