// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import TrayPanel from "./TrayPanel.svelte";
import { demoPanel, type PanelData } from "../lib/tray-panel";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const mount = (data: PanelData = demoPanel(), run = vi.fn()) =>
  ({ view: render(TrayPanel, { load: async () => data, run, poll: 60_000 }), run });

describe("Tray panel", () => {
  it("shows the headline readings, history and lists", async () => {
    const { view } = mount();
    await waitFor(() => expect(view.getByText("Claude Code listens on all interfaces")).toBeTruthy());
    expect(view.getByRole("heading", { name: "holdmap" })).toBeTruthy();
    expect(view.getByText("Agents").previousElementSibling?.textContent).toBe("4");
    expect(view.getByText("Dev servers", { selector: ".legend" }).previousElementSibling?.textContent).toBe("3");
    expect(view.getByRole("img", { name: /Agent CPU over the last 48 scans/ })).toBeTruthy();
    expect(view.getByRole("img", { name: /Something needed you in \d+ of the last 48 scans/ })).toBeTruthy();
    expect(view.getByText("Look now")).toBeTruthy();
    expect(view.getByText("Worth a look")).toBeTruthy();
    expect(view.getByRole("button", { name: /:5173 Vite · dashboard Network/ })).toBeTruthy();
    expect(view.getByRole("button", { name: "Claude Code · storefront CPU 4.2% memory 288 MB" })).toBeTruthy();
  });

  it("runs each row's tray action, and Escape closes the panel", async () => {
    const { view, run } = mount();
    await waitFor(() => expect(view.getByText("Claude Code listens on all interfaces")).toBeTruthy());
    await fireEvent.click(view.getByRole("button", { name: /Claude Code listens on all interfaces/ }));
    await fireEvent.click(view.getByRole("button", { name: /:3000/ }));
    await fireEvent.click(view.getByRole("button", { name: /^Cursor/ }));
    await fireEvent.click(view.getByRole("button", { name: "Stop all…" }));
    await fireEvent.click(view.getAllByRole("button", { name: "Open holdmap" })[0]);
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(run.mock.calls.map(([id]) => id)).toEqual([
      "need:0:agent:agent:4242", "focus:3000", "agent:agent:811", "stop-all-dev", "show", "hide-panel",
    ]);
  });

  it("says what it doesn't know yet instead of showing zeros", async () => {
    const empty: PanelData = {
      ...demoPanel(), ready: false, agents: null, cpu: null, memory: null, cpuHistory: [], attentionHistory: [],
      needs: null, dev: [], stopDev: false, topAgents: [],
    };
    const { view } = mount(empty);
    await waitFor(() => expect(view.getByText("Checking agents and ports…")).toBeTruthy());
    expect(view.getByText("Fills in as holdmap scans")).toBeTruthy();
    expect(view.getByText("Agents").previousElementSibling?.textContent).toBe("…");
    expect(view.queryByRole("button", { name: "Stop all…" })).toBeNull();
    expect(view.queryByText("0")).toBeNull();
  });

  it("calls a quiet machine all clear", async () => {
    const { view } = mount({ ...demoPanel(), needs: [], exposed: 0, dev: [], stopDev: false });
    await waitFor(() => expect(view.getByText("All clear")).toBeTruthy());
    expect(view.getByText("None")).toBeTruthy();
    expect(view.getByText("No dev servers running.")).toBeTruthy();
  });
});
