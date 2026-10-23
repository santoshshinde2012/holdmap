// The one selected agent shared by every Agents surface: the card rail, the Computer and
// Footprint maps, the Home radial map, ↑/↓, search and deep links. Agent ids are `agent:<pid>`,
// so a restarted agent gets a new id; the store keeps a pid-free identity and re-finds the
// same agent after a refresh instead of dropping the selection.
import type { Agent } from "./types";

type Identity = Pick<Agent, "id" | "product" | "kind" | "name" | "command" | "parent">;

/** Pid-free identity: same product, kind and name. */
export function agentIdentity(a: Pick<Agent, "product" | "kind" | "name">): string {
  return `${a.product}|${a.kind}|${a.name}`;
}

export interface SelectionMemory {
  id: string;
  identity: string;
  command: string;
}

export function remember(a: Identity): SelectionMemory {
  return { id: a.id, identity: agentIdentity(a), command: a.command };
}

/**
 * The agent a remembered selection points at in a new report: the same id, else the only agent
 * with the same identity, else the one whose command line also matches. Null when gone or ambiguous.
 */
export function resolveSelection(memory: SelectionMemory | null, agents: readonly Identity[]): Identity | null {
  if (!memory) return null;
  const exact = agents.find((a) => a.id === memory.id);
  if (exact) return exact;
  const same = agents.filter((a) => agentIdentity(a) === memory.identity);
  if (same.length === 1) return same[0];
  const command = same.filter((a) => a.command === memory.command);
  return command.length === 1 ? command[0] : null;
}

const HASH = /^#agent=(.+)$/;

/** `#agent=<id or name>` from a deep link. */
export function parseAgentHash(hash: string): string | null {
  const m = HASH.exec(hash);
  if (!m) return null;
  try { return decodeURIComponent(m[1]); } catch { return null; }
}

/** The agent a deep link names: an exact id, else a case-insensitive name. */
export function findLinkedAgent<T extends Pick<Agent, "id" | "name">>(link: string, agents: readonly T[]): T | null {
  const low = link.toLowerCase();
  return agents.find((a) => a.id === link) ?? agents.find((a) => a.name.toLowerCase() === low) ?? null;
}

export function agentHash(id: string | null): string {
  return id ? `#agent=${encodeURIComponent(id)}` : "";
}
