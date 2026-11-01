// @vitest-environment jsdom
import { cleanup, fireEvent, render, within } from "@testing-library/svelte";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import HomeView from "../HomeView.svelte";
import PartsRail from "./PartsRail.svelte";
import PartInspector from "./PartInspector.svelte";
import Readings from "./Readings.svelte";
import { MOCK_SNAPSHOT, mockAgents } from "../../../lib/mock";
import { attention, hotAgents, needsByAgent } from "../../../lib/home";
import { agentColors } from "../../../lib/agents";
import { buildScene, readings } from "../../../lib/machine3d/scene";
import { demoActivity } from "../../../lib/activity-demo";
import type { AgentsReport } from "../../../lib/types";

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  window.matchMedia ??= ((media: string) => ({ matches: false, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  Element.prototype.scrollIntoView ??= vi.fn();
});
afterEach(cleanup);

const report = mockAgents(undefined, true);
/** What Home passes in: agents that need you themselves, and every "Needs you" title by agent. */
const attended = (r: AgentsReport | null) => { const items = attention(MOCK_SNAPSHOT.entries, r); return { hot: hotAgents(items), needs: needsByAgent(items, r) }; };
const scene = buildScene({ report, entries: MOCK_SNAPSHOT.entries, ...attended(report) });
const colors = agentColors(report.agents);
const cursor = report.agents.find((a) => a.product === "cursor")!;

describe("Home 3D layout", () => {
  it("loads on demand and shows the map, with the reason, when WebGL isn't available", async () => {
    // jsdom has no WebGL, like a webview with hardware acceleration turned off.
    const view = render(HomeView, { entries: MOCK_SNAPSHOT.entries, report, layout: "3d", onopenagent: vi.fn(), onopenentry: vi.fn(), onview: vi.fn() });
    expect(await view.findByText("3D isn't available in this window", {}, { timeout: 4000 })).toBeTruthy();
    expect(view.getByText(/WebGL 2 is turned off or unsupported here/)).toBeTruthy();
    expect(view.getByRole("group", { name: /^Machine map/ })).toBeTruthy();
    expect(within(view.getByRole("radiogroup", { name: "Home layout" })).getByRole("radio", { name: /3D/ }).getAttribute("aria-checked")).toBe("true");
  });
});

describe("3D parts list", () => {
  it("has a button for every agent, and every part of the agent in focus, by layer", async () => {
    const onagent = vi.fn(), onpart = vi.fn();
    const view = render(PartsRail, { scene, colors, focus: cursor.id, selected: null, onagent, onpart, onhover: vi.fn() });
    const nav = view.getByRole("navigation", { name: "Machine parts" });
    for (const a of scene.agents) expect(within(nav).getByRole("button", { name: new RegExp(`^${a.label} `) })).toBeTruthy();
    expect(within(nav).getByRole("button", { name: /^Codex CLI Not running/ })).toBeTruthy();
    expect(within(nav).getByRole("button", { name: /^Cursor / }).getAttribute("aria-pressed")).toBe("true");
    for (const h of ["Runtime", "This computer", "Configuration"]) expect(within(nav).getByRole("heading", { name: h })).toBeTruthy();
    const port = within(nav).getByRole("button", { name: /^Port :3000/ });
    expect(port.getAttribute("aria-haspopup")).toBe("menu");
    await fireEvent.click(port);
    expect(onpart).toHaveBeenCalledWith(expect.objectContaining({ kind: "port", label: ":3000" }), port);
    await fireEvent.click(within(nav).getByRole("button", { name: /^Docker Desktop / }));
    expect(onagent).toHaveBeenCalledWith(report.agents.find((a) => a.product === "docker-desktop")!.id);
    // Arrow keys move between the buttons.
    port.focus();
    await fireEvent.keyDown(port, { key: "ArrowDown" });
    expect(document.activeElement).not.toBe(port);
  });

  it("lists only agents until one is in focus", () => {
    const view = render(PartsRail, { scene, colors, focus: null, selected: null, onagent: vi.fn(), onpart: vi.fn(), onhover: vi.fn() });
    expect(view.queryByRole("heading", { name: "Runtime" })).toBeNull();
  });
});

describe("3D part inspector", () => {
  const part = (pred: (p: typeof scene.parts[number]) => boolean) => scene.parts.find(pred)!;

  it("says how sure each kind of part is", () => {
    const running = render(PartInspector, { part: part((p) => p.kind === "mcp" && p.live), onclose: vi.fn(), onopenagent: vi.fn(), onport: vi.fn() });
    expect(running.getByText("Running now")).toBeTruthy();
    cleanup();
    const configured = render(PartInspector, { part: part((p) => p.kind === "mcp" && !p.live), onclose: vi.fn(), onopenagent: vi.fn(), onport: vi.fn() });
    expect(configured.getByText("MCP definition found")).toBeTruthy();
    cleanup();
    const skill = render(PartInspector, { part: part((p) => p.kind === "skill"), onclose: vi.fn(), onopenagent: vi.fn(), onport: vi.fn() });
    expect(skill.getByText("Skill file found")).toBeTruthy();
    expect(skill.getByText(/doesn't prove it was loaded/)).toBeTruthy();
  });

  it("opens port actions and the agent, and has nothing to open for an agent that isn't running", async () => {
    const onport = vi.fn(), onopenagent = vi.fn();
    const port = part((p) => p.kind === "port" && p.owner === cursor.id);
    const view = render(PartInspector, { part: port, onclose: vi.fn(), onopenagent, onport });
    await fireEvent.click(view.getByRole("button", { name: "Port actions…" }));
    expect(onport).toHaveBeenCalledWith(cursor.ports[0].entry_id, expect.any(HTMLElement));
    await fireEvent.click(view.getByRole("button", { name: "Open in Agents" }));
    expect(onopenagent).toHaveBeenCalledWith(cursor.id);
    cleanup();
    const dormant = render(PartInspector, { part: part((p) => p.kind === "dormant"), onclose: vi.fn(), onopenagent: vi.fn(), onport: vi.fn() });
    expect(dormant.getByText("Configuration found")).toBeTruthy();
    expect(dormant.queryByRole("button", { name: "Open in Agents" })).toBeNull();
  });
});

describe("3D readings", () => {
  it("reads 'Not read' for figures that weren't collected, never zero", () => {
    const plain = buildScene({ report: mockAgents(), entries: MOCK_SNAPSHOT.entries });
    const view = render(Readings, { readings: readings(plain), scope: "This computer" });
    const region = view.getByRole("region", { name: "Live readings: This computer" });
    expect(within(region).getAllByText("Not read")).toHaveLength(2);
    expect(within(region).getAllByText("opt-in, off")).toHaveLength(2);
  });
});

describe("3D gauges", () => {
  it("turns readings that count parts into toggles, and leaves the rest as figures", async () => {
    const onpick = vi.fn();
    const withCalls = buildScene({ report, entries: MOCK_SNAPSHOT.entries, activity: demoActivity(), recording: { active: true, endpoint: null, started_at_ms: 0, received: 1, dropped: 0 } });
    const view = render(Readings, { readings: readings(withCalls), scope: "This computer", active: "ports", onpick });
    const region = view.getByRole("region", { name: "Live readings: This computer" });
    const ports = within(region).getByRole("button", { name: /Ports/ });
    expect(ports.getAttribute("aria-pressed")).toBe("true");
    await fireEvent.click(ports);
    expect(onpick).toHaveBeenLastCalledWith(null);
    await fireEvent.click(within(region).getByRole("button", { name: /Calls/ }));
    expect(onpick).toHaveBeenLastCalledWith("calls");
    // CPU and memory are figures, not filters.
    expect(within(region).queryByRole("button", { name: /CPU|Memory/ })).toBeNull();
  });

  it("never offers a toggle for a figure that wasn't collected or isn't in yet", () => {
    const plain = buildScene({ report: mockAgents(), entries: MOCK_SNAPSHOT.entries });
    const view = render(Readings, { readings: readings(plain), scope: "This computer", onpick: vi.fn() });
    expect(view.queryByRole("button", { name: /MCP running|Skills/ })).toBeNull();
    cleanup();
    const waiting = render(Readings, { readings: readings(buildScene({ report: null }), null, false), scope: "This computer", onpick: vi.fn() });
    expect(waiting.queryAllByRole("button")).toHaveLength(0);
    expect(waiting.getAllByText("…")).toHaveLength(7);
  });

  it("describes a recorded call without offering to open an agent", () => {
    const call = buildScene({ report, activity: demoActivity() }).parts.find((p) => p.kind === "call")!;
    const view = render(PartInspector, { part: call, onclose: vi.fn(), onopenagent: vi.fn(), onport: vi.fn() });
    expect(view.getByText("Reported")).toBeTruthy();
    expect(view.getByText(/isn't tied to a process or a configured server/)).toBeTruthy();
    expect(view.queryByRole("button", { name: "Open in Agents" })).toBeNull();
  });
});
