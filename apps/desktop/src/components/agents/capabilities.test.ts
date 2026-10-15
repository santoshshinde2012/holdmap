// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import type { ComponentProps } from "svelte";
import AgentCapabilitiesView from "./AgentCapabilitiesView.svelte";
import AgentCard from "./AgentCard.svelte";
import { MOCK_SNAPSHOT, mockAgents } from "../../lib/mock";
import type { AgentsReport } from "../../lib/types";

beforeAll(() => {
  Element.prototype.scrollIntoView ??= vi.fn();
  window.matchMedia ??= ((media: string) => ({ matches: false, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const report = () => mockAgents(undefined, true);
const mount = (r: AgentsReport | null = report(), extras: Partial<ComponentProps<typeof AgentCapabilitiesView>> = {}) => render(AgentCapabilitiesView, { report: r, selectedAgent: "agent:52000", onselectagent: vi.fn(), ...extras });

describe("MCP and skills workspace", () => {
  it("shows a declaration/source/project diagram without claiming activation or calls", async () => {
    const view = mount();
    await fireEvent.click(view.getByRole("button", { name: "Inspect MCP server filesystem, Project scope" }));
    const relationship = view.getByRole("region", { name: "Declared capability relationship" });
    expect(relationship.textContent).toContain("mcp.json");
    expect(relationship.textContent).toContain("filesystem");
    expect(relationship.textContent).toContain("shop-web");
    const detail = view.getByRole("complementary", { name: "Capability details" });
    expect(detail.textContent).toContain("/Users/dev/code/shop-web/.cursor/mcp.json");
    expect(detail.textContent).toContain("STDIO");
    expect(detail.textContent).toContain("Configured does not mean connected, loaded or invoked");
    const runtime = view.getByRole("region", { name: "Running MCP process observations" });
    expect(runtime.textContent).toContain("52070");
    expect(runtime.textContent).toContain("independently of configuration");
  });

  it("selects skill file details and navigates to reported calls only on request", async () => {
    const onrecordedcalls = vi.fn(); const view = mount(report(), { onrecordedcalls });
    await fireEvent.click(view.getByRole("button", { name: "Inspect skill frontend-checks, Project scope" }));
    const detail = view.getByRole("complementary", { name: "Capability details" });
    expect(detail.textContent).toContain("frontend-checks/SKILL.md");
    expect(detail.textContent).toContain("skill instructions are not collected or executed");
    expect(detail.textContent).toContain("Check UI behavior, accessibility and responsive layouts");
    expect(detail.textContent).toContain("does not prove this skill was loaded or activated");
    expect(onrecordedcalls).not.toHaveBeenCalled();
    await fireEvent.click(view.getByRole("button", { name: "Open recorded calls" }));
    expect(onrecordedcalls).toHaveBeenCalledOnce();
  });

  it("keeps running processes separate when a definition is disabled", async () => {
    const view = mount();
    await fireEvent.click(view.getByRole("button", { name: "Inspect MCP server legacy-browser, Project scope" }));
    expect(view.getByRole("complementary", { name: "Capability details" }).textContent).toContain("Disabled in config");
    await fireEvent.click(view.getByRole("button", { name: "Inspect running process Filesystem MCP, PID 52070" }));
    const detail = view.getByRole("complementary", { name: "Capability details" });
    expect(detail.textContent).toContain("has not been matched to a configuration entry or invocation");
    expect(detail.textContent).toContain("inferred");
    expect(view.getByRole("region", { name: "Observed process relationship" })).toBeTruthy();
  });

  it("handles search, filtering and no-match recovery without losing visibility limits", async () => {
    const onclearquery = vi.fn(); const view = mount(report(), { query: "not-a-capability", onclearquery });
    expect(view.getByText("No inventory matches")).toBeTruthy();
    expect(view.getByText("Visibility limits")).toBeTruthy();
    await fireEvent.click(view.getAllByRole("button", { name: "Clear search" })[0]);
    expect(onclearquery).toHaveBeenCalledOnce();
    await view.rerender({ report: report(), selectedAgent: "agent:52000", query: "", onselectagent() {} });
    await fireEvent.click(view.getByRole("radio", { name: /Skills/ }));
    expect(view.queryByRole("button", { name: /Inspect MCP server/ })).toBeNull();
    expect(view.getAllByRole("button", { name: /Inspect skill/ })).toHaveLength(2);
    expect(view.getByRole("button", { name: /Inspect running process/ })).toBeTruthy();
  });

  it("does not treat a missing or uninspected DTO as no configured capabilities", () => {
    const r = mockAgents(); r.agents.forEach((agent) => delete agent.capabilities);
    const view = mount(r);
    expect(view.getByRole("heading", { name: "Inventory not inspected" })).toBeTruthy();
    expect(view.container.textContent).toContain("not reported by this backend");
    expect(view.container.querySelector<HTMLDetailsElement>(".limits")?.open).toBe(true);
  });

  it("retains loading/error/unsupported states and bounds all empty-state claims", () => {
    const view = mount(report(), { selectedAgent: "agent:48000", error: "A supported source could not be read." });
    expect(view.getByRole("alert").textContent).toContain("could not be read");
    expect(view.getByRole("heading", { name: "Inventory not inspected" })).toBeTruthy();
    const unsupportedAgent = view.getByRole("button", { name: "Inspect capabilities for Docker Desktop, PID 48000" });
    expect(unsupportedAgent.textContent).toContain("Inventory not inspected");
    expect(unsupportedAgent.textContent).not.toContain("0 MCP");
    expect(view.container.textContent).toContain("not supported for Docker Desktop");
    expect(view.container.textContent).toContain("Missing entries do not prove");
  });

  it("shows initial-loading state without inventing an installed inventory", () => {
    const view = mount(null, { loading: true });
    expect(view.getByText("Loading agents…")).toBeTruthy();
  });

  it("explains that no running agents is not an installed-capability inventory", () => {
    const view = mount({ ...report(), agents: [] });
    expect(view.getByRole("heading", { name: "No running agents to inspect" })).toBeTruthy();
    expect(view.container.textContent).toContain("not a list of everything installed");
  });

  it("escapes names and metadata, and preserves selection across refreshes", async () => {
    const r = report(); const server = r.agents[1].capabilities!.mcp_servers[1];
    server.name = '<img src=x onerror="alert(1)">';
    const view = mount(r);
    await fireEvent.click(view.getByRole("button", { name: `Inspect MCP server ${server.name}, Project scope` }));
    expect(view.container.querySelector("img")).toBeNull();
    await view.rerender({ report: structuredClone(r), selectedAgent: "agent:52000", onselectagent() {} });
    expect(view.getByRole("complementary", { name: "Capability details" }).textContent).toContain(server.name);
  });

  it("exposes the agent-card entry point without inspecting sources itself", async () => {
    const r = mockAgents(); const oncapabilities = vi.fn();
    const view = render(AgentCard, { agent: r.agents[1], report: r, entries: MOCK_SNAPSHOT.entries, selected: true, color: "var(--accent)", onselect() {}, onentry() {}, onreveal() {}, oneditor() {}, onstop() {}, onstopall() {}, oncapabilities });
    const button = view.getByRole("button", { name: /MCP & skills.*inspect sources/ });
    expect(oncapabilities).not.toHaveBeenCalled();
    await fireEvent.click(button); expect(oncapabilities).toHaveBeenCalledOnce();
    expect(r.agents[1].capabilities).toBeUndefined();
  });

  it("renders skill purpose as plain text and preserves the folder identity", async () => {
    const r = report(); const skill = r.agents[1].capabilities!.skills[1];
    skill.declared_name = "UI verification";
    skill.description = '<img src=x onerror="alert(1)">';
    const view = mount(r);
    await fireEvent.click(view.getByRole("button", { name: "Inspect skill UI verification, Project scope" }));
    expect(view.container.querySelector("img")).toBeNull();
    const detail = view.getByRole("complementary", { name: "Capability details" });
    expect(detail.textContent).toContain(skill.description);
    expect(detail.textContent).toContain("Skill folder namefrontend-checks");
  });
});
