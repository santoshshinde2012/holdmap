import type { ActionPlan, AppInfo, Explanation, Snapshot, StopReport } from "./types";
import { MOCK_SNAPSHOT, mockExplain, mockPlan, mockStop } from "./mock";

/** True inside the Tauri webview; false in a plain browser (`npm run dev`), where mocks are used. */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function appInfo(): Promise<AppInfo> {
  if (!isTauri) return { version: "0.1.0", platform: "browser", tray: false };
  return call("app_info");
}

export async function scan(all: boolean): Promise<Snapshot> {
  if (!isTauri) {
    await delay(250);
    return { ...MOCK_SNAPSHOT, entries: [...MOCK_SNAPSHOT.entries], taken_at_ms: Date.now() };
  }
  return call("scan", { all });
}

export async function explain(port: number): Promise<Explanation> {
  if (!isTauri) return mockExplain(port);
  return call("explain", { port });
}

export async function plan(target: string, force: boolean, allowProtected = false): Promise<ActionPlan> {
  if (!isTauri) return mockPlan(target, force);
  return call("plan", { target, force, allowProtected });
}

export async function stop(target: string, force: boolean, allowProtected = false): Promise<StopReport> {
  if (!isTauri) {
    await delay(700);
    return mockStop(target);
  }
  return call("stop", { target, force, allowProtected });
}

export async function openUrl(url: string): Promise<void> {
  if (!isTauri) {
    window.open(url, "_blank", "noopener");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export async function onEvent<T>(name: string, cb: (payload: T) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(name, (e) => cb(e.payload));
}
