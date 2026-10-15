// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import type { ComponentProps } from "svelte";
import AgentCapabilitiesView from "./AgentCapabilitiesView.svelte";
import AgentCard from "./AgentCard.svelte";
import { MOCK_SNAPSHOT, mockAgents } from "../../lib/mock";
import type { AgentsReport } from "../../lib/types";

beforeAll(() => {
  globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
  Element.prototype.scrollIntoView ??= vi.fn();
  window.matchMedia ??= ((media: string) => ({ matches: false, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const report = () => mockAgents(undefined, true);
const mount = (r: AgentsReport | null = report(), extras: Partial<ComponentProps<typeof AgentCapabilitiesView>> = {}) => render(AgentCapabilitiesView, { report: r, selectedAgent: "agent:52000", reduced: true, onselectagent: vi.fn(), ...extras });
const cursorMcp = (name: string) => `Inspect MCP server ${name}, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.cursor/mcp.json`;
const frontendSkill = (name = "frontend-checks") => `Inspect skill ${name}, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.agents/skills/frontend-checks/SKILL.md`;
const details = (view: ReturnType<typeof mount>) => view.getByRole("complementary", { name: "Capability details" });

describe("MCP and skills graph workspace", () => {
  it("starts with a graph and agent rail without an automatic inspector or expanded panels", () => {
    const view = mount(report(), { selectedAgent: null });
    expect(view.getByRole("region", { name: "Agent capability graph" })).toBeTruthy();
    expect(view.getByRole("complementary", { name: "Capability agents" })).toBeTruthy();
    expect(view.queryByRole("complementary", { name: "Capability details" })).toBeNull();
    expect(view.queryByRole("region", { name: "Visibility limits" })).toBeNull();
    expect(view.queryByRole("combobox", { name: /^Scope:/ })).toBeNull();
    expect(view.getByRole("button", { name: "Coverage" }).getAttribute("aria-expanded")).toBe("false");
    expect(view.container.querySelector(".detail-pane")).toBeNull();
  });
  it("selects an MCP graph card explicitly and restores its focus on close", async () => {
    const view = mount(), card = view.getByRole("button", { name: cursorMcp("filesystem") });
    card.focus(); await fireEvent.click(card);
    expect(details(view).textContent).toContain("/Users/dev/code/shop-web/.cursor/mcp.json");
    expect(details(view).textContent).toContain("STDIO");
    expect(details(view).textContent).toContain("Configured does not mean connected, loaded or invoked");
    expect(document.activeElement).toBe(view.getByRole("heading", { name: "filesystem" }));
    expect(card.getAttribute("aria-pressed")).toBe("true");
    expect(view.queryByRole("complementary", { name: "Capability agents" })).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "Close capability details" }));
    expect(view.queryByRole("complementary", { name: "Capability details" })).toBeNull();
    expect(document.activeElement).toBe(card);
  });
  it("shows skill purpose/source without claiming activation and opens Calls only on request", async () => {
    const onrecordedcalls = vi.fn(), view = mount(report(), { onrecordedcalls });
    await fireEvent.click(view.getByRole("button", { name: frontendSkill() }));
    expect(details(view).textContent).toContain("frontend-checks/SKILL.md");
    expect(details(view).textContent).toContain("Check UI behavior, accessibility and responsive layouts");
    expect(details(view).textContent).toContain("does not prove this skill was loaded or activated");
    expect(details(view).textContent).toContain("skill instructions are not collected or executed");
    expect(onrecordedcalls).not.toHaveBeenCalled();
    await fireEvent.click(view.getByRole("button", { name: "Open recorded calls" }));
    expect(onrecordedcalls).toHaveBeenCalledOnce();
  });
  it("keeps a disabled definition separate from an inferred runtime candidate", async () => {
    const view = mount();
    await fireEvent.click(view.getByRole("button", { name: cursorMcp("legacy-browser") }));
    expect(details(view).textContent).toContain("Disabled in config");
    await fireEvent.click(view.getByRole("button", { name: "Inspect running process Filesystem MCP, PID 52070" }));
    expect(details(view).textContent).toContain("has not been matched to a configuration entry or invocation");
    expect(details(view).textContent).toContain("inferred");
    expect(details(view).textContent).not.toContain("Source file");
  });
  it("opens Filters on request and keeps Running independent of definition scope/state", async () => {
    const view = mount();
    await fireEvent.click(view.getByRole("button", { name: "Filters" }));
    const state = view.getByRole("combobox", { name: /^State:/ });
    await fireEvent.click(state); await fireEvent.click(view.getByRole("option", { name: "Disabled MCP" }));
    expect(view.getAllByRole("button", { name: /^Inspect MCP server/ })).toHaveLength(1);
    expect(view.queryByRole("button", { name: /^Inspect skill/ })).toBeNull();
    expect(view.getByRole("button", { name: /Inspect running process/ })).toBeTruthy();
    await fireEvent.click(view.getByRole("radio", { name: /^Skills/ }));
    expect((state as HTMLButtonElement).disabled).toBe(true);
    expect(view.getAllByRole("button", { name: /^Inspect skill/ })).toHaveLength(2);
    expect(view.queryByRole("button", { name: /Inspect running process/ })).toBeNull();
    await fireEvent.click(view.getByRole("radio", { name: /^Running/ }));
    expect(view.queryByRole("button", { name: /^Inspect MCP server/ })).toBeNull();
    expect(view.queryByRole("button", { name: /^Inspect skill/ })).toBeNull();
    expect(view.getByRole("button", { name: /Inspect running process/ })).toBeTruthy();
    expect((view.getByRole("combobox", { name: /^Scope:/ }) as HTMLButtonElement).disabled).toBe(true);
  });
  it("closes a filtered-out selection without choosing another definition", async () => {
    const view = mount();
    await fireEvent.click(view.getByRole("button", { name: cursorMcp("filesystem") }));
    await fireEvent.click(view.getByRole("radio", { name: /^Skills/ }));
    expect(view.queryByRole("complementary", { name: "Capability details" })).toBeNull();
    expect(view.getAllByRole("button", { name: /^Inspect skill/ }).every((node) => node.getAttribute("aria-pressed") === "false")).toBe(true);
  });
  it("keeps old-backend coverage hidden until requested, then focuses its heading", async () => {
    const r = mockAgents(); r.agents.forEach((agent) => delete agent.capabilities);
    const view = mount(r);
    expect(view.container.textContent).toContain("Inventory not inspected");
    expect(view.queryByRole("region", { name: "Visibility limits" })).toBeNull();
    expect(view.container.textContent).not.toContain("not reported by this backend");
    await fireEvent.click(view.getByRole("button", { name: "Coverage" }));
    expect(view.getByRole("region", { name: "Visibility limits" }).textContent).toContain("not reported by this backend");
    expect(document.activeElement).toBe(view.getByRole("heading", { name: "Coverage & sources" }));
  });
  it("deduplicates common coverage and restores the opener after Escape", async () => {
    const r = report(); for (const agent of r.agents) agent.capabilities!.limits = ["Shared coverage fixture."];
    const view = mount(r, { selectedAgent: null }), button = view.getByRole("button", { name: "Coverage" });
    button.focus(); await fireEvent.click(button);
    expect(view.getAllByText("Shared coverage fixture.")).toHaveLength(1);
    expect(view.getByRole("region", { name: "Visibility limits" }).textContent).toContain("Applies to 3 agents");
    await fireEvent.keyDown(view.getByRole("heading", { name: "Coverage & sources" }), { key: "Escape" });
    expect(view.queryByRole("region", { name: "Visibility limits" })).toBeNull();
    expect(document.activeElement).toBe(button);
  });
  it("retains an empty selected agent as a graph card without a blank inspector or expanded limits", async () => {
    const view = mount(report(), { selectedAgent: "agent:48000", error: "A supported source could not be read." });
    expect(view.getByRole("alert").textContent).toContain("could not be read");
    expect(view.getByRole("button", { name: "Inspect capabilities for Docker Desktop, PID 48000" })).toBeTruthy();
    expect(view.getByRole("heading", { name: "Inventory not inspected" })).toBeTruthy();
    expect(view.container.textContent).toContain("Missing entries do not prove");
    expect(view.queryByRole("complementary", { name: "Capability details" })).toBeNull();
    expect(view.queryByRole("region", { name: "Visibility limits" })).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "View coverage" }));
    expect(view.getByRole("region", { name: "Visibility limits" }).textContent).toContain("not supported for Docker Desktop");
  });
  it("keeps no-match recovery separate from coverage and automatic selection", async () => {
    const onclearquery = vi.fn(), view = mount(report(), { query: "no-such-capability", onclearquery });
    expect(view.getByRole("heading", { name: "No inventory matches" })).toBeTruthy();
    expect(view.queryByRole("complementary", { name: "Capability details" })).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "Clear search" }));
    expect(onclearquery).toHaveBeenCalledOnce();
  });
  it("shows loading and no-running-agent states without inventing installed inventory", async () => {
    const view = mount(null, { loading: true }); expect(view.getByText("Loading agents…")).toBeTruthy();
    await view.rerender({ report: { ...report(), agents: [] }, selectedAgent: null, onselectagent() {} });
    expect(view.getByRole("heading", { name: "No running agents to inspect" })).toBeTruthy();
    expect(view.container.textContent).toContain("not a list of everything installed");
  });
  it("retains an exact selection on refresh and closes it when that definition disappears", async () => {
    const r = report(), view = mount(r);
    await fireEvent.click(view.getByRole("button", { name: cursorMcp("filesystem") }));
    await view.rerender({ report: structuredClone(r), selectedAgent: "agent:52000", reduced: true, onselectagent() {} });
    expect(details(view).textContent).toContain("filesystem");
    const next = structuredClone(r); next.agents[1].capabilities!.mcp_servers = next.agents[1].capabilities!.mcp_servers.filter((item) => item.name !== "filesystem");
    await view.rerender({ report: next, selectedAgent: "agent:52000", reduced: true, onselectagent() {} });
    await waitFor(() => expect(view.queryByRole("complementary", { name: "Capability details" })).toBeNull());
    expect(view.getByRole("complementary", { name: "Capability agents" })).toBeTruthy();
  });
  it("shares exact definitions across agents while distinguishing same aliases at different sources", async () => {
    const r = report(); r.agents[0].capabilities!.skills.push(structuredClone(r.agents[1].capabilities!.skills[1]));
    const view = mount(r, { selectedAgent: null });
    expect(view.getAllByRole("button", { name: /^Inspect MCP server filesystem, Project scope\./ })).toHaveLength(2);
    const shared = view.getAllByRole("button", { name: /^Inspect skill frontend-checks/ }); expect(shared).toHaveLength(1);
    await fireEvent.click(shared[0]); expect(details(view).textContent).toContain("PID 51200"); expect(details(view).textContent).toContain("PID 52000");
    await fireEvent.click(view.getByRole("button", { name: cursorMcp("filesystem") }));
    expect(details(view).textContent).toContain("/Users/dev/code/shop-web/.cursor/mcp.json");
  });
  it("renders untrusted purpose as plain text while retaining skill folder identity", async () => {
    const r = report(), skill = r.agents[1].capabilities!.skills[1]; skill.declared_name = "UI verification"; skill.description = '<img src=x onerror="alert(1)">';
    const view = mount(r); await fireEvent.click(view.getByRole("button", { name: frontendSkill("UI verification") }));
    expect(view.container.querySelector("img")).toBeNull(); expect(details(view).textContent).toContain(skill.description); expect(details(view).textContent).toContain("Skill folder namefrontend-checks");
  });
  it("keeps the agent-card entry point explicitly opt-in", async () => {
    const r = mockAgents(), oncapabilities = vi.fn();
    const view = render(AgentCard, { agent: r.agents[1], report: r, entries: MOCK_SNAPSHOT.entries, selected: true, color: "var(--accent)", onselect() {}, onentry() {}, onreveal() {}, oneditor() {}, onstop() {}, onstopall() {}, oncapabilities });
    expect(oncapabilities).not.toHaveBeenCalled(); await fireEvent.click(view.getByRole("button", { name: /MCP & skills.*inspect sources/ })); expect(oncapabilities).toHaveBeenCalledOnce(); expect(r.agents[1].capabilities).toBeUndefined();
  });
});
