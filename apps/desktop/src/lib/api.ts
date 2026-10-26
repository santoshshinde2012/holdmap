import type { ActivitySnapshot, AgentFlowReport, RecordingCredentials, RecordingStatus } from "./agent-flow/model";
import { demoActivity, demoAgentFlow, demoRecordingStatus, startDemoRecording, stopDemoRecording, clearDemoActivity } from "./activity-demo";
import { liveJson, useLiveCollector } from "./browser-live";
import type { ActionPlan, AgentsReport, AppInfo, Config, DesktopHelpDestination, Explanation, Graph, HistoryEntry, HttpInfo, PortDetails, PortEntry, Protocol, Snapshot, StopReport } from "./types";
import { version as pkgVersion } from "../../package.json";
import type { ShutdownPreview } from "./power";
import { activeScheduleCount, DemoSchedules, validateScheduleDelay, validateScheduledStops, type ScheduledAction, type ScheduledStopRequest } from "./schedules";
import type { AnalyzerResult, AnalyzerView, CatalogDiff, CatalogStatus, ConfiguredServer, FindingState, Insights, McpDraft, RunEvent, RunRecord, OutputMapping, Preview, ToolReport, ToolSpec } from "./analyzers";
import { DemoAnalyzers } from "./analyzers-demo";
import type { PresetView } from "./analyzer-presets";
import type { InstallEvent, InstallPlan, InstallRecord } from "./analyzer-install";
import { attention, type AttentionItem } from "./home";
import { MOCK_SNAPSHOT, mockAgents, mockDetails, mockExplain, mockHttp, mockPlan, mockStop, mockTopology } from "./mock";

/** True inside the Tauri webview. */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Inspection = Explanation & { details?: PortDetails[]; http?: HttpInfo | null };

function liveReads(): boolean {
  return useLiveCollector();
}

/** Production previews use examples; the local Vite collector reads real CLI metadata. */
export function usesSampleData(): boolean {
  return !isTauri && !liveReads();
}

function liveWrites(): never {
  throw new Error("Use the Holdmap desktop app to change running processes. The browser live collector is read-only.");
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

let demoPowerSequence = 0;
const demoPowerPreviews = new Map<string, number>();
const demoStopPreviews = new Map<string, { expires: number; target: string; force: boolean; allowProtected: boolean; blocked: boolean }>();
let demoStopSequence = 0;
const demoSchedules = new DemoSchedules();
let demoScheduleTimer: ReturnType<typeof setInterval> | null = null;

function watchDemoSchedules() {
  if (demoScheduleTimer) return;
  demoScheduleTimer = setInterval(() => {
    demoSchedules.tick();
    if (!activeScheduleCount(demoSchedules.list())) {
      clearInterval(demoScheduleTimer!);
      demoScheduleTimer = null;
    }
  }, 1000);
}

/** Review shutdown of the local desktop host. Browser mode only simulates this flow. */
export async function shutdownPreview(): Promise<ShutdownPreview> {
  if (liveReads()) liveWrites();
  if (isTauri) return call("shutdown_preview");
  demoPowerPreviews.clear();
  const confirmation_id = `demo-power-${++demoPowerSequence}`;
  demoPowerPreviews.set(confirmation_id, Date.now() + 60_000);
  return { confirmation_id, platform: "browser", hostname: "Demo computer", expires_in_secs: 60 };
}

export async function shutdownMachine(confirmationId: string): Promise<{ requested: boolean }> {
  if (!confirmationId?.trim()) throw new Error("Review shutdown before confirming.");
  if (liveReads()) liveWrites();
  if (isTauri) return call("shutdown_machine", { confirmationId });
  const expires = demoPowerPreviews.get(confirmationId);
  demoPowerPreviews.delete(confirmationId);
  if (!expires || Date.now() >= expires) throw new Error("Shutdown confirmation expired or was already used. Review again.");
  // Sample mode never imports or invokes a native power command.
  await delay(250);
  return { requested: true };
}

/** Schedules are owned by the local desktop session; browser mode only simulates them. */
export async function scheduleShutdown(confirmationId: string, delaySecs: number): Promise<ScheduledAction> {
  validateScheduleDelay(delaySecs);
  if (!confirmationId?.trim()) throw new Error("Review shutdown before scheduling.");
  if (liveReads()) liveWrites();
  if (isTauri) return call("schedule_shutdown", { confirmationId, delaySecs });
  const expires = demoPowerPreviews.get(confirmationId);
  if (!expires || Date.now() >= expires) throw new Error("Shutdown confirmation expired or was already used. Review again.");
  const action = demoSchedules.create("shutdown", "Shut down Demo computer", [], delaySecs);
  demoPowerPreviews.delete(confirmationId);
  watchDemoSchedules();
  return action;
}

export async function scheduleStops(requests: ScheduledStopRequest[], delaySecs: number, agentId?: string): Promise<ScheduledAction> {
  validateScheduleDelay(delaySecs);
  validateScheduledStops(requests);
  if (liveReads()) liveWrites();
  if (isTauri) return call("schedule_stops", { requests, delaySecs, agentId: agentId ?? null });
  for (const request of requests) {
    const preview = demoStopPreviews.get(request.confirmation_id);
    if (!preview || Date.now() >= preview.expires || preview.target !== request.target || preview.force !== request.force || preview.allowProtected !== request.allow_protected || preview.blocked) {
      throw new Error("A reviewed stop plan expired, changed or was already used. Review every port again.");
    }
  }
  const targets = requests.map((request) => request.target);
  const action = demoSchedules.create(agentId ? "agent" : "port", agentId ? `Stop agent services (${targets.length} ${targets.length === 1 ? "port" : "ports"})` : `Stop :${targets[0]}`, targets, delaySecs);
  for (const request of requests) demoStopPreviews.delete(request.confirmation_id);
  watchDemoSchedules();
  return action;
}

export async function listSchedules(): Promise<ScheduledAction[]> {
  if (isTauri) return call("list_schedules");
  demoSchedules.tick();
  return demoSchedules.list();
}

export async function cancelSchedule(id: string): Promise<ScheduledAction> {
  if (!id?.trim()) throw new Error("Choose a scheduled action to cancel.");
  if (isTauri) return call("cancel_schedule", { id });
  return demoSchedules.cancel(id);
}

export async function appInfo(): Promise<AppInfo> {
  if (!isTauri) return { version: pkgVersion, platform: "browser", tray: false, shortcut: mockShortcut };
  return call("app_info");
}

const desktopHelpUrls: Record<DesktopHelpDestination, string> = {
  downloads: "https://santoshshinde2012.github.io/holdmap/#install",
  installation: "https://santoshshinde2012.github.io/holdmap/docs/desktop/#first-open",
};

/** Explicit help actions only; native code independently restricts the destination. */
export async function openDesktopHelp(destination: DesktopHelpDestination): Promise<void> {
  if (destination !== "downloads" && destination !== "installation") throw new Error("Choose desktop downloads or the installation guide.");
  if (isTauri) return call("open_desktop_help", { destination });
  window.open(desktopHelpUrls[destination], "_blank", "noopener,noreferrer");
}

/** A snapshot no older than `maxAgeMs` (the app shares a scan that just ran; 0 forces one). */
export async function scan(all: boolean, maxAgeMs?: number): Promise<Snapshot> {
  if (isTauri) return call("scan", { all, maxAgeMs });
  if (liveReads()) return liveJson("scan", { all });
  await delay(250);
  return { ...MOCK_SNAPSHOT, entries: mockEntries().map(mockUsage), taken_at_ms: Date.now() };
}

export async function explain(port: number): Promise<Explanation> {
  if (isTauri) return call("explain", { port });
  if (liveReads()) return liveJson("explain", { port });
  return mockExplain(port);
}

/** HTTP status and page title of a local port, or null when it doesn't speak HTTP. */
export async function http(port: number): Promise<HttpInfo | null> {
  if (isTauri) return call("http_info", { port });
  if (liveReads()) return (await liveJson<Inspection>("inspect", { port })).http ?? null;
  return mockHttp(port);
}

export async function plan(target: string, force: boolean, allowProtected = false): Promise<ActionPlan> {
  if (liveReads()) liveWrites();
  if (!isTauri) {
    const reviewed = mockPlan(target, force, allowProtected);
    const confirmation_id = `demo-stop-${++demoStopSequence}`;
    for (const [id, preview] of demoStopPreviews) if (Date.now() >= preview.expires) demoStopPreviews.delete(id);
    // A bounded, short-lived preview registry is independent of the simulated action list.
    if (demoStopPreviews.size >= 256) demoStopPreviews.delete(demoStopPreviews.keys().next().value!);
    demoStopPreviews.set(confirmation_id, { expires: Date.now() + 300_000, target, force, allowProtected, blocked: !!reviewed.blocked });
    return { ...reviewed, confirmation_id };
  }
  return call("plan", { target, force, allowProtected });
}

export async function stop(target: string, force: boolean, allowProtected = false, confirmationId?: string): Promise<StopReport> {
  if (liveReads()) liveWrites();
  if (!isTauri) {
    if (confirmationId) {
      const reviewed = demoStopPreviews.get(confirmationId);
      if (!reviewed || Date.now() >= reviewed.expires || reviewed.target !== target || reviewed.force !== force || reviewed.allowProtected !== allowProtected || reviewed.blocked) {
        throw new Error("The reviewed stop plan expired, changed or was already used. Review again.");
      }
      demoStopPreviews.delete(confirmationId);
    }
    await delay(700);
    // Browser preview: remember the stop so the "Recently stopped" panel has something to show.
    const e = MOCK_SNAPSHOT.entries.find((x) => x.port === parseInt(target, 10));
    const report = mockStop(target);
    if (e?.process) {
      mockHistory.unshift({ at_ms: Date.now(), port: e.port, protocol: e.protocol, label: e.label, command: e.process.cmdline, cwd: e.process.cwd, project: e.project?.name ?? null, framework: e.framework?.name ?? null, pid: e.process.pid });
    }
    return report;
  }
  if (!confirmationId) throw new Error("Review a stop plan before confirming.");
  return call("stop", { target, force, allowProtected, confirmationId });
}

/** Review all agent targets against one fresh native scan before dispatching any stop. */
export async function stopMany(agentId: string, requests: ScheduledStopRequest[]): Promise<StopReport[]> {
  if (!agentId?.trim() || agentId.length > 64) throw new Error("Choose the agent whose reviewed services should be stopped.");
  if (!requests.length || requests.length > 32) throw new Error("Review between 1 and 32 agent listener targets.");
  const targets = new Set<string>(), handles = new Set<string>();
  for (const request of requests) {
    if (!/^[1-9]\d{0,4}\/(tcp|udp)$/.test(request.target) || Number(request.target.split("/")[0]) > 65_535) throw new Error("Each stop requires a canonical local port and protocol.");
    if (!request.confirmation_id?.trim() || request.confirmation_id.length > 64) throw new Error("Review each stop plan before confirming.");
    if (request.force !== false || request.allow_protected !== false) throw new Error("Agent batch stops require normal mode without a protection override.");
    if (targets.has(request.target) || handles.has(request.confirmation_id)) throw new Error("Each agent listener needs its own reviewed plan.");
    targets.add(request.target);
    handles.add(request.confirmation_id);
  }
  if (isTauri) return call("stop_many", { agentId, requests });
  if (liveReads()) liveWrites();
  // Browser mode validates and consumes the complete review before changing sample data.
  // It never imports or invokes native process APIs.
  const previews = requests.map((request) => demoStopPreviews.get(request.confirmation_id));
  for (const request of requests) demoStopPreviews.delete(request.confirmation_id);
  for (const [index, request] of requests.entries()) {
    const preview = previews[index];
    if (!preview || Date.now() >= preview.expires || preview.target !== request.target || preview.force || preview.allowProtected || preview.blocked) throw new Error("An agent stop plan expired, changed or was already used. Review every target again.");
  }
  const agent = mockAgents().agents.find((value) => value.id === agentId);
  const owned = new Set(agent?.ports.filter((port) => port.role === "dev_server" || port.role === "service").map((port) => `${port.port}/${port.protocol}`));
  if (!agent || requests.some((request) => !owned.has(request.target))) throw new Error("The reviewed listeners no longer belong to this agent's stoppable services.");
  const reports: StopReport[] = [];
  for (const request of requests) {
    const report = await stop(request.target, false, false);
    reports.push(report);
    if (!report.success || !report.freed || report.history_error) break;
  }
  return reports;
}

export async function freePort(near: number): Promise<number | null> {
  if (liveReads()) liveWrites();
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
  if (isTauri) return call("port_details", { port });
  if (liveReads()) return (await liveJson<Inspection>("inspect", { port })).details ?? [];
  return mockDetails(port);
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
export async function restartStopped(port: number, protocol: Protocol, pid: number): Promise<{ pid: number; command: string; log: string }> {
  if (liveReads()) liveWrites();
  if (!isTauri) return { pid: 4243, command: "npm run dev", log: "/tmp/holdmap.log" };
  return call("restart_stopped", { port, protocol, pid });
}

/** Download, verify and install the update the backend announced, then restart. */
export async function installUpdate(): Promise<void> {
  if (!isTauri) return;
  return call("install_update");
}

export async function onEvent<T>(name: string, cb: (payload: T) => void): Promise<() => void> {
  if (!isTauri) return name === "schedules-changed" ? demoSchedules.subscribe((actions) => cb(actions as T)) : () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(name, (e) => cb(e.payload));
}

/** Service topology of the latest scan (dev services and their peers unless `all`). */
export async function topology(all: boolean): Promise<Graph> {
  if (isTauri) return call("topology", { all });
  if (liveReads()) return liveJson("topology", { all });
  const g = mockTopology({ ...MOCK_SNAPSHOT, entries: mockEntries() }, all);
  return { ...g, nodes: g.nodes.map((n) => (n.root_pid ? { ...n, cpu_percent: Math.round((n.cpu_percent + Math.random()) * 10) / 10 } : n)) };
}

/** Ordinary agent scans leave capability files alone; inspection is requested by its workspace. */
export async function agents(includeCapabilities = false): Promise<AgentsReport> {
  if (isTauri) return call("agents", { includeCapabilities });
  if (liveReads()) return liveJson("agents", { capabilities: includeCapabilities });
  const report = mockAgents({ ...MOCK_SNAPSHOT, taken_at_ms: Date.now() }, includeCapabilities);
  return emptyDemo() ? { ...report, agents: [], configured: [] } : report;
}

/** OS process/socket relationships. Recorded calls are fetched independently. */
export async function agentFlow(): Promise<AgentFlowReport> {
  if (isTauri) return call("agent_flow");
  if (liveReads()) return liveJson("agent_flow");
  return demoAgentFlow(await agents());
}

export async function activitySnapshot(): Promise<ActivitySnapshot> {
  if (isTauri) return call("activity_snapshot");
  if (liveReads()) return liveJson("activity");
  return demoActivity();
}

export async function activityStatus(): Promise<RecordingStatus> {
  if (isTauri) return call("activity_status");
  if (liveReads()) return { active: false, endpoint: null, started_at_ms: null, received: 0, dropped: 0 };
  return demoRecordingStatus();
}

/** Explicit opt-in only; credentials are transient and never persisted in frontend settings. */
export async function startActivity(): Promise<RecordingCredentials> {
  if (isTauri) return call("start_activity");
  if (liveReads()) liveWrites();
  startDemoRecording();
  return { endpoint: "", token: "" };
}

export async function stopActivity(): Promise<RecordingStatus> {
  if (isTauri) return call("stop_activity");
  if (liveReads()) liveWrites();
  stopDemoRecording();
  return demoRecordingStatus();
}

export async function clearActivity(): Promise<void> {
  if (isTauri) return call("clear_activity");
  if (liveReads()) liveWrites();
  clearDemoActivity();
}

/** Returned only on the user's copy action. Includes an ephemeral recording credential. */
export async function activitySetup(provider: "codex" | "claude"): Promise<string> {
  if (isTauri) return call("activity_setup", { provider });
  if (liveReads()) return "# Live browser mode cannot start a local recorder.\n# Use the desktop app: Agents > Flow > Calls > Start recording.\n";
  return "# Browser demo: no local collector is running.\n# Install the desktop app and start recording under Agents > Flow > Calls.\n";
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
  if (liveReads()) liveWrites();
  if (!isTauri) return { pid: 4242, command: entry.command.join(" "), log: "/tmp/holdmap.log" };
  // Name the exact recording only: executable syntax stays in the backend's history file.
  return call("restart", { atMs: entry.at_ms, port: entry.port, protocol: entry.protocol, pid: entry.pid });
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
  if (isTauri) return call("remote_scan", { host });
  if (liveReads()) throw new Error("Remote scans need the Holdmap desktop app.");
  await delay(900);
  if (/fail|offline/.test(host)) throw new Error(`ssh ${host} failed or timed out: ssh: connect to host ${host} port 22: Connection refused`);
  mockConfig = { ...mockConfig, recent_hosts: [host, ...mockConfig.recent_hosts.filter((h) => h !== host)].slice(0, 6) };
  const entries = MOCK_SNAPSHOT.entries.filter((e) => e.process && !e.container).slice(0, 7).map((e) => ({ ...e, id: "r:" + e.id, user: e.port < 1024 ? "root" : "deploy", project: null }));
  return { ...MOCK_SNAPSHOT, entries, scan_ms: 212, taken_at_ms: Date.now(), platform: `remote:${host}` };
}

/** Browser demo only: a plausible, gently moving CPU figure per process so sparklines have a shape. */
/** The mock ports; `?mockPorts=N` pads the list to N for scrolling and polling load tests. */
export const MAX_MOCK_PORTS = 1_000;
/** `?mockPorts=0` pretends nothing is listening and no agents run: the first-minute empty machine. */
const emptyDemo = () => new URLSearchParams(globalThis.location?.search ?? "").get("mockPorts") === "0";
function mockEntries(): PortEntry[] {
  if (emptyDemo()) return [];
  const requested = Number(new URLSearchParams(globalThis.location?.search ?? "").get("mockPorts"));
  const want = Number.isFinite(requested) ? Math.min(MAX_MOCK_PORTS, Math.max(0, Math.floor(requested))) : 0;
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

// Opt-in analyzers. The desktop runs them; the browser demo simulates approval and runs.
const demoAnalyzers = new DemoAnalyzers(emptyDemo());

export async function analyzers(): Promise<AnalyzerView[]> {
  if (isTauri) return call("analyzers_list");
  if (liveReads()) return [];
  return demoAnalyzers.list();
}

export async function analyzerPreview(id: string): Promise<Preview> {
  if (isTauri) return call("analyzer_preview", { id });
  if (liveReads()) liveWrites();
  return demoAnalyzers.preview(id, await agents(true));
}

/** Approve what the sheet showed; a remote MCP server also needs `allowRemote`. */
export async function approveAnalyzer(id: string, allowRemote = false): Promise<AnalyzerView> {
  if (isTauri) return call("analyzer_approve", { id, allowRemote });
  if (liveReads()) liveWrites();
  return demoAnalyzers.approve(id, allowRemote);
}

/** Stop the run in progress (the MCP call is cancelled; CLI analyzers are stopped). */
export async function cancelAnalyzerRun(): Promise<void> {
  if (isTauri) return call("analyzer_cancel");
  demoAnalyzers.cancel();
}

/** Register any MCP server as an analyzer (simulated in the browser demo). It stays off. */
export async function addMcpAnalyzer(draft: McpDraft): Promise<AnalyzerView> {
  if (isTauri) return call("analyzer_add_mcp", { draft });
  if (liveReads()) liveWrites();
  return demoAnalyzers.addMcp(draft);
}

/** The agents' configured MCP servers that can become analyzers. Starts nothing. */
export async function configuredMcpServers(): Promise<ConfiguredServer[]> {
  if (isTauri) return call("analyzer_configured_servers");
  if (liveReads()) return [];
  return demoAnalyzers.configuredServers(await agents(true));
}

/** Start (or contact) the server once and list its tools; no tool is called. */
export async function listAnalyzerTools(id: string): Promise<ToolReport> {
  if (isTauri) return call("analyzer_list_tools", { id });
  if (liveReads()) liveWrites();
  await delay(400);
  return demoAnalyzers.listTools(id);
}

/** The saved tool listing. Starts nothing. */
export async function analyzerTools(id: string): Promise<ToolReport> {
  if (isTauri) return call("analyzer_tools", { id });
  if (liveReads()) liveWrites();
  return demoAnalyzers.tools(id);
}

/** Save the chosen tools and their input mapping (approval is needed again). */
export async function mapAnalyzerTools(id: string, tools: ToolSpec[]): Promise<AnalyzerView> {
  if (isTauri) return call("analyzer_map_tools", { id, tools });
  if (liveReads()) liveWrites();
  return demoAnalyzers.mapTools(id, tools);
}

/** Save where findings sit in the tools' output (approval is needed again). */
export async function mapAnalyzerOutput(id: string, output: OutputMapping): Promise<AnalyzerView> {
  if (isTauri) return call("analyzer_map_output", { id, output });
  if (liveReads()) liveWrites();
  return demoAnalyzers.mapOutput(id, output);
}

export async function revokeAnalyzer(id: string): Promise<boolean> {
  if (isTauri) return call("analyzer_revoke", { id });
  if (liveReads()) liveWrites();
  return demoAnalyzers.revoke(id);
}

/** Register an analyzer from manifest text (simulated in the browser demo). */
export async function addAnalyzer(manifest: string): Promise<AnalyzerView> {
  if (isTauri) return call("analyzer_add", { manifest });
  if (liveReads()) liveWrites();
  return demoAnalyzers.add(manifest);
}

/** Ready-made manifests for well-known local analyzers. */
export async function analyzerPresets(): Promise<PresetView[]> {
  if (isTauri) return call("analyzer_presets");
  if (liveReads()) return [];
  return demoAnalyzers.presets();
}

/** Register a preset as the user's own analyzer; it stays off until approved. */
export async function addAnalyzerPreset(id: string): Promise<AnalyzerView> {
  if (isTauri) return call("analyzer_add_preset", { id });
  if (liveReads()) liveWrites();
  return demoAnalyzers.addPreset(id);
}

/** Remove an analyzer the user registered. */
export async function removeAnalyzer(id: string): Promise<boolean> {
  if (isTauri) return call("analyzer_remove", { id });
  if (liveReads()) liveWrites();
  return demoAnalyzers.remove(id);
}

/** The signed analyzer catalog in use and any update waiting for review. */
export async function analyzerCatalog(): Promise<CatalogStatus | null> {
  if (isTauri) return call("analyzer_catalog");
  if (liveReads()) return null;
  return demoAnalyzers.catalog();
}

/** Download the newest signed catalog (user-initiated) and stage it for review. */
export async function fetchAnalyzerCatalog(): Promise<CatalogDiff> {
  if (isTauri) return call("analyzer_catalog_fetch");
  if (liveReads()) liveWrites();
  return demoAnalyzers.fetchCatalog();
}

export async function acceptAnalyzerCatalog(): Promise<CatalogStatus> {
  if (isTauri) return call("analyzer_catalog_accept");
  if (liveReads()) liveWrites();
  return demoAnalyzers.acceptCatalog();
}

export async function rejectAnalyzerCatalog(): Promise<void> {
  if (isTauri) return call("analyzer_catalog_reject");
  if (liveReads()) liveWrites();
  demoAnalyzers.rejectCatalog();
}

/** Allow or stop automatic runs (when items change) for an approved network analyzer. */
export async function setAnalyzerBackground(id: string, allow: boolean): Promise<AnalyzerView> {
  if (isTauri) return call("analyzer_background", { id, allow });
  if (liveReads()) liveWrites();
  return demoAnalyzers.setBackground(id, allow);
}

export async function runAnalyzer(id: string): Promise<AnalyzerResult> {
  if (isTauri) return call("analyzer_run", { id });
  if (liveReads()) liveWrites();
  const report = await agents(true);
  if (!(await analyzers()).some((a) => a.id === id && a.state === "approved")) return demoAnalyzers.run(id, report);
  // The browser demo walks the same steps the app reports, so progress can be seen and cancelled.
  demoAnalyzers.resetCancel();
  const subjects = demoAnalyzers.runSubjects(id, report);
  const total = subjects.length;
  const send = (event: RunEvent) => demoRunListeners.forEach((cb) => cb({ id, event }));
  const step = async (event: RunEvent, ms: number) => {
    send(event);
    await delay(ms);
    if (demoAnalyzers.wasCancelled) throw new Error("The run was cancelled.");
  };
  await step({ step: "preparing", index: 0, total, subject: null, agents: [] }, 150);
  for (let i = 0; i < total; i++) {
    const s = subjects[i];
    await step({ step: "copying", index: i + 1, total, subject: s.name, agents: s.agents }, 60);
    await step({ step: "scanning", index: i + 1, total, subject: s.name, agents: s.agents }, demoRunTick);
  }
  await step({ step: "collecting", index: 0, total, subject: null, agents: [] }, 120);
  return demoAnalyzers.run(id, report);
}

export interface RunProgressMessage { id: string; event: RunEvent }
const demoRunListeners = new Set<(m: RunProgressMessage) => void>();
/** `?demoRunMs=` slows the browser demo's per-item step (Playwright uses it to see progress). */
const demoRunTick = Number(new URLSearchParams(globalThis.location?.search ?? "").get("demoRunMs") ?? "") || 250;

/** Live steps of the analyzer run in progress (names only). Returns an unsubscribe. */
export async function onAnalyzerProgress(cb: (m: RunProgressMessage) => void): Promise<() => void> {
  if (isTauri) return onEvent<RunProgressMessage>("analyzer-run-progress", cb);
  demoRunListeners.add(cb);
  return () => demoRunListeners.delete(cb);
}

/** Past runs of an analyzer, newest first: when, version, counts. */
export async function analyzerHistory(id: string): Promise<RunRecord[]> {
  if (isTauri) return call("analyzer_history", { id });
  if (liveReads()) return [];
  return demoAnalyzers.history(id);
}

/** Show a scanned skill's folder in the file manager. Only folders holdmap recorded can be
 *  shown; the browser can't open folders, so it returns the folder for the caller to copy. */
export async function revealAnalyzerSubject(id: string, subject: string): Promise<{ revealed: boolean; folder: string | null }> {
  if (isTauri) {
    await call("analyzer_reveal_subject", { id, subject });
    return { revealed: true, folder: null };
  }
  return { revealed: false, folder: demoAnalyzers.subjectDir(id, subject) };
}

/** What installing an analyzer's program would do. Runs nothing. */
export async function analyzerInstallPlan(id: string): Promise<InstallPlan> {
  if (isTauri) return call("analyzer_install_plan", { id });
  if (liveReads()) liveWrites();
  return demoAnalyzers.installPlan(id);
}

let demoInstallCancelled = false;

/** Install after the user agreed to the plan; updates arrive through `on`. */
export async function installAnalyzer(id: string, on: (e: InstallEvent) => void): Promise<InstallRecord> {
  if (isTauri) {
    const off = await onEvent<{ id: string; event: InstallEvent }>("analyzer-install", (m) => { if (m.id === id) on(m.event); });
    try { return await call("analyzer_install", { id }); } finally { off(); }
  }
  if (liveReads()) liveWrites();
  demoInstallCancelled = false;
  const { demoInstall } = await import("./analyzer-install-demo");
  const record = await demoInstall(await demoAnalyzers.installPlan(id), on, () => demoInstallCancelled);
  demoAnalyzers.recordInstall(record);
  return record;
}

export async function cancelAnalyzerInstall(): Promise<void> {
  if (isTauri) return call("analyzer_install_cancel");
  demoInstallCancelled = true;
}

export async function runChangedAnalyzers(): Promise<string[]> {
  if (isTauri) return call("analyzers_run_changed");
  return [];
}

export async function insights(): Promise<Insights> {
  if (isTauri) return call("insights");
  if (liveReads()) return { results: [], open: { critical: 0, high: 0, medium: 0, low: 0, info: 0 } };
  return demoAnalyzers.insights();
}

/** The "needs you" list. In the app the Rust rules decide (shared with `holdmap needs` and
 *  MCP `list_needs`); the browser demo uses the TypeScript mirror, kept equal by a parity test. */
export async function needs(entries: PortEntry[] | null, report: AgentsReport | null, insights: Insights | null): Promise<AttentionItem[]> {
  if (isTauri) return call("needs", { entries, report, insights });
  return attention(entries, report, insights);
}

/** Store an API key an analyzer declares in the OS keychain (desktop app only). */
export async function setAnalyzerSecret(id: string, name: string, value: string): Promise<void> {
  if (isTauri) return call("analyzer_secret_set", { id, name, value });
  throw new Error("API keys are stored by the holdmap desktop app; this browser view can't keep them.");
}

/** Delete a stored API key. */
export async function deleteAnalyzerSecret(id: string, name: string): Promise<boolean> {
  if (isTauri) return call("analyzer_secret_delete", { id, name });
  return false;
}

export async function reviewFinding(fingerprint: string, state: FindingState, days?: number, reason?: string): Promise<void> {
  const note = reason?.trim() ? reason.trim().slice(0, 200) : null;
  if (isTauri) return call("review_finding", { fingerprint, state, days: days ?? null, reason: note });
  if (liveReads()) liveWrites();
  demoAnalyzers.review(fingerprint, state, note);
}
