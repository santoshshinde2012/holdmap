// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import PinDialog from "./PinDialog.svelte";
import HistoryPanel from "./HistoryPanel.svelte";
import SettingsDialog from "./SettingsDialog.svelte";
import CopyValue from "./ui/CopyValue.svelte";
import CodeBlock from "./detail/CodeBlock.svelte";
import type { HistoryEntry } from "../lib/types";
import type { SettingsActions, SettingsModel } from "../lib/settings";

beforeAll(() => {
  window.matchMedia ??= ((media: string) => ({ matches: true, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  const animate = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(animate);
  Element.prototype.scrollIntoView ??= vi.fn();
});
afterEach(cleanup);

const entry: HistoryEntry = { at_ms: 1000, port: 3000, protocol: "tcp", label: "node", command: ["node", "server.js"], cwd: "/tmp", project: null, framework: null, pid: 123 };
const model: SettingsModel = { theme: "dark", density: "comfortable", singleKeyShortcuts: true, config: { pins: [], notify: true, notify_dev_only: true, history_limit: 100, scan_interval_secs: 4, hotkey: "off", recent_hosts: [] }, autostart: false, shortcut: null, hotkeys: [], version: "test", platform: "macos", configDir: null, powerAvailable: true, powerDemo: false };
const settingsActions: SettingsActions = { setTheme() {}, setDensity() {}, setSingleKeyShortcuts() {}, async setAutostart() {}, async setHotkey() {}, async setNotify() {}, async setScanInterval() {}, async setHistoryLimit() {}, async clearHistory() {}, copy() {}, shutdown() {} };

describe("settings keyboard navigation", () => {
  it("changes sections only for unmodified navigation outside composition", async () => {
    const view = render(SettingsDialog, { model, actions: settingsActions, onclose() {} });
    const general = view.getByRole("tab", { name: "General" });
    general.focus();
    for (const extra of [{ isComposing: true }, { keyCode: 229 }, { altKey: true }, { ctrlKey: true }, { metaKey: true }]) {
      await fireEvent.keyDown(general, { key: "End", ...extra });
    }
    expect(general.getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(general);
    await fireEvent.keyDown(general, { key: "End" });
    expect(view.getByRole("tab", { name: "About" }).getAttribute("aria-selected")).toBe("true");
  });
});

describe("history action availability", () => {
  it("keeps simultaneous ports, protocols and duplicate records from the same stopped process distinct", async () => {
    const records: HistoryEntry[] = [entry, { ...entry, port: 3001 }, { ...entry, protocol: "udp" }, { ...entry }];
    const onrestart = vi.fn().mockResolvedValue(undefined);
    const view = render(HistoryPanel, { items: records, onrestart, oncopy() {}, onclear() {}, onclose() {} });
    const buttons = view.getAllByRole("button", { name: "Restart" });
    expect(buttons).toHaveLength(4);
    await fireEvent.click(buttons[1]);
    expect(onrestart).toHaveBeenCalledWith(records[1]);
    await fireEvent.click(buttons[2]);
    expect(onrestart).toHaveBeenLastCalledWith(records[2]);
  });

  it("honors backend refusals and associates each disabled action with a visible explanation", async () => {
    const oncopy = vi.fn(), onrestart = vi.fn();
    const view = render(HistoryPanel, { items: [{ ...entry, command: ["C:\\Windows\\System32\\cmd.exe", "/C", "fragmented shell text"], restartable: false, copyable: false }], platform: "windows", oncopy, onrestart, onclear() {}, onclose() {} });
    for (const name of ["Copy command", "Restart"]) {
      const button = view.getByRole("button", { name }) as HTMLButtonElement;
      expect(button.disabled).toBe(true);
      const reason = document.getElementById(button.getAttribute("aria-describedby")!);
      expect(reason?.textContent).toContain(name === "Restart" ? "Start the service normally" : "cannot reproduce the original command");
      await fireEvent.click(button);
    }
    expect(oncopy).not.toHaveBeenCalled();
    expect(onrestart).not.toHaveBeenCalled();
  });

  it.each(["cmd", "C:\\Windows\\System32\\CMD.EXE"])("fails safely for older Windows %s recordings without availability flags", async (program) => {
    const oncopy = vi.fn(), onrestart = vi.fn();
    const view = render(HistoryPanel, { items: [{ ...entry, command: [program, "/C", "parsed shell text"] }], platform: "windows", oncopy, onrestart, onclear() {}, onclose() {} });
    for (const name of ["Copy command", "Restart"]) {
      const button = view.getByRole("button", { name }) as HTMLButtonElement;
      expect(button.disabled).toBe(true);
      await fireEvent.click(button);
    }
    expect(oncopy).not.toHaveBeenCalled();
    expect(onrestart).not.toHaveBeenCalled();
    expect(view.container.textContent).toContain("Original Windows shell syntax was not recorded");
  });

  it("allows backend restart for captured native shell syntax while keeping copy unavailable", async () => {
    const recorded = { ...entry, command: ["cmd.exe", "/C", "redacted display text"], restartable: true, copyable: false };
    const oncopy = vi.fn(), onrestart = vi.fn().mockResolvedValue(undefined);
    const view = render(HistoryPanel, { items: [recorded], platform: "windows", oncopy, onrestart, onclear() {}, onclose() {} });
    expect((view.getByRole("button", { name: "Copy command" }) as HTMLButtonElement).disabled).toBe(true);
    const restart = view.getByRole("button", { name: "Restart" }) as HTMLButtonElement;
    expect(restart.disabled).toBe(false);
    expect(restart.getAttribute("aria-describedby")).toBeNull();
    await fireEvent.click(restart);
    await waitFor(() => expect(onrestart).toHaveBeenCalledWith(recorded));
    expect(oncopy).not.toHaveBeenCalled();
  });

  it("preserves ordinary legacy argv, executable case and shell quoting when flags are absent", async () => {
    const recorded = { ...entry, command: ["Node", "server.js", "a&b"], cwd: "/tmp/a b" };
    const oncopy = vi.fn(), onrestart = vi.fn().mockResolvedValue(undefined);
    const view = render(HistoryPanel, { items: [recorded], platform: "macos", oncopy, onrestart, onclear() {}, onclose() {} });
    await fireEvent.click(view.getByRole("button", { name: "Copy command" }));
    expect(oncopy).toHaveBeenCalledWith("cd -- '/tmp/a b' && 'Node' server.js 'a&b'", "Command");
    await fireEvent.click(view.getByRole("button", { name: "Restart" }));
    expect(onrestart).toHaveBeenCalledWith(recorded);
    expect(view.container.querySelector(".unavailable")).toBeNull();
  });

  it("keeps ordinary Windows argv available without flags and preserves executable case", async () => {
    const recorded = { ...entry, command: ["C:\\Program Files\\Node.EXE", "server.js"], cwd: "C:\\Work" };
    const oncopy = vi.fn(), onrestart = vi.fn().mockResolvedValue(undefined);
    const view = render(HistoryPanel, { items: [recorded], platform: "windows", oncopy, onrestart, onclear() {}, onclose() {} });
    await fireEvent.click(view.getByRole("button", { name: "Copy command" }));
    expect(oncopy.mock.calls[0][0]).toContain("'C:\\Program Files\\Node.EXE'");
    await fireEvent.click(view.getByRole("button", { name: "Restart" }));
    expect(onrestart).toHaveBeenCalledWith(recorded);
    expect(view.container.querySelector(".unavailable")).toBeNull();
  });

  it.each([{ command: [] }, { command: [""] }])("does not offer actions for an empty executable even with stale true flags: $command", async ({ command }) => {
    const view = render(HistoryPanel, { items: [{ ...entry, command, restartable: true, copyable: true }], oncopy() {}, onrestart() {}, onclear() {}, onclose() {} });
    expect((view.getByRole("button", { name: "Copy command" }) as HTMLButtonElement).disabled).toBe(true);
    expect((view.getByRole("button", { name: "Restart" }) as HTMLButtonElement).disabled).toBe(true);
    expect(view.container.textContent).toContain("No restart command was recorded. Start the service using its normal launcher.");
  });

  it("refreshes action availability and removes stale explanations with a newer backend recording", async () => {
    const props = { oncopy() {}, onrestart() {}, onclear() {}, onclose() {} };
    const view = render(HistoryPanel, { ...props, items: [{ ...entry, restartable: false, copyable: false }] });
    await view.rerender({ ...props, items: [{ ...entry, restartable: true, copyable: true }] });
    expect((view.getByRole("button", { name: "Copy command" }) as HTMLButtonElement).disabled).toBe(false);
    expect((view.getByRole("button", { name: "Restart" }) as HTMLButtonElement).disabled).toBe(false);
    expect(view.container.querySelector(".unavailable")).toBeNull();
  });
});

describe("native write failures", () => {
  it("contains failed unpin writes, prevents duplicate submissions, and allows retry", async () => {
    let reject!: (error: Error) => void;
    const onunpin = vi.fn().mockImplementationOnce(() => new Promise<void>((_, fail) => { reject = fail; })).mockResolvedValue(undefined);
    const onsave = vi.fn(), onclose = vi.fn();
    const view = render(PinDialog, { port: 3000, pinned: true, inUse: () => null, onsave, onunpin, onclose });
    await fireEvent.click(view.getByRole("button", { name: "Unpin" }));
    expect((view.getByRole("button", { name: "Unpinning…" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(view.getByRole("button", { name: "Unpinning…" }));
    await fireEvent.submit(view.container.querySelector("form")!);
    expect(onunpin).toHaveBeenCalledOnce();
    expect(onsave).not.toHaveBeenCalled();
    reject(new Error("Pin settings are read-only"));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Pin settings are read-only"));
    expect(onclose).not.toHaveBeenCalled();
    await fireEvent.click(view.getByRole("button", { name: "Unpin" }));
    await waitFor(() => expect(onclose).toHaveBeenCalledOnce());
    expect(onunpin).toHaveBeenCalledTimes(2);
  });

  it("retains history and its confirmation after a failed clear, then supports retry", async () => {
    const onclear = vi.fn().mockRejectedValueOnce(new Error("History file is read-only")).mockResolvedValue(undefined);
    const view = render(HistoryPanel, { items: [entry], onclear, onrestart() {}, onclose() {}, oncopy() {} });
    await fireEvent.click(view.getByRole("button", { name: "Clear history…" }));
    await fireEvent.click(view.getByRole("button", { name: "Clear history" }));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("History file is read-only"));
    expect(view.container.textContent).toContain("server.js");
    expect(view.container.textContent).toContain("Clear all 1 entries?");
    await fireEvent.click(view.getByRole("button", { name: "Clear history" }));
    await waitFor(() => expect(view.queryByRole("button", { name: "Clear history" })).toBeNull());
    expect(onclear).toHaveBeenCalledTimes(2);
  });

  it("renders the settings history-clear failure beside the action", async () => {
    const view = render(SettingsDialog, { model, section: "scanning", actions: { ...settingsActions, clearHistory: vi.fn().mockRejectedValue(new Error("History file is read-only")) }, onclose() {} });
    await fireEvent.click(view.getByRole("button", { name: "Clear history…" }));
    await fireEvent.click(view.getByRole("button", { name: "Clear" }));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("History file is read-only"));
    expect(view.container.textContent).not.toContain("History cleared");
  });

  it("restores the actual launch-at-login value after a rejected write", async () => {
    let reject!: (error: Error) => void;
    const setAutostart = vi.fn(() => new Promise<void>((_, fail) => { reject = fail; }));
    const view = render(SettingsDialog, { model, actions: { ...settingsActions, setAutostart }, onclose() {} });
    await fireEvent.click(view.getByRole("switch", { name: "Launch at login" }));
    expect((view.getByRole("switch", { name: "Launch at login" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(view.getByRole("switch", { name: "Launch at login" }));
    expect(setAutostart).toHaveBeenCalledOnce();
    reject(new Error("Login permission denied"));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Login permission denied"));
    expect(view.getByRole("switch", { name: "Launch at login" }).getAttribute("aria-checked")).toBe("false");
  });

  it.each(["Notify me", "Only dev servers and pinned ports"])("restores %s after a rejected notification write", async (name) => {
    const setNotify = vi.fn().mockRejectedValue(new Error("Notification settings are read-only"));
    const view = render(SettingsDialog, { model, section: "notifications", actions: { ...settingsActions, setNotify }, onclose() {} });
    await fireEvent.click(name === "Notify me" ? view.getByRole("switch", { name }) : view.getByRole("checkbox", { name }));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Notification settings are read-only"));
    expect(view.getByRole("switch", { name: "Notify me" }).getAttribute("aria-checked")).toBe("true");
    expect((view.getByRole("checkbox", { name: "Only dev servers and pinned ports" }) as HTMLInputElement).checked).toBe(true);
  });

  it("restores the active global shortcut after a rejected registration", async () => {
    const setHotkey = vi.fn().mockRejectedValue(new Error("Shortcut already registered"));
    const settingsModel = { ...model, hotkeys: [{ id: "off", label: "Off" }, { id: "alt-p", label: "Alt+P" }] };
    const view = render(SettingsDialog, { model: settingsModel, actions: { ...settingsActions, setHotkey }, onclose() {} });
    await fireEvent.click(view.getByRole("combobox", { name: "Global shortcut: Off" }));
    await fireEvent.click(view.getByRole("option", { name: "Alt+P" }));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Shortcut already registered"));
    expect(view.getByRole("combobox", { name: "Global shortcut: Off" })).toBeTruthy();
  });

  it("restores a rejected keyboard preference and shows its failure", async () => {
    const setSingleKeyShortcuts = vi.fn(() => { throw new Error("Preferences are read-only"); });
    const view = render(SettingsDialog, { model, actions: { ...settingsActions, setSingleKeyShortcuts }, onclose() {} });
    await fireEvent.click(view.getByRole("switch", { name: "Single-key shortcuts" }));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Preferences are read-only"));
    expect(view.getByRole("switch", { name: "Single-key shortcuts" }).getAttribute("aria-checked")).toBe("true");
    await fireEvent.click(view.getByRole("switch", { name: "Single-key shortcuts" }));
    expect(setSingleKeyShortcuts).toHaveBeenCalledTimes(2);
    expect(view.getByRole("switch", { name: "Single-key shortcuts" }).getAttribute("aria-checked")).toBe("true");
  });

  it.each([["Light", "setTheme", "Dark"], ["Compact", "setDensity", "Comfortable"]] as const)("restores a rejected %s appearance setting", async (choice, action, previous) => {
    const setter = vi.fn(() => { throw new Error("Preferences are read-only"); });
    const view = render(SettingsDialog, { model, section: "appearance", actions: { ...settingsActions, [action]: setter }, onclose() {} });
    await fireEvent.click(view.getByRole("radio", { name: choice }));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Preferences are read-only"));
    expect(view.getByRole("radio", { name: previous }).getAttribute("aria-checked")).toBe("true");
  });
});

type Copy = (text: string, what: string) => boolean | void | Promise<boolean | void>;
const copyControls = [
  { name: "value", mount: (oncopy: Copy) => render(CopyValue, { value: "node server.js", what: "Command", oncopy }) },
  { name: "code block", mount: (oncopy: Copy) => render(CodeBlock, { code: "node server.js", what: "Command", oncopy }) },
];
for (const control of copyControls) describe(`${control.name} clipboard feedback`, () => {
  it("waits for clipboard completion before announcing success", async () => {
    let resolve!: () => void;
    const oncopy = vi.fn(() => new Promise<void>((done) => { resolve = done; }));
    const view = control.mount(oncopy);
    await fireEvent.click(view.getByRole("button", { name: "Copy command" }));
    expect(view.queryByRole("button", { name: "Command copied" })).toBeNull();
    expect((view.getByRole("button", { name: "Copy command" }) as HTMLButtonElement).disabled).toBe(true);
    resolve();
    await waitFor(() => expect(view.getByRole("button", { name: "Command copied" })).toBeTruthy());
  });

  it("contains clipboard rejection and never announces copied", async () => {
    const view = control.mount(vi.fn().mockRejectedValue(new Error("Clipboard permission denied")));
    await fireEvent.click(view.getByRole("button", { name: "Copy command" }));
    await waitFor(() => expect(view.getByRole("button", { name: "Command could not be copied" })).toBeTruthy());
    expect(view.queryByRole("button", { name: "Command copied" })).toBeNull();
  });

  it("preserves failure reported by a parent that already handled the error", async () => {
    const view = control.mount(vi.fn().mockResolvedValue(false));
    await fireEvent.click(view.getByRole("button", { name: "Copy command" }));
    await waitFor(() => expect(view.getByRole("button", { name: "Command could not be copied" })).toBeTruthy());
    expect(view.queryByRole("button", { name: "Command copied" })).toBeNull();
  });
});

describe("agent settings opt-in", () => {
  it("can be turned off later in Settings, as the first-run dialog promises", async () => {
    const setAgentSettings = vi.fn();
    const view = render(SettingsDialog, { model: { ...model, agentSettings: true }, section: "scanning", actions: { ...settingsActions, setAgentSettings }, onclose() {} });
    const toggle = view.getByRole("switch", { name: "Read agent settings" });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    await fireEvent.click(toggle);
    await waitFor(() => expect(setAgentSettings).toHaveBeenCalledWith(false));
  });
});
