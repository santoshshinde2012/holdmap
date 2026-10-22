// Contract between the Settings panel and the app: the panel renders and validates, the app
// persists (Tauri commands) and applies. Every setter resolves when saved or rejects with a
// user-facing message.
import type { Config, DesktopHelpDestination } from "./types";
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

/** Human summary of the scan cadence, e.g. "every 4 s · every 10 s in the background". */
export function cadenceText(secs: number): string {
  const s = Math.min(60, Math.max(1, Math.round(secs)));
  const bg = Math.max(10, Math.round(s * 2.5));
  return `Every ${s} s while the window is open, every ${bg} s in the background.`;
}
