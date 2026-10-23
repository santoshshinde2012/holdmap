// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import AgentCard from "./AgentCard.svelte";
import AgentDetails from "./AgentDetails.svelte";
import AgentsView from "../AgentsView.svelte";
import { MOCK_SNAPSHOT, mockAgents } from "../../lib/mock";
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
    expect(container.textContent).toContain("total memory");
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

  it("shows omitted folders, links and tools even when their visible lists are empty", () => {
    const a = { ...mockAgents().agents[0], folders: [], links: [], tools: [], more_folders: 3, more_links: 5, more_tools: 4 };
    const { container } = render(AgentDetails, detailProps(a));
    expect(container.textContent).toContain("+3 folders not listed");
    expect(container.textContent).toContain("+5 links not listed");
    expect(container.textContent).toContain("+4 tools not listed");
  });

  it("can select the parent agent and describes specific binds precisely", async () => {
    const a = mockAgents().agents[0], navigate = vi.fn();
    a.ports[0].exposure = "specific";
    const { getByRole, container } = render(AgentDetails, { ...detailProps(a), onnavigate: navigate });
    await fireEvent.click(getByRole("button", { name: "Cursor 52000" }));
    expect(navigate).toHaveBeenCalledWith("agent:52000");
    expect(container.textContent).toContain("bound to a specific interface");
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
