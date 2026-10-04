// Structural sharing for live data. Each poll returns brand-new objects even when nothing
// changed; `share` hands back the previous object wherever the new one is equal, so Svelte's
// `===` checks see "unchanged" and nothing downstream re-renders, re-fetches or remounts.

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null;
const idOf = (v: unknown) => (isObj(v) && typeof v.id === "string" ? v.id : undefined);

/**
 * `next`, reusing every part of `prev` that is deep-equal to it. Returns `prev` itself when
 * the two are equal. Arrays of `{ id: string }` items are matched by id (so a reorder keeps
 * each item's identity), other arrays by index.
 */
export function share<T>(prev: T | null | undefined, next: T): T {
  if (Object.is(prev, next) || !isObj(prev) || !isObj(next)) return next;
  if (Array.isArray(prev) !== Array.isArray(next)) return next;
  if (Array.isArray(next)) {
    const p = prev as unknown as unknown[];
    const byId = new Map<string, unknown>();
    for (const x of p) { const id = idOf(x); if (id !== undefined) byId.set(id, x); }
    let same = p.length === next.length;
    const out = next.map((x, i) => {
      const id = idOf(x);
      const v = share(id !== undefined ? byId.get(id) : p[i], x);
      if (v !== p[i]) same = false;
      return v;
    });
    return (same ? prev : out) as T;
  }
  const keys = Object.keys(next);
  let same = keys.length === Object.keys(prev).length;
  const out: Obj = {};
  for (const k of keys) {
    const v = share(prev[k], (next as Obj)[k]);
    if (v !== prev[k] || !(k in prev)) same = false;
    out[k] = v;
  }
  return (same ? prev : out) as T;
}

/** A record with `key` set to `value`, or the same record when the value is already there. */
export function withKey<K extends string | number, V>(rec: Record<K, V>, key: K, value: V): Record<K, V> {
  return key in rec && rec[key] === value ? rec : { ...rec, [key]: value };
}

/** Only the keys `keep` accepts, or the same record when nothing goes. */
export function pruned<V>(rec: Record<number, V>, keep: (key: number) => boolean): Record<number, V> {
  const drop = Object.keys(rec).map(Number).filter((k) => !keep(k));
  if (!drop.length) return rec;
  const out = { ...rec };
  for (const k of drop) delete out[k];
  return out;
}

/**
 * The header's freshness label. A scan lands every `scanSecs` plus however long the scan
 * takes, so "Live" holds for two cadences; only a real stall shows "Ns ago" (and turns amber
 * after three), instead of flickering to "4s ago" between ordinary scans.
 */
export function freshness(ago: number | null, scanSecs: number): { text: string; stale: boolean } {
  if (ago === null) return { text: "starting…", stale: false };
  const live = Math.max(6, scanSecs * 2 + 2);
  return { text: ago < live ? "Live" : `${ago}s ago`, stale: ago >= Math.max(9, scanSecs * 3 + 3) };
}
