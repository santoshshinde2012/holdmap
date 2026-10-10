// Contract between the Settings panel and the app: the panel renders and validates, the app
// persists (Tauri commands) and applies. Every setter resolves when saved or rejects with a
// user-facing message.
import type { Config } from "./types";
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
  config: Config;
  autostart: boolean;
  /** Label of the active global shortcut, or null when off / unavailable. */
  shortcut: string | null;
  hotkeys: HotkeyPreset[];
  version: string;
  platform: string;
  configDir: string | null;
  /** Whether this desktop supports shutdown, or the browser can simulate its confirmation. */
  powerAvailable: boolean;
  powerDemo: boolean;
}

export interface SettingsActions {
  setTheme(t: Theme): void;
  setDensity(d: Density): void;
  setAutostart(on: boolean): Promise<void>;
  setHotkey(id: string): Promise<void>;
  setNotify(on: boolean, devOnly: boolean): Promise<void>;
  setScanInterval(secs: number): Promise<void>;
  setHistoryLimit(n: number): Promise<void>;
  clearHistory(): Promise<void>;
  copy(text: string, what: string): void;
  /** Opens a separate preview and acknowledgement; never shuts down immediately. */
  shutdown(): void;
}

export const SETTINGS_SECTIONS = [
  { id: "general", label: "General", icon: "sliders" },
  { id: "appearance", label: "Appearance", icon: "sun" },
  { id: "notifications", label: "Notifications", icon: "bell" },
  { id: "scanning", label: "Scanning & data", icon: "radar" },
  { id: "power", label: "Power", icon: "monitor" },
  { id: "about", label: "About", icon: "info" },
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]["id"];

/** Human summary of the scan cadence, e.g. "every 4 s · every 10 s in the background". */
export function cadenceText(secs: number): string {
  const s = Math.min(60, Math.max(1, Math.round(secs)));
  const bg = Math.max(10, Math.round(s * 2.5));
  return `Every ${s} s while the window is open, every ${bg} s in the background.`;
}
