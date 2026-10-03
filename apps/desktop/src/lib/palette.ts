// Command palette model: a flat list of commands, ranked by a small fuzzy matcher.

export interface Command {
  id: string;
  title: string;
  subtitle?: string;
  group: "Ports" | "Actions" | "Filters" | "View" | "Settings";
  keywords?: string;
  shortcut?: string[];
  icon?: string;
  danger?: boolean;
  /** Added to the fuzzy score when the command matches (e.g. the selected port, dev servers). */
  boost?: number;
  run: () => void;
}

/** Subsequence fuzzy score: higher is better, -1 means no match. Rewards prefixes, word starts and runs. */
export function fuzzyScore(query: string, text: string): number {
  const q = query.toLowerCase().replace(/\s+/g, "");
  const t = text.toLowerCase();
  if (!q) return 0;
  let score = 0;
  let ti = 0;
  let run = 0;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return -1;
    const wordStart = found === 0 || /[\s:·/\-_.(]/.test(t[found - 1]);
    run = found === ti ? run + 1 : 0;
    score += 1 + (wordStart ? 6 : 0) + run * 3 - Math.min(found - ti, 6) * 0.5;
    ti = found + 1;
  }
  if (t.startsWith(q)) score += 20;
  return score - t.length * 0.01;
}

export function rank(query: string, commands: Command[], limit = 40): Command[] {
  if (!query.trim()) return commands.slice(0, limit);
  return commands
    .map((c) => ({
      c,
      s: Math.max(
        fuzzyScore(query, c.title),
        fuzzyScore(query, `${c.title} ${c.subtitle ?? ""} ${c.keywords ?? ""}`) - 4,
      ),
    }))
    .filter((x) => x.s >= 0)
    .map((x) => ({ ...x, s: x.s + (x.c.boost ?? 0) }))
    .sort((a, b) => b.s - a.s)
    .slice(0, limit)
    .map((x) => x.c);
}

export const GROUP_ORDER = ["Ports", "Actions", "Filters", "View", "Settings"] as const;

/**
 * Sections for the palette. Without a query: the fixed group order. With one: still grouped (so a
 * run of "Stop …" results reads as one section), but sections follow their best match.
 */
export function groupResults(query: string, results: Command[]): { name: Command["group"]; items: Command[] }[] {
  const order: Command["group"][] = query.trim() ? [...new Set(results.map((c) => c.group))] : [...GROUP_ORDER];
  return order.map((name) => ({ name, items: results.filter((c) => c.group === name) })).filter((g) => g.items.length);
}
