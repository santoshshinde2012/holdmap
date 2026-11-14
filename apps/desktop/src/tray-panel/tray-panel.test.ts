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
    const tile = (label: string) => view.getByText(label, { selector: ".label" }).parentElement!;
    expect(tile("Agents").querySelector(".value")?.textContent).toBe("4");
    expect(tile("Agents").querySelector(".sub")?.textContent).toBe("+1 dev tool");
    expect(tile("RAM").querySelector(".value")?.textContent).toBe("1.6GB");
    expect(tile("CPU").querySelector(".value")?.textContent).toBe("7.4%");
    expect(view.container.querySelector(".head .status")?.textContent).toBe("2 need you");
    expect(view.getByRole("img", { name: /Agent CPU over the last 48 scans/ })).toBeTruthy();
    expect(view.getByRole("img", { name: /Something needed you in \d+ of the last 48 scans/ })).toBeTruthy();
    expect(view.getByText("Look now")).toBeTruthy();
    expect(view.getByText("Worth a look")).toBeTruthy();
    expect(view.getByRole("button", { name: /:5173 Vite · dashboard Network/ })).toBeTruthy();
    expect(view.getByRole("button", { name: "Claude Code · storefront CPU 4.2% RAM 288 MB" })).toBeTruthy();
    // Most RAM first; a long folder name truncates and the full row is in the tooltip.
    const rows = view.getAllByRole("button").filter((b) => b.classList.contains("trow"));
    expect(rows.map((r) => r.querySelector("b")?.textContent ?? r.querySelector(".name")?.textContent)).toEqual(["Cursor", "Claude Code", "Codex CLI", "Gemini CLI", "Docker Desktop"]);
    expect(rows[0].getAttribute("title")).toBe("Cursor · storefront-monorepo-with-a-long-name: 1.1 GB RAM, CPU 2.1%");
    // Docker Desktop is a developer tool, listed apart and said so.
    expect(view.getByText("Developer tools")).toBeTruthy();
    expect(rows[4].getAttribute("title")).toContain("(developer tool)");
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
    expect(view.getByText("Agents", { selector: ".label" }).parentElement!.querySelector(".value")?.textContent).toBe("…");
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
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
      const section = this.getAttribute("data-section");
      if (section) scrolled.push(section);
      return { top: 0 } as DOMRect;
    });
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

describe("Tray panel edge cases", () => {
  it("says it is offline, with a retry, instead of checking forever", async () => {
    let fail = true;
    const load = vi.fn(async () => { if (fail) throw new Error("no backend"); return demoPanel(); });
    const view = render(TrayPanel, { load, run: vi.fn(), poll: 60_000 });
    await waitFor(() => expect(view.container.querySelector(".head .status")?.textContent).toBe("Can't read holdmap"));
    expect(view.container.textContent).not.toContain("Checking agents and ports…");
    expect(view.container.textContent).not.toContain("Looking for agents…");
    fail = false;
    await fireEvent.click(view.getByRole("button", { name: "Try now" }));
    await waitFor(() => expect(view.container.querySelector(".head .status")?.textContent).toBe("2 need you"));
    expect(view.queryByRole("button", { name: "Try now" })).toBeNull();
  });

  it("calls zero agents zero, and lists many agents with the rest behind one link", async () => {
    const zero = mount({ ...demoPanel(), agents: 0, topAgents: [], tools: [], moreAgents: 0 }).view;
    await waitFor(() => expect(zero.getByText("No agents running.")).toBeTruthy());
    cleanup();
    const many = Array.from({ length: 5 }, (_, i) => ({ name: `Agent ${i} with a very long product name indeed`, place: `a-very-long-folder-name-number-${i}`, cpu: i, memory: (5 - i) << 20, action: `agent:${i}` }));
    const { view, run } = mount({ ...demoPanel(), agents: 25, topAgents: many, moreAgents: 20 });
    await waitFor(() => expect(view.getByRole("button", { name: "+20 more in holdmap" })).toBeTruthy());
    const rows = view.getAllByRole("button").filter((b) => b.classList.contains("trow") && !b.classList.contains("tool"));
    expect(rows).toHaveLength(5);
    // The full name and folder stay in the tooltip when the row truncates them.
    expect(rows[0].getAttribute("title")).toBe("Agent 0 with a very long product name indeed · a-very-long-folder-name-number-0: 5.0 MB RAM, CPU 0.0%");
    await fireEvent.click(view.getByRole("button", { name: "+20 more in holdmap" }));
    expect(run).toHaveBeenCalledWith("show");
  });
});
