// Reactive holder for the selected agent. App creates one and injects it into the views; the
// rules live in agent-selection.ts so they stay unit-testable without Svelte.
import { step } from "./agents";
import { remember, resolveSelection, type SelectionMemory } from "./agent-selection";
import type { Agent } from "./types";

export class AgentSelection {
  /** The selected agent's current id, or null. */
  id = $state<string | null>(null);
  #memory: SelectionMemory | null = null;

  /** Select an agent (null clears). Pass the agent when known so a pid change can be followed. */
  select(id: string | null, agents: readonly Agent[] = []): void {
    this.id = id;
    const a = id ? agents.find((x) => x.id === id) : undefined;
    this.#memory = a ? remember(a) : id ? { id, identity: "", command: "" } : null;
  }

  clear(): void { this.select(null); }

  /** ↑/↓ through the visible agents. */
  step(ids: string[], delta: number, agents: readonly Agent[] = []): void {
    this.select(step(ids, this.id, delta), agents);
  }

  /** Follow the selection into a fresh report: same id, or the same agent under a new pid. */
  reconcile(agents: readonly Agent[]): void {
    if (!this.#memory) return;
    const found = resolveSelection(this.#memory, agents) as Agent | null;
    if (!found) { this.id = null; this.#memory = null; return; }
    if (found.id !== this.id) this.id = found.id;
    this.#memory = remember(found);
  }
}

export function createAgentSelection(): AgentSelection {
  return new AgentSelection();
}
