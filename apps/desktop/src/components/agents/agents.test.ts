// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import AgentCard from "./AgentCard.svelte";
import AgentDetails from "./AgentDetails.svelte";
import AgentsView from "../AgentsView.svelte";
import { MOCK_SNAPSHOT } from "../../lib/mock";
import { mockAgents } from "../../lib/mock-agents";
import type { Agent } from "../../lib/types";

beforeAll(() => {
  Element.prototype.scrollIntoView ??= () => {};
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  window.matchMedia ??= ((media: string) => ({ matches: false, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} }));
});
afterEach(() => { cleanup(); localStorage.clear(); });

const actions = { onreveal() {}, oneditor() {}, onstop() {}, onstopall() {} };
const detailProps = (agent: Agent) => ({ agent, report: mockAgents(), entries: MOCK_SNAPSHOT.entries, color: "var(--accent)", onentry() {}, ...actions });
const props = (agent: Agent, selected = false) => ({ agent, selected, report: mockAgents(), entries: MOCK_SNAPSHOT.entries, color: "var(--accent)", onselect() {}, onentry() {}, ...actions });

describe("agent visibility", () => {
  it("opens the shared folder chooser from the card name/path while preserving quick actions", async () => {
    const a = mockAgents().agents[1], folder = a.folders[0];
    const onfolder = vi.fn(), onreveal = vi.fn(), oneditor = vi.fn();
    const { getByRole } = render(AgentDetails, { ...detailProps(a), onfolder, onreveal, oneditor });
    await fireEvent.click(getByRole("button", { name: `Folder actions for ${folder.path}` }));
    expect(onfolder).toHaveBeenCalledExactlyOnceWith(folder.path);
    await fireEvent.click(getByRole("group", { name: `Open ${folder.label}` }).querySelector("button[title='Open in file manager']")!);
    expect(onreveal).toHaveBeenCalledExactlyOnceWith(folder.path);
    await fireEvent.click(getByRole("group", { name: `Open ${folder.label}` }).querySelector("button[title='Open in editor']")!);
    expect(oneditor).toHaveBeenCalledExactlyOnceWith(folder.path);
  });

  it("prevents repeated card quick actions while a folder is opening", async () => {
    const a = mockAgents().agents[1], folder = a.folders[0];
    let resolve!: () => void;
    const onreveal = vi.fn(() => new Promise<void>((done) => (resolve = done)));
    const oneditor = vi.fn();
    const { getByRole } = render(AgentDetails, { ...detailProps(a), onreveal, oneditor });
    const group = getByRole("group", { name: `Open ${folder.label}` });
    const reveal = group.querySelector("button[title='Open in file manager']")!;
    const editor = group.querySelector("button[title='Open in editor']")!;
    await fireEvent.click(reveal);
    await fireEvent.click(reveal);
    await fireEvent.click(editor);
    expect(onreveal).toHaveBeenCalledOnce();
    expect(oneditor).not.toHaveBeenCalled();
    resolve();
  });

  it("shows whole-agent resources and all owned process totals before expansion", () => {
    const a = mockAgents().agents[1];
    const { container } = render(AgentCard, props(a));
    expect(container.textContent).toContain("15 processes");
    expect(container.textContent).toContain("5 tools");
    expect(container.textContent).toContain("6.2% CPU");
    expect(container.querySelector(".res-sub")?.textContent).toBe("RAM");
    expect(container.querySelector(".body")).toBeNull();
  });

  it("renders inferred MCP without a port and preserves tool command expansion under polling", async () => {
    const a = mockAgents().agents[1];
    const { container, rerender } = render(AgentDetails, detailProps(a));
    expect(container.textContent).toContain("Filesystem MCP");
    expect(container.textContent).toContain("MCP server · inferred · no listening port");
    const details = container.querySelector("section[aria-label='Agent tools'] details") as HTMLDetailsElement;
    await fireEvent.click(details.querySelector("summary")!);
    // jsdom does not implement native details activation.
    details.open = true;
    await rerender(detailProps({ ...structuredClone(a), cpu_percent: 12.3 }));
    expect(container.textContent).toContain("12% CPU");
    expect(container.querySelector("section[aria-label='Agent tools'] details")).toBe(details);
    expect(details.open).toBe(true);
  });

  it("shows unknown memory counters instead of a zero", () => {
    const a = mockAgents().agents[1];
    const missing = { value: null, unknown: "not returned" };
    const report = {
      ...mockAgents(),
      memory: [{
        agent_id: a.id, pid: a.pid, started_at: a.started_at,
        headline_bytes: a.memory_bytes, headline_counter: "physical_footprint" as const,
        headline_meaning: "macOS physical footprint",
        machine_bytes: { value: null, unknown: "machine size was not read" },
        share_ppm: { value: null, unknown: "machine size was not read" },
        resident: missing, physical_footprint: { value: a.memory_bytes, unknown: null },
        private_bytes: { value: null, unknown: "task_for_pid is not used" },
        shared_bytes: missing, proportional: missing, compressed: missing,
        virtual_bytes: missing, swap_bytes: missing, commit_bytes: missing,
        peak_resident: { value: null, unknown: "peaks are not simultaneous" },
        peak_physical: { value: null, unknown: "peaks are not simultaneous" },
        groups: [], processes: [], limits: [],
        trend: { samples: [], direction: "unknown" as const, unknown: "only one sample", rule: "band" },
      }],
    };
    const { container } = render(AgentDetails, { ...detailProps(a), report });
    expect(container.textContent).toContain("unknown (not returned)");
    expect(container.textContent).toContain("task_for_pid is not used");
    expect(container.textContent).not.toContain("Private 0 B");
  });

  it("shows omitted folders, links and tools even when their visible lists are empty", async () => {
    const a = { ...mockAgents().agents[0], folders: [], links: [], tools: [], more_folders: 3, more_links: 5, more_tools: 4 };
    const { container } = render(AgentDetails, detailProps(a));
    expect(container.textContent).toContain("+3 folders not listed");
    expect(container.textContent).toContain("+4 tools not listed");
    // Relationships live on Flow, so the details screen does not repeat the link list.
    expect(container.textContent).not.toContain("+5 links not listed");
  });

  it("opens MCP & skills, Flow and Security instead of copying them", async () => {
    const r = mockAgents(), a = r.agents[1];
    const oncapabilities = vi.fn(), onflow = vi.fn(), oninsights = vi.fn();
    const view = render(AgentDetails, { ...detailProps(a), report: r, oncapabilities, onflow, oninsights });
    expect(view.queryByRole("tablist", { name: "Agent sections" })).toBeNull();
    expect(view.getByRole("region", { name: "RAM" })).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Open in MCP & skills" }));
    expect(oncapabilities).toHaveBeenCalledOnce();
    await fireEvent.click(view.getByRole("button", { name: "Open in Flow" }));
    expect(onflow).toHaveBeenCalledOnce();
    await fireEvent.click(view.getByRole("button", { name: "Open in Security" }));
    expect(oninsights).toHaveBeenCalledOnce();
    expect(view.container.textContent).not.toContain("Live socket links");
    expect(view.container.textContent).not.toContain("What Cursor can use");
  });

  it("keeps servers, skills and file locations behind Configuration", () => {
    const report = mockAgents(undefined, true);
    const a = report.agents.find((agent) => agent.product === "cursor")!;
    const { container, getByRole } = render(AgentDetails, { ...detailProps(a), report, oncapabilities() {}, onflow() {}, oninsights() {} });
    const cfg = container.querySelector("details.cfg");
    expect(cfg).toBeTruthy();
    expect((cfg as HTMLDetailsElement).open).toBe(false);
    expect(cfg?.textContent).toContain("Locations & layers");
    expect(getByRole("button", { name: "Open in MCP & skills" }).getAttribute("title")).toBe("Servers, skills and file locations");
    expect(getByRole("button", { name: "Open in Flow" }).getAttribute("title")).toBe("Connections and recorded calls");
    expect(getByRole("button", { name: "Open in Security" }).getAttribute("title")).toBe("Findings and analyzer results");
  });

  it("keeps the agent's own facts and the ports that open the port details", () => {
    const a = mockAgents().agents[0];
    a.ports[0].exposure = "specific";
    const { container } = render(AgentDetails, detailProps(a));
    expect(container.textContent).toContain("Ports it serves");
    expect(container.textContent).toContain("bound to a specific interface");
    expect(container.textContent).not.toContain("Open findings");
    expect(container.querySelector(`#section-memory-agent-${a.id.replace(/[^a-zA-Z0-9_-]/g, "-")}`)).toBeTruthy();
    expect(container.textContent).toContain("RAM");
  });

  it("keeps collection limitations visible when no agents were detected", () => {
    const r = { ...mockAgents(), agents: [], limits: ["Process information was unavailable; this scan is incomplete."] };
    const { container } = render(AgentsView, { report: r, entries: [], selectedAgent: null, dark: false, reduced: true, onselectagent() {}, onentry() {}, ...actions });
    expect(container.textContent).toContain("this scan is incomplete");
    expect(container.querySelector<HTMLDetailsElement>(".limits")?.open).toBe(true);
  });

  it("shows a distinct empty search result while retaining report limitations", () => {
    const { container } = render(AgentsView, { report: mockAgents(), query: "missing-agent", entries: [], selectedAgent: null, dark: false, reduced: true, onselectagent() {}, onentry() {}, ...actions });
    expect(container.textContent).toContain("No agents match this search");
    expect(container.textContent).toContain("0 / 3");
    expect(container.textContent).toContain("individual tool calls aren't collected");
    expect(container.querySelector("article.card")).toBeNull();
  });

  it("hides the agent cards so the map uses the full width", async () => {
    const view = render(AgentsView, { report: mockAgents(), entries: [], selectedAgent: null, dark: false, reduced: true, onselectagent() {}, onentry() {}, ...actions });
    expect(view.getByRole("complementary", { name: "Agents" })).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Hide cards" }));
    expect(view.queryByRole("complementary", { name: "Agents" })).toBeNull();
    expect(view.getByRole("button", { name: "Show cards" })).toBeTruthy();
  });

  it("draws the selected agent's picture and leaves the other agents off the map", () => {
    const view = render(AgentsView, { report: mockAgents(), entries: [], selectedAgent: "agent:52000", picture: "footprint", dark: false, reduced: true, onselectagent() {}, onentry() {}, ...actions });
    const ids = [...view.container.querySelectorAll(".map .svelte-flow__node")].map((n) => n.getAttribute("data-id"));
    expect(ids).toContain("agent:52000");
    expect(ids).not.toContain("agent:51200");
    expect(ids).not.toContain("agent:48000");
    expect(view.container.querySelector(".map")?.textContent).toContain("Cursor: its folders, tools, ports and hosts.");
    expect(view.getByRole("complementary", { name: "Agents" }).textContent).toContain("Claude Code");
  });

  it("starts on the computer picture with Project disabled until a folder is chosen", async () => {
    const view = render(AgentsView, { report: mockAgents(), entries: [], selectedAgent: null, dark: false, reduced: true, onselectagent() {}, onentry() {}, ...actions });
    expect(view.getByRole("radio", { name: "Computer" }).getAttribute("aria-checked")).toBe("true");
    expect((view.getByRole("radio", { name: "Project" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(view.getByRole("radio", { name: "Footprint" }));
    expect(view.getByRole("radio", { name: "Footprint" }).getAttribute("aria-checked")).toBe("true");
    expect(view.getByRole("button", { name: "Recent projects" })).toBeTruthy();
  });

  it("enables Project after a folder is chosen from a card", async () => {
    const view = render(AgentsView, { report: mockAgents(), entries: MOCK_SNAPSHOT.entries, selectedAgent: "agent:52000", dark: false, reduced: true, onselectagent() {}, onentry() {}, ...actions });
    await fireEvent.click(view.getByRole("button", { name: "Folder actions for /Users/dev/code/shop-web" }));
    expect((view.getByRole("radio", { name: "Project" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("hides the agents legend and counts", async () => {
    const view = render(AgentsView, { report: mockAgents(), entries: [], selectedAgent: null, dark: false, reduced: true, onselectagent() {}, onentry() {}, ...actions });
    expect(view.container.querySelector(".legend")).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Legend" }));
    expect(view.container.querySelector(".legend")).toBeNull();
    expect(view.getByRole("button", { name: "Legend" }).getAttribute("aria-pressed")).toBe("false");
  });
});

describe("AgentsView when the lookup fails", () => {
  it("says why and offers to try again instead of looking forever", async () => {
    const retry = vi.fn();
    const { container, getByRole } = render(AgentsView, { report: null, capabilitiesError: "holdmap isn't installed", onrefreshcapabilities: retry, entries: [], selectedAgent: null, dark: false, reduced: true, onselectagent() {}, onentry() {}, ...actions });
    expect(getByRole("alert").textContent).toContain("Couldn't look up the agents");
    expect(container.textContent).toContain("holdmap isn't installed");
    expect(container.textContent).not.toContain("Looking for agents");
    await fireEvent.click(getByRole("button", { name: "Try again" }));
    expect(retry).toHaveBeenCalledOnce();
  });
});
