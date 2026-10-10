import type { ActionPlan, AgentsReport, AppInfo, Config, Explanation, Graph, HistoryEntry, HttpInfo, PortDetails, PortEntry, Snapshot, StopReport } from "./types";
import { version as pkgVersion } from "../../package.json";
import { MOCK_SNAPSHOT, mockAgents, mockDetails, mockExplain, mockHttp, mockPlan, mockStop, mockTopology } from "./mock";

/** True inside the Tauri webview; false in a plain browser (`npm run dev`), where mocks are used. */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function appInfo(): Promise<AppInfo> {
  if (!isTauri) return { version: pkgVersion, platform: "browser", tray: false, shortcut: mockShortcut, config_dir: "~/.config/holdmap" };
  return call("app_info");
}

/** A snapshot no older than `maxAgeMs` (the app shares a scan that just ran; 0 forces one). */
export async function scan(all: boolean, maxAgeMs?: number): Promise<Snapshot> {
  if (!isTauri) {
    await delay(250);
    return { ...MOCK_SNAPSHOT, entries: mockEntries().map(mockUsage), taken_at_ms: Date.now() };
  }
  return call("scan", { all, maxAgeMs });
}

export async function explain(port: number): Promise<Explanation> {
  if (!isTauri) return mockExplain(port);
  return call("explain", { port });
}

/** HTTP status and page title of a local port, or null when it doesn't speak HTTP. */
export async function http(port: number): Promise<HttpInfo | null> {
  if (!isTauri) return mockHttp(port);
  return call("http_info", { port });
}

export async function plan(target: string, force: boolean, allowProtected = false): Promise<ActionPlan> {
  if (!isTauri) return mockPlan(target, force, allowProtected);
  return call("plan", { target, force, allowProtected });
}

export async function stop(target: string, force: boolean, allowProtected = false): Promise<StopReport> {
  if (!isTauri) {
    await delay(700);
    // Browser preview: remember the stop so the "Recently stopped" panel has something to show.
    const e = MOCK_SNAPSHOT.entries.find((x) => x.port === parseInt(target, 10));
    const report = mockStop(target);
    if (e?.process) {
      mockHistory.unshift({ at_ms: Date.now(), port: e.port, protocol: e.protocol, label: e.label, command: e.process.cmdline, cwd: e.process.cwd, project: e.project?.name ?? null, framework: e.framework?.name ?? null, pid: e.process.pid });
    }
    return report;
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

/** Open http://localhost:<port> in the default browser (the backend builds the URL). */
export async function openPort(port: number): Promise<void> {
  if (!isTauri) {
    window.open(`http://localhost:${port}`, "_blank", "noopener");
    return;
  }
  return call("open_port", { port });
}

/** Connections, process tree, uptime and bind risk for the listeners on a port (lazy). */
export async function portDetails(port: number): Promise<PortDetails[]> {
  if (!isTauri) return mockDetails(port);
  return call("port_details", { port });
}

/** Show the project folder of the service on `port` in Finder / Explorer. */
export async function revealProject(port: number): Promise<void> {
  if (!isTauri) return;
  return call("reveal_project", { port });
}

/** Open the project folder of the service on `port` in an editor; resolves to its name. */
export async function openInEditor(port: number): Promise<string> {
  if (!isTauri) return "VS Code";
  return call("open_in_editor", { port });
}

/** Show an agent's folder in Finder / Explorer (path must belong to that agent). */
export async function revealAgentFolder(agentId: string, path: string): Promise<void> {
  if (!isTauri) return;
  return call("reveal_agent_folder", { agentId, path });
}

/** Open an agent's folder in the user's editor; resolves to its name. */
export async function openAgentFolder(agentId: string, path: string): Promise<string> {
  if (!isTauri) return "VS Code";
  return call("open_agent_folder", { agentId, path });
}

/** Start again what holdmap just stopped on `port` (the second half of "Restart"). */
export async function restartStopped(port: number): Promise<{ pid: number; command: string; log: string }> {
  if (!isTauri) return { pid: 4243, command: "npm run dev", log: "/tmp/holdmap.log" };
  return call("restart_stopped", { port });
}

/** Download, verify and install the update the backend announced, then restart. */
export async function installUpdate(): Promise<void> {
  if (!isTauri) return;
  return call("install_update");
}

export async function onEvent<T>(name: string, cb: (payload: T) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(name, (e) => cb(e.payload));
}

/** Service topology of the latest scan (dev services and their peers unless `all`). */
export async function topology(all: boolean): Promise<Graph> {
  if (!isTauri) {
    // Live like the real thing: every poll is a new graph whose CPU figures move.
    const g = mockTopology();
    return { ...g, nodes: g.nodes.map((n) => (n.root_pid ? { ...n, cpu_percent: Math.round((n.cpu_percent + Math.random()) * 10) / 10 } : n)) };
  }
  return call("topology", { all });
}

/** The AI coding agents of the latest scan and their footprint. */
export async function agents(): Promise<AgentsReport> {
  if (!isTauri) return mockAgents({ ...MOCK_SNAPSHOT, taken_at_ms: Date.now() });
  return call("agents");
}

let mockConfig: Config = { pins: [{ port: 3000, label: null }], notify: true, notify_dev_only: true, history_limit: 200, scan_interval_secs: 4, hotkey: "alt-p", recent_hosts: ["devbox", "deploy@staging.internal"] };

// Same labels as src-tauri/src/shortcuts.rs, so browser screenshots match the platform.
const mac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
const MOCK_HOTKEYS = [{ id: "alt-p", label: mac ? "⌘⌥P" : "Ctrl+Alt+P" }, { id: "alt-space", label: mac ? "⌥Space" : "Ctrl+Alt+Space" }, { id: "alt-k", label: mac ? "⌘⌥K" : "Ctrl+Alt+K" }, { id: "off", label: "Off" }];
const mockHistory: HistoryEntry[] = [];
let mockShortcut: string | null = MOCK_HOTKEYS[0].label;

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
  if (!isTauri) return { pid: 4242, command: entry.command.join(" "), log: "/tmp/holdmap.log" };
  // Name the entry only: the backend reads the command from its own history file.
  return call("restart", { atMs: entry.at_ms, port: entry.port });
}

/** Query (enable = undefined) or change launch at login. */
export async function autostart(enable?: boolean): Promise<boolean> {
  if (!isTauri) return false;
  return call("autostart", { enable: enable ?? null });
}

/** Pin `port` or update its label. */
export async function setPin(port: number, label: string | null): Promise<Config> {
  if (!isTauri) {
    const pins = mockConfig.pins.filter((p) => p.port !== port);
    pins.push({ port, label: label?.trim() || null });
    return (mockConfig = { ...mockConfig, pins: pins.sort((a, b) => a.port - b.port) });
  }
  return call("set_pin", { port, label: label?.trim() || null });
}

export async function unpin(port: number): Promise<Config> {
  if (!isTauri) return (mockConfig = { ...mockConfig, pins: mockConfig.pins.filter((p) => p.port !== port) });
  return call("unpin", { port });
}

export async function setPreferences(p: { scanIntervalSecs?: number; historyLimit?: number }): Promise<Config> {
  if (!isTauri) {
    await delay(150);
    return (mockConfig = { ...mockConfig, scan_interval_secs: p.scanIntervalSecs ?? mockConfig.scan_interval_secs, history_limit: p.historyLimit ?? mockConfig.history_limit });
  }
  return call("set_preferences", { scanIntervalSecs: p.scanIntervalSecs ?? null, historyLimit: p.historyLimit ?? null });
}

export async function hotkeys(): Promise<{ id: string; label: string }[]> {
  if (!isTauri) return MOCK_HOTKEYS;
  return call("hotkeys");
}

/** Switch the global shortcut; resolves to the active label (null when off). */
export async function setHotkey(preset: string): Promise<string | null> {
  if (!isTauri) {
    mockConfig = { ...mockConfig, hotkey: preset };
    return (mockShortcut = MOCK_HOTKEYS.find((h) => h.id === preset && h.id !== "off")?.label ?? null);
  }
  return call("set_hotkey", { preset });
}

/** Read-only scan of another machine over SSH. */
export async function remoteScan(host: string): Promise<Snapshot> {
  if (!isTauri) {
    await delay(900);
    if (/fail|offline/.test(host)) throw new Error(`ssh ${host} failed or timed out: ssh: connect to host ${host} port 22: Connection refused`);
    mockConfig = { ...mockConfig, recent_hosts: [host, ...mockConfig.recent_hosts.filter((h) => h !== host)].slice(0, 6) };
    const entries = MOCK_SNAPSHOT.entries.filter((e) => e.process && !e.container).slice(0, 7).map((e) => ({ ...e, id: "r:" + e.id, user: e.port < 1024 ? "root" : "deploy", project: null }));
    return { ...MOCK_SNAPSHOT, entries, scan_ms: 212, taken_at_ms: Date.now(), platform: `remote:${host}` };
  }
  return call("remote_scan", { host });
}

/** Browser demo only: a plausible, gently moving CPU figure per process so sparklines have a shape. */
/** The mock ports; `?mockPorts=N` pads the list to N for scrolling and polling load tests. */
function mockEntries(): PortEntry[] {
  const want = Number(new URLSearchParams(globalThis.location?.search ?? "").get("mockPorts")) || 0;
  const out = [...MOCK_SNAPSHOT.entries];
  const tmpl = MOCK_SNAPSHOT.entries.filter((e) => e.process);
  for (let i = 0; out.length < want; i++) {
    const t = tmpl[i % tmpl.length], port = 20000 + i;
    out.push({ ...t, id: `tcp:${port}`, port, pid: 50000 + i, pids: [50000 + i], process: { ...t.process!, pid: 50000 + i } });
  }
  return out;
}

function mockUsage(e: PortEntry): PortEntry {
  if (!e.process) return e;
  const base = ((e.port * 7919) % 23) / 2 + 0.4;
  const t = Date.now() / 4000 + e.port;
  const cpu = Math.max(0, base * (1 + 0.55 * Math.sin(t) + 0.3 * Math.sin(t * 2.7)) + Math.random() * 0.6);
  return { ...e, process: { ...e.process, cpu_percent: Math.round(cpu * 10) / 10 } };
}
