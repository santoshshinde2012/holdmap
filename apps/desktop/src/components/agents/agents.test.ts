// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import AgentCard from "./AgentCard.svelte";
import AgentsView from "../AgentsView.svelte";
import { MOCK_SNAPSHOT, mockAgents } from "../../lib/mock";
import type { Agent } from "../../lib/types";

beforeAll(() => { Element.prototype.scrollIntoView ??= () => {}; });
afterEach(cleanup);

const actions = { onreveal() {}, oneditor() {}, onstop() {}, onstopall() {} };
const props = (agent: Agent, selected = false) => ({ agent, selected, report: mockAgents(), entries: MOCK_SNAPSHOT.entries, color: "var(--accent)", onselect() {}, onentry() {}, ...actions });

describe("agent visibility", () => {
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
    const { container, rerender } = render(AgentCard, props(a, true));
    expect(container.textContent).toContain("Filesystem MCP");
    expect(container.textContent).toContain("MCP server · inferred · no listening port");
    const details = container.querySelector("section[aria-label='Agent tools'] details") as HTMLDetailsElement;
    await fireEvent.click(details.querySelector("summary")!);
    // jsdom does not implement native details activation.
    details.open = true;
    await rerender(props({ ...structuredClone(a), cpu_percent: 12.3 }, true));
    expect(container.textContent).toContain("12% CPU");
    expect(container.querySelector("section[aria-label='Agent tools'] details")).toBe(details);
    expect(details.open).toBe(true);
  });

  it("shows omitted folders, links and tools even when their visible lists are empty", () => {
    const a = { ...mockAgents().agents[0], folders: [], links: [], tools: [], more_folders: 3, more_links: 5, more_tools: 4 };
    const { container } = render(AgentCard, props(a, true));
    expect(container.textContent).toContain("+3 folders not listed");
    expect(container.textContent).toContain("+5 links not listed");
    expect(container.textContent).toContain("+4 tools not listed");
  });

  it("can select the parent agent and describes specific binds precisely", async () => {
    const a = mockAgents().agents[0], navigate = vi.fn();
    a.ports[0].exposure = "specific";
    const { getByRole, container } = render(AgentCard, { ...props(a, true), onnavigate: navigate });
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
});
