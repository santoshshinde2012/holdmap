/** Vite `npm run dev` talks to a local `holdmap` CLI. The website demo and Playwright keep sample data. */

export const LIVE_PREFIX = "/__holdmap";

export function wantsSampleData(search = typeof location === "undefined" ? "" : location.search): boolean {
  const value = new URLSearchParams(search).get("demo");
  return value !== null && value !== "0" && value !== "false";
}

export function useLiveCollector(env: { tauri?: boolean; dev?: boolean; vitest?: boolean; search?: string } = {}): boolean {
  const tauri = env.tauri ?? (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window);
  const dev = env.dev ?? import.meta.env.DEV;
  const vitest = env.vitest ?? Boolean(import.meta.env.VITEST);
  const search = env.search ?? (typeof location === "undefined" ? "" : location.search);
  return !tauri && Boolean(dev) && !vitest && !wantsSampleData(search);
}

export async function liveJson<T>(route: string, query: Record<string, string | number | boolean | undefined> = {}): Promise<T> {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value === undefined || value === false) continue;
    params.set(key, value === true ? "1" : String(value));
  }
  const suffix = params.size ? `?${params}` : "";
  const response = await fetch(`${LIVE_PREFIX}/${route}${suffix}`);
  const body: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = body && typeof body === "object" && "error" in body && typeof body.error === "string" ? body.error : `Live holdmap scan failed (${response.status})`;
    throw new Error(error);
  }
  return body as T;
}
