// View model for the selected-agent detail panel: the summary tiles and short access rows. Long
// explanations stay available as a tooltip; the panel shows one plain sentence per topic.
import { LEVEL_LABEL, TOPIC_LABEL, accessFacts, counts } from "./agents";
import { humanBytes } from "./format";
import type { Stat } from "../components/ui/StatStrip.svelte";
import type { AccessLevel, Agent } from "./types";

export interface FactRow { topic: string; label: string; level: AccessLevel; levelText: string; text: string; full: string; inferred: boolean }

/** First sentence, capped, so "Unknown" rows stay one short line. */
export function firstSentence(text: string, max = 90): string {
  const t = text.trim();
  const m = /^(.+?[.!?])(\s|$)/.exec(t);
  const s = m ? m[1] : t;
  return s.length > max ? `${s.slice(0, max - 1).trimEnd()}…` : s;
}

export function factRows(a: Agent): FactRow[] {
  return accessFacts(a).map((f) => ({
    topic: f.topic,
    label: TOPIC_LABEL[f.topic],
    level: f.level,
    levelText: LEVEL_LABEL[f.level],
    text: f.level === "unknown" ? firstSentence(f.summary, 64) : firstSentence(f.summary),
    full: f.summary,
    inferred: f.evidence !== "observed",
  }));
}

export function detailStats(a: Agent): Stat[] {
  const c = counts(a);
  return [
    { id: "folders", label: "Folders", value: c.folders },
    { id: "ports", label: "Ports", value: c.ports },
    { id: "links", label: "Links", value: c.links },
    { id: "procs", label: "Processes", value: c.processes, meta: `${a.cpu_percent.toFixed(a.cpu_percent < 10 ? 1 : 0)}% CPU · ${humanBytes(a.memory_bytes)}` },
  ];
}
