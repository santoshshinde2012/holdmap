import type { ActionPlan, AppInfo, Config, Explanation, Graph, HistoryEntry, Snapshot, StopReport } from "./types";
import { MOCK_SNAPSHOT, mockExplain, mockPlan, mockStop, mockTopology } from "./mock";

/** True inside the Tauri webview; false in a plain browser (`npm run dev`), where mocks are used. */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function appInfo(): Promise<AppInfo> {
  if (!isTauri) return { version: "0.1.0", platform: "browser", tray: false, shortcut: null };
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

export async function freePort(near: number): Promise<number | null> {
  if (!isTauri) {
    const used = new Set(MOCK_SNAPSHOT.entries.map((e) => e.port));
    let p = near;
    while (used.has(p)) p++;
    return p;
  }
  return call("free_port", { near });
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

/** Service topology of the latest scan (dev services and their peers unless `all`). */
export async function topology(all: boolean): Promise<Graph> {
  if (!isTauri) return mockTopology();
  return call("topology", { all });
}

let mockConfig: Config = { pins: [{ port: 3000, label: null }], notify: true, notify_dev_only: true, history_limit: 200 };
const mockHistory: HistoryEntry[] = [];

export async function getConfig(): Promise<Config> {
  if (!isTauri) return mockConfig;
  return call("get_config");
}

export async function togglePin(port: number, label?: string): Promise<Config> {
  if (!isTauri) {
    const has = mockConfig.pins.some((p) => p.port === port);
    mockConfig = { ...mockConfig, pins: has ? mockConfig.pins.filter((p) => p.port !== port) : [...mockConfig.pins, { port, label: label ?? null }] };
    return mockConfig;
  }
  return call("toggle_pin", { port, label: label ?? null });
}

export async function setNotify(notify: boolean, devOnly: boolean): Promise<Config> {
  if (!isTauri) return (mockConfig = { ...mockConfig, notify, notify_dev_only: devOnly });
  return call("set_notify", { notify, devOnly });
}

export async function history(limit = 50): Promise<HistoryEntry[]> {
  if (!isTauri) return mockHistory.slice(0, limit);
  return call("history", { limit });
}

export async function clearHistory(): Promise<void> {
  if (!isTauri) { mockHistory.length = 0; return; }
  return call("clear_history");
}

export async function restart(entry: HistoryEntry): Promise<{ pid: number; command: string; log: string }> {
  if (!isTauri) return { pid: 4242, command: entry.command.join(" "), log: "/tmp/portwise.log" };
  return call("restart", { entry });
}

/** Query (enable = undefined) or change launch at login. */
export async function autostart(enable?: boolean): Promise<boolean> {
  if (!isTauri) return false;
  return call("autostart", { enable: enable ?? null });
}
