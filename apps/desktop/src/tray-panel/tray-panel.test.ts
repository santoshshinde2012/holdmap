// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import TrayPanel from "./TrayPanel.svelte";
import { demoAction, demoPanel, type PanelData } from "../lib/tray-panel";

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
      needs: null, dev: [], stopDev: false, topAgents: [], churn: [],
    };
    const { view } = mount(empty);
    await waitFor(() => expect(view.getByText("Checking agents and ports…")).toBeTruthy());
    // Both charts wait for scans: the agents' CPU and port changes.
    expect(view.getAllByText("Fills in as holdmap scans")).toHaveLength(2);
    expect(view.getByText("Agents").previousElementSibling?.textContent).toBe("…");
    expect(view.queryByRole("button", { name: "Stop all…" })).toBeNull();
    expect(view.queryByText("0")).toBeNull();
  });

  it("keeps itself open with the pin, and shows the pin's state", async () => {
    let pinned = false;
    const run = vi.fn((id: string) => { if (id === "pin-panel") pinned = !pinned; });
    const view = render(TrayPanel, { load: async () => ({ ...demoPanel(), pinned }), run, poll: 60_000 });
    const pin = await view.findByRole("button", { name: "Keep open" });
    expect(pin.getAttribute("aria-pressed")).toBe("false");
    await fireEvent.click(pin);
    expect(run).toHaveBeenCalledWith("pin-panel");
    await waitFor(() => expect(view.getByRole("button", { name: "Keep open" }).getAttribute("aria-pressed")).toBe("true"));
  });

  it("opens at the section of the menu-bar item clicked, once per click", async () => {
    const scrolled: string[] = [];
    Element.prototype.scrollIntoView = function (this: Element) { scrolled.push(this.getAttribute("data-section") ?? ""); };
    let data = { ...demoPanel(), focus: "dev", focusSeq: 1 };
    const view = render(TrayPanel, { load: async () => data, run: vi.fn(), poll: 20 });
    await waitFor(() => expect(scrolled).toEqual(["dev"]));
    // Polls with the same click don't scroll again; a new click does.
    await new Promise((r) => setTimeout(r, 60));
    expect(scrolled).toEqual(["dev"]);
    data = { ...data, focus: "needs", focusSeq: 2 };
    await waitFor(() => expect(scrolled).toEqual(["dev", "needs"]));
    view.unmount();
  });

  it("shows the ports ring and port changes", async () => {
    const { view } = mount();
    await waitFor(() => expect(view.getByRole("img", { name: "33 on this computer only, 4 open to the network" })).toBeTruthy());
    expect(view.getByRole("img", { name: /^In the last 48 scans: \d+ ports opened, \d+ closed\.$/ })).toBeTruthy();
  });

  it("calls a quiet machine all clear", async () => {
    const { view } = mount({ ...demoPanel(), needs: [], exposed: 0, dev: [], stopDev: false });
    await waitFor(() => expect(view.getByText("All clear")).toBeTruthy());
    expect(view.getByText("None")).toBeTruthy();
    expect(view.getByText("No dev servers running.")).toBeTruthy();
  });
});


describe("Tray quick settings", () => {
  it("updates switches from the returned settings and restores overview with Escape", async () => {
    let data = demoPanel();
    const run = vi.fn((id: string) => { data = demoAction(data, id); });
    const view = render(TrayPanel, { load: async () => data, run, poll: 60_000 });
    await view.findByText("Claude Code listens on all interfaces");
    await fireEvent.click(view.getByRole("button", { name: /Quick settings/ }));
    const toggle = view.getByRole("switch", { name: "Desktop notifications" });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    await fireEvent.click(toggle);
    await waitFor(() => expect(toggle.getAttribute("aria-checked")).toBe("false"));
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(view.queryByRole("region", { name: "Quick settings" })).toBeNull();
    expect(run).not.toHaveBeenCalledWith("hide-panel");
  });

  it("reports rejected actions without claiming the switch changed", async () => {
    const { view } = mount(demoPanel(), vi.fn().mockRejectedValue(new Error("save failed")));
    await view.findByText("Claude Code listens on all interfaces");
    await fireEvent.click(view.getByRole("button", { name: /Quick settings/ }));
    await fireEvent.click(view.getByRole("switch", { name: "Desktop notifications" }));
    expect(await view.findByRole("alert")).toHaveProperty("textContent", "Couldn’t save that setting. Please try again.");
    expect(view.getByRole("switch", { name: "Desktop notifications" }).getAttribute("aria-checked")).toBe("true");
  });
});
