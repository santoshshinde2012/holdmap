/** UI preferences in localStorage under `holdmap.*`, with one-shot read of legacy `pw.*`. */

const PREFIX = "holdmap.";
const LEGACY = "pw.";

function rawGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function rawSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* private mode */
  }
}

/** Read `holdmap.<name>`, else migrate from `pw.<name>` if present. */
export function prefGet(name: string): string | null {
  const next = rawGet(PREFIX + name);
  if (next !== null) return next;
  const old = rawGet(LEGACY + name);
  if (old !== null) {
    rawSet(PREFIX + name, old);
    try {
      localStorage.removeItem(LEGACY + name);
    } catch {
      /* ignore */
    }
  }
  return old;
}

/** Read a stored toggle; missing or malformed values use the documented default. */
export function prefGetBoolean(name: string, fallback: boolean): boolean {
  const value = prefGet(name);
  return value === "1" ? true : value === "0" ? false : fallback;
}

/** Persist `holdmap.<name>` and drop any leftover `pw.<name>`. */
export function prefSet(name: string, value: string): void {
  rawSet(PREFIX + name, value);
  try {
    localStorage.removeItem(LEGACY + name);
  } catch {
    /* ignore */
  }
}
