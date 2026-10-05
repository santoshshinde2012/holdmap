/**
 * Small caches with a hard size cap. Used for explain / HTTP / details so a long browse
 * of ports can't grow the UI heap without bound (a contributor to the blank WebView).
 */

/** Keep at most `max` entries, preferring `prefer` and then the most recently touched. */
export class LruMap<K, V> {
  private readonly order: K[] = [];
  private readonly vals = new Map<K, V>();
  constructor(private readonly max: number) {
    if (max < 1) throw new Error("LruMap max must be ≥ 1");
  }

  get size(): number {
    return this.vals.size;
  }

  has(key: K): boolean {
    return this.vals.has(key);
  }

  get(key: K): V | undefined {
    if (!this.vals.has(key)) return undefined;
    this.touch(key);
    return this.vals.get(key);
  }

  /** Value without bumping recency (for reads that shouldn't keep a key alive). */
  peek(key: K): V | undefined {
    return this.vals.get(key);
  }

  set(key: K, value: V, prefer?: ReadonlySet<K>): void {
    if (this.vals.has(key)) {
      this.vals.set(key, value);
      this.touch(key);
      return;
    }
    this.vals.set(key, value);
    this.order.push(key);
    this.trim(prefer);
  }

  delete(key: K): void {
    if (!this.vals.delete(key)) return;
    const i = this.order.indexOf(key);
    if (i >= 0) this.order.splice(i, 1);
  }

  /** Drop every key `keep` rejects. */
  prune(keep: (key: K) => boolean): void {
    for (const k of [...this.vals.keys()]) if (!keep(k)) this.delete(k);
  }

  clear(): void {
    this.vals.clear();
    this.order.length = 0;
  }

  keys(): IterableIterator<K> {
    return this.vals.keys();
  }

  /** A plain record of the current contents (for reactive state). */
  toRecord(): Record<string | number, V> {
    const out: Record<string | number, V> = {};
    for (const [k, v] of this.vals) out[k as string | number] = v;
    return out;
  }

  private touch(key: K): void {
    const i = this.order.indexOf(key);
    if (i >= 0) this.order.splice(i, 1);
    this.order.push(key);
  }

  private trim(prefer?: ReadonlySet<K>): void {
    while (this.vals.size > this.max && this.order.length) {
      const victim = this.order.find((k) => !prefer?.has(k)) ?? this.order[0];
      this.delete(victim);
    }
  }
}

/** Cap for explain / HTTP / details caches (selected port is always kept via `prefer`). */
export const DETAILS_CACHE_MAX = 24;
/** Cap for timestamp maps that track when a port was last probed. */
export const STAMP_CACHE_MAX = 64;
