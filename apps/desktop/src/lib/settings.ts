// Contract between the Settings panel and the app: the panel renders and validates, the app
// persists (Tauri commands) and applies. Every setter resolves when saved or rejects with a
// user-facing message.
import type { Config, DesktopHelpDestination, TrayItemId } from "./types";
import type { Density } from "./rows";

export type Theme = "system" | "light" | "dark";

/** One icon per theme. "Match system" uses sun-moon so it can't be mistaken for a device icon. */
export const THEME_ICON: Record<Theme, string> = { system: "sun-moon", light: "sun", dark: "moon" };
export const THEME_LABEL: Record<Theme, string> = { system: "match system", light: "light", dark: "dark" };

export interface HotkeyPreset { id: string; label: string }

export interface SettingsModel {
  theme: Theme;
  /** Port list row height: comfortable (44px) or compact (36px). */
  density: Density;
  /** Character shortcuts can be disabled independently of Cmd/Ctrl and arrow navigation. */
  singleKeyShortcuts: boolean;
  config: Config;
  autostart: boolean;
  /** Label of the active global shortcut, or null when off / unavailable. */
  shortcut: string | null;
  hotkeys: HotkeyPreset[];
  version: string;
  platform: string;
  configDir: string | null;
  updater?: boolean | null;
  appPath?: string | null;
  architecture?: string | null;
  /** Whether this desktop supports shutdown, or the browser can simulate its confirmation. */
  powerAvailable: boolean;
  powerDemo: boolean;
  /** Pending or executing actions in this local desktop session. */
  scheduledCount?: number;
  /** Opt-in: read agents' MCP, skills and permission settings (names only). */
  agentSettings?: boolean;
  /** Whether the desktop has holdmap's menu-bar / tray icon. */
  tray?: boolean;
  /** False when macOS hides menu-bar items for lack of room; null when that can't be told. */
  trayOnScreen?: boolean | null;
  /** The names of the menu-bar items macOS hides. */
  trayHidden?: string[];
}

export interface SettingsActions {
  setTheme(t: Theme): void;
  setDensity(d: Density): void;
  setSingleKeyShortcuts(on: boolean): void;
  setAutostart(on: boolean): Promise<void>;
  setHotkey(id: string): Promise<void>;
  setNotify(on: boolean, devOnly: boolean): Promise<void>;
  setScanInterval(secs: number): Promise<void>;
  setHistoryLimit(n: number): Promise<void>;
  clearHistory(): Promise<void>;
  copy(text: string, what: string): void;
  openDesktopHelp?(destination: DesktopHelpDestination): Promise<void>;
  /** Opens a separate preview and acknowledgement; never shuts down immediately. */
  shutdown(): void;
  /** Opens the scheduled shutdown preview, with a future time and separate acknowledgement. */
  scheduleShutdown?(): void;
  viewSchedules?(): void;
  setAgentSettings?(on: boolean): void;
  setTrayItems?(items: TrayItemId[]): Promise<void>;
  setTrayCombined?(on: boolean): Promise<void>;
  setShowInDock?(on: boolean): Promise<void>;
}

export const SETTINGS_SECTIONS = [
  { id: "general", label: "General", icon: "sliders" },
  { id: "appearance", label: "Appearance", icon: "sun" },
  { id: "notifications", label: "Notifications", icon: "bell" },
  { id: "scanning", label: "Scanning & data", icon: "radar" },
  { id: "analyzers", label: "Analyzers", icon: "shield" },
  { id: "power", label: "Power", icon: "monitor" },
  { id: "about", label: "About", icon: "info" },
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]["id"];

/** Match release download labels while retaining the native value for diagnostics. */
export function architectureLabel(architecture: string, platform: string): string {
  if (architecture === "aarch64") return platform === "macos" ? "Apple silicon (ARM64)" : "ARM64 (aarch64)";
  if (architecture === "x86_64") return platform === "macos" ? "Intel (x64)" : "x64 (x86_64)";
  return architecture;
}

/** `Config::TRAY_ITEMS` in menu-bar order, with what each shows. Linux's panel label holds text
 * only, so the icon and the bar chart are left out there. */
export const TRAY_ITEM_OPTIONS: { value: TrayItemId; label: string; description: string; text: boolean }[] = [
  { value: "icon", label: "holdmap icon", description: "With a dot when something needs you.", text: false },
  { value: "agents", label: "Agents", description: "How many are running.", text: true },
  { value: "needs", label: "Needs you", description: "How many items; amber or red when there are some.", text: true },
  { value: "cpu", label: "Agent CPU", description: "Their CPU together; orange from 60%, red from 80%.", text: true },
  { value: "dev", label: "Dev servers", description: "How many are running.", text: true },
  { value: "cpu_history", label: "CPU history", description: "A line of the agents' CPU over recent scans.", text: true },
  { value: "agent_bars", label: "Agent bars", description: "One bar of CPU per running agent.", text: false },
];

/** The items this desktop can show. */
export function trayItemOptions(platform: string): typeof TRAY_ITEM_OPTIONS {
  return platform === "linux" ? TRAY_ITEM_OPTIONS.filter((o) => o.text) : TRAY_ITEM_OPTIONS;
}

/** `items` with `item` turned on or off, in menu-bar order. None left keeps the icon, so the
 * app can still be reached (the backend does the same). */
export function nextTrayItems(items: readonly string[], item: TrayItemId, on: boolean): TrayItemId[] {
  const next = TRAY_ITEM_OPTIONS.map((o) => o.value).filter((v) => (v === item ? on : items.includes(v)));
  return next.length ? next : ["icon"];
}

/** What the tray widget is called here, or null where nothing can sit beside its icon:
 * Windows tray icons have no text (figures go in the tooltip) and some Linux desktops have no
 * tray. The browser preview shows the macOS wording. */
export function trayWidgetName(platform: string, tray: boolean | undefined): string | null {
  if (platform === "browser") return "Menu bar";
  if (!tray || platform === "windows") return null;
  return platform === "macos" ? "Menu bar" : "Panel";
}

/** Human summary of the scan cadence, e.g. "every 4 s · every 10 s in the background". */
export function cadenceText(secs: number): string {
  const s = Math.min(60, Math.max(1, Math.round(secs)));
  const bg = Math.max(10, Math.round(s * 2.5));
  return `Every ${s} s while the window is open, every ${bg} s in the background.`;
}
