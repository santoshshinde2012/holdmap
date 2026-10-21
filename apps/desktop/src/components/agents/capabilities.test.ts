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
// Details open inline (a region by the clicked card or row); the map canvas keeps its side panel.
const details = (view: ReturnType<typeof mount>) => view.queryByRole("region", { name: "Capability details" }) ?? view.getByRole("complementary", { name: "Capability details" });
const noDetails = (view: ReturnType<typeof mount>) => !view.queryByRole("region", { name: "Capability details" }) && !view.queryByRole("complementary", { name: "Capability details" });
const tab = (view: ReturnType<typeof mount>, name: string) => fireEvent.click(view.getByRole("tab", { name }));

/** Reproduce repeated roots sharing inventory without adding a product fixture hook. */
function repeatedRoots(): AgentsReport {
  const r = report(), cursor = r.agents[1];
  for (const pid of [52001, 52002, 52003]) {
    const clone = structuredClone(cursor);
    clone.id = `agent:${pid}`; clone.pid = pid; clone.started_at += pid;
    clone.processes = []; clone.process_ids = [pid]; clone.tools = [];
    r.agents.push(clone);
  }
  return r;
}

describe("MCP and skills capability explorer", () => {
  it("starts with purpose cards and no automatic inspector or expanded disclosures", () => {
    const view = mount(report(), { selectedAgent: null });
    expect(view.getByRole("radio", { name: "Explore" }).getAttribute("aria-checked")).toBe("true");
    expect(view.queryByRole("region", { name: "Agent capability graph" })).toBeNull();
    expect(view.getByRole("complementary", { name: "Capability agents" })).toBeTruthy();
    expect(noDetails(view)).toBe(true);
    expect(view.queryByRole("region", { name: "Visibility limits" })).toBeNull();
    expect(view.queryByRole("combobox", { name: /^Scope:/ })).toBeNull();
    const card = view.getByRole("button", { name: frontendSkill() });
    expect(card.textContent).toContain("Check UI behavior, accessibility and responsive layouts");
    expect(card.textContent).toContain("Skill file found");
  });
  it("selects a card explicitly, separates source/evidence tabs and restores its opener", async () => {
    const view = mount(), card = view.getByRole("button", { name: cursorMcp("filesystem") });
    card.focus(); await fireEvent.click(card);
    expect(document.activeElement).toBe(view.getByRole("heading", { name: "filesystem" }));
    expect(view.getByRole("tab", { name: "Purpose" }).getAttribute("aria-selected")).toBe("true");
    expect(details(view).textContent).not.toContain("/Users/dev/code/shop-web/.cursor/mcp.json");
    await tab(view, "Source");
    expect(details(view).textContent).toContain("/Users/dev/code/shop-web/.cursor/mcp.json");
    expect(details(view).textContent).toContain("STDIO");
    await tab(view, "Evidence");
    expect(details(view).textContent).toMatch(/Connection & catalog/);
    await fireEvent.click(view.getByRole("button", { name: "Close capability details" }));
    expect(noDetails(view)).toBe(true);
    expect(document.activeElement).toBe(card);
  });
  it("keeps declared skill purpose separate from activity and opens Calls only explicitly", async () => {
    const onrecordedcalls = vi.fn(), view = mount(report(), { onrecordedcalls });
    await fireEvent.click(view.getByRole("button", { name: frontendSkill() }));
    expect(details(view).textContent).toContain("Check UI behavior, accessibility and responsive layouts");
    await tab(view, "Evidence");
    expect(details(view).textContent).toMatch(/Loading & activation/);
    expect(details(view).textContent).toMatch(/instructions.*not (?:collected|retained|executed)/i);
    await tab(view, "Activity");
    expect(details(view).textContent).toMatch(/No recorded calls are linked/);
    expect(onrecordedcalls).not.toHaveBeenCalled();
    await fireEvent.click(view.getByRole("button", { name: "Open recorded calls" }));
    expect(onrecordedcalls).toHaveBeenCalledOnce();
  });
  it("offers Map without losing the exact Explore selection", async () => {
    const view = mount();
    const card = view.getByRole("button", { name: frontendSkill() });
    card.focus(); await fireEvent.click(card);
    await fireEvent.click(view.getByRole("radio", { name: "Map" }));
    expect(view.getByRole("region", { name: "Agent capability graph" })).toBeTruthy();
    expect(details(view).textContent).toContain("frontend-checks");
    await fireEvent.click(view.getByRole("radio", { name: "Explore" }));
    expect(details(view).textContent).toContain("frontend-checks");
    await fireEvent.click(view.getByRole("button", { name: "Close capability details" }));
    expect(document.activeElement).toBe(view.getByRole("button", { name: frontendSkill() }));
  });
  it("groups four matching roots once and expands every exact PID without merging runtime", async () => {
    const view = mount(repeatedRoots(), { selectedAgent: null });
    expect(view.getAllByRole("button", { name: /^Explore context Cursor,/ })).toHaveLength(1);
    expect(view.getByRole("button", { name: "Explore context Cursor, 4 observed processes" })).toBeTruthy();
    expect(view.getAllByRole("button", { name: /^Inspect skill frontend-checks/ })).toHaveLength(1);
    expect(view.queryByRole("button", { name: "Inspect agent card Cursor, PID 52001" })).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "Show processes for Cursor" }));
    for (const pid of [52000, 52001, 52002, 52003]) expect(view.getByRole("button", { name: `Inspect agent card Cursor, PID ${pid}` })).toBeTruthy();
    expect(view.getAllByRole("button", { name: /^Inspect running process Filesystem MCP/ })).toHaveLength(1);
    expect(noDetails(view)).toBe(true);
  });
  it("does not group roots with conflicting coverage or declarations", () => {
    const r = repeatedRoots(); r.agents.at(-1)!.capabilities!.limits.push("This context was bounded earlier.");
    const view = mount(r, { selectedAgent: null });
    expect(view.getAllByRole("button", { name: /^Explore context Cursor,/ })).toHaveLength(2);
    expect(view.getByRole("button", { name: "Explore context Cursor, 3 observed processes" })).toBeTruthy();
  });
  it("keeps a selected context scoped after metadata refresh and closes details when its members disappear", async () => {
    const r = repeatedRoots(), view = mount(r, { selectedAgent: null });
    await fireEvent.click(view.getByRole("button", { name: "Explore context Cursor, 4 observed processes" }));
    expect(view.getAllByRole("button", { name: /^Inspect MCP server/ })).toHaveLength(3);
    const next = structuredClone(r);
    for (const agent of next.agents.filter(agent => agent.name === "Cursor")) agent.capabilities!.limits.push("Fresh source inspection was bounded.");
    await view.rerender({ report: next, selectedAgent: null, reduced: true, onselectagent() {} });
    expect(view.getAllByRole("button", { name: /^Inspect MCP server/ })).toHaveLength(3);
    expect(view.queryByRole("button", { name: /^Inspect MCP server filesystem,.*Claude Code/ })).toBeNull();
    const skill = view.getByRole("button", { name: /^Inspect skill frontend-checks/ });
    skill.focus(); await fireEvent.click(skill);
    expect(details(view).textContent).toContain("frontend-checks");
    const gone = { ...next, agents: next.agents.filter(agent => agent.name !== "Cursor") };
    await view.rerender({ report: gone, selectedAgent: null, reduced: true, onselectagent() {} });
    await waitFor(() => expect(noDetails(view)).toBe(true));
    expect(view.queryByRole("button", { name: /^Inspect skill frontend-checks/ })).toBeNull();
  });
  it("compares shared exact sources separately from same-name files and uninspected contexts", async () => {
    const r = report();
    r.agents[0].capabilities!.skills.push(structuredClone(r.agents[1].capabilities!.skills[0]));
    const view = mount(r, { selectedAgent: null });
    await fireEvent.click(view.getByRole("radio", { name: "Compare" }));
    const left = view.getByRole("combobox", { name: /^Left(?: context)?:/ });
    const right = view.getByRole("combobox", { name: /^Right(?: context)?:/ });
    await fireEvent.click(left); await fireEvent.click(view.getByRole("option", { name: /^Claude Code/ }));
    await fireEvent.click(right); await fireEvent.click(view.getByRole("option", { name: /^Cursor/ }));
    expect(view.container.textContent).toContain("/Users/dev/.agents/skills/code-review/SKILL.md");
    expect(view.container.textContent).toContain("/Users/dev/code/shop-api/.mcp.json");
    expect(view.container.textContent).toContain("/Users/dev/code/shop-web/.cursor/mcp.json");
    expect(view.container.textContent).toMatch(/\d+ shared/);
    const aliases = view.getAllByRole("button", { name: /^Inspect compared MCP server filesystem, / });
    expect(aliases).toHaveLength(2);
    for (const alias of aliases) {
      expect(alias.textContent).toContain("Found");
      expect(alias.textContent).toContain("Not found");
    }
    const shared = view.getByRole("button", { name: "Inspect compared skill code-review, User scope. Agents: Claude Code (PID 51200), Cursor (PID 52000). Source: /Users/dev/.agents/skills/code-review/SKILL.md" });
    expect(shared.textContent?.match(/Found/g)).toHaveLength(2);
    expect(shared.textContent).not.toContain("Not found");
    await fireEvent.click(right); await fireEvent.click(view.getByRole("option", { name: /^Docker Desktop/ }));
    expect(view.container.textContent).toMatch(/not inspected|unknown/i);
    expect(view.queryByText("Not installed", { exact: true })).toBeNull();
    expect(view.queryByText("Unused", { exact: true })).toBeNull();
    expect(view.container.textContent).not.toContain("0 tools");
  });
  it("distinguishes same-alias definitions in one source by scope, project and owning process", async () => {
    const r = report(), source = "/Users/dev/.claude.json";
    const definition = structuredClone(r.agents[0].capabilities!.mcp_servers[0]);
    r.agents[0].capabilities!.mcp_servers = [
      { ...definition, id: "same-file:user", source_path: source, scope: "user", project_path: null },
      { ...definition, id: "same-file:api", source_path: source, scope: "local", project_path: "/Users/dev/code/shop-api" },
    ];
    r.agents[1].capabilities!.mcp_servers = [
      { ...definition, id: "same-file:web", source_path: source, scope: "local", project_path: "/Users/dev/code/shop-web" },
    ];
    const view = mount(r, { selectedAgent: null });
    await fireEvent.click(view.getByRole("radio", { name: "Compare" }));
    await fireEvent.click(view.getByRole("combobox", { name: /^Left context:/ }));
    await fireEvent.click(view.getByRole("option", { name: /^Claude Code/ }));
    await fireEvent.click(view.getByRole("combobox", { name: /^Right context:/ }));
    await fireEvent.click(view.getByRole("option", { name: /^Cursor/ }));
    expect(view.getAllByRole("button", { name: /^Inspect compared MCP server filesystem, / })).toHaveLength(3);
    for (const [scope, owner, project] of [
      ["User", "Claude Code (PID 51200)", null],
      ["Local project", "Claude Code (PID 51200)", "/Users/dev/code/shop-api"],
      ["Local project", "Cursor (PID 52000)", "/Users/dev/code/shop-web"],
    ] as const) {
      const label = `Inspect compared MCP server filesystem, ${scope} scope. Agents: ${owner}.${project ? ` Project: ${project}.` : ""} Source: ${source}`;
      await fireEvent.click(view.getByRole("button", { name: label }));
      await tab(view, "Source");
      expect(details(view).textContent).toContain(source);
      expect(details(view).textContent).toContain(scope);
      if (project) expect(details(view).textContent).toContain(project);
      else expect(details(view).textContent).not.toContain("Project relationship");
      expect(details(view).textContent).toContain(owner.includes("51200") ? "PID 51200" : "PID 52000");
    }
  });
  it("shows catalog unknown rather than zero tools and never turns inventory into usage", async () => {
    const view = mount();
    await fireEvent.click(view.getByRole("radio", { name: /^Tools/ }));
    expect(view.getByRole("heading", { name: "Tool catalog not reported" })).toBeTruthy();
    expect(view.container.textContent).toMatch(/unknown, not zero/i);
    expect(view.container.textContent).not.toContain("0 tools");
    expect(noDetails(view)).toBe(true);
  });
  it("keeps Compare inspector ownership inside its chosen pair and invalidates a row on pair change", async () => {
    const r = report(), third = structuredClone(r.agents[1]);
    third.id = "agent:53000"; third.pid = 53000; third.product = "codex"; third.name = "Codex"; third.vendor = "OpenAI";
    third.processes = []; third.process_ids = [53000]; third.tools = [];
    r.agents.push(third);
    const view = mount(r, { selectedAgent: null });
    await fireEvent.click(view.getByRole("radio", { name: "Compare" }));
    const left = view.getByRole("combobox", { name: /^Left(?: context)?:/ });
    const right = view.getByRole("combobox", { name: /^Right(?: context)?:/ });
    await fireEvent.click(left); await fireEvent.click(view.getByRole("option", { name: /^Claude Code/ }));
    await fireEvent.click(right); await fireEvent.click(view.getByRole("option", { name: /^Cursor/ }));
    const reviewLabel = "Inspect compared skill code-review, User scope. Agents: Cursor (PID 52000). Source: /Users/dev/.agents/skills/code-review/SKILL.md";
    await fireEvent.click(view.getByRole("button", { name: reviewLabel }));
    await tab(view, "Source");
    expect(details(view).textContent).toContain("52000");
    expect(details(view).textContent).not.toContain("53000");
    await fireEvent.click(right); await fireEvent.click(view.getByRole("option", { name: /^Docker Desktop/ }));
    await waitFor(() => expect(noDetails(view)).toBe(true));
    expect(view.queryByRole("button", { name: reviewLabel })).toBeNull();
  });
  it("keeps disabled configuration separate from inferred runtime observations", async () => {
    const view = mount();
    await fireEvent.click(view.getByRole("button", { name: cursorMcp("legacy-browser") }));
    await tab(view, "Source"); expect(details(view).textContent).toContain("Disabled in config");
    await fireEvent.click(view.getByRole("radio", { name: /^Running/ }));
    await fireEvent.click(view.getByRole("button", { name: "Inspect running process Filesystem MCP, PID 52070" }));
    await tab(view, "Evidence");
    expect(details(view).textContent).toMatch(/Not matched to a config entry/);
    expect(details(view).textContent).toContain("inferred");
    await tab(view, "Source"); expect(details(view).textContent).not.toContain("Source file");
  });
  it("applies scope/state filters while clearing an excluded selection without replacement", async () => {
    const view = mount();
    await fireEvent.click(view.getByRole("button", { name: cursorMcp("filesystem") }));
    await fireEvent.click(view.getByRole("button", { name: "Filters" }));
    const state = view.getByRole("combobox", { name: /^State:/ });
    await fireEvent.click(state); await fireEvent.click(view.getByRole("option", { name: "Disabled MCP" }));
    expect(view.getAllByRole("button", { name: /^Inspect MCP server/ })).toHaveLength(1);
    expect(noDetails(view)).toBe(true);
    expect(view.getByRole("button", { name: cursorMcp("legacy-browser") }).getAttribute("aria-expanded")).toBe("false");
  });
  it("reveals old-backend coverage only on request without inventing an empty catalog", async () => {
    const r = mockAgents(); r.agents.forEach((agent) => delete agent.capabilities);
    const view = mount(r);
    await fireEvent.click(view.getByRole("radio", { name: /^Skills/ }));
    expect(view.getByRole("radio", { name: "Skills" }).textContent?.trim()).toBe("Skills");
    expect(view.queryByRole("region", { name: "Visibility limits" })).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "Coverage" }));
    expect(view.getByRole("region", { name: "Visibility limits" }).textContent).toContain("not reported by this backend");
    expect(document.activeElement).toBe(view.getByRole("heading", { name: "Coverage & sources" }));
  });
  it("deduplicates coverage notes and restores focus after local Escape", async () => {
    const r = report(); for (const agent of r.agents) agent.capabilities!.limits = ["Shared coverage fixture."];
    const view = mount(r, { selectedAgent: null }), button = view.getByRole("button", { name: "Coverage" });
    button.focus(); await fireEvent.click(button);
    expect(view.getAllByText("Shared coverage fixture.")).toHaveLength(1);
    expect(view.getByRole("region", { name: "Visibility limits" }).textContent).toContain("Applies to 3 agents");
    await fireEvent.keyDown(view.getByRole("heading", { name: "Coverage & sources" }), { key: "Escape" });
    expect(view.queryByRole("region", { name: "Visibility limits" })).toBeNull(); expect(document.activeElement).toBe(button);
  });
  it("keeps no-match recovery separate from automatic selection", async () => {
    const onclearquery = vi.fn(), view = mount(report(), { query: "no-such-capability", onclearquery });
    expect(view.getByRole("heading", { name: /No .*match/ })).toBeTruthy();
    expect(noDetails(view)).toBe(true);
    await fireEvent.click(view.getByRole("button", { name: "Clear search" })); expect(onclearquery).toHaveBeenCalledOnce();
  });
  it("shows loading and no-running-agent states without claiming installed inventory", async () => {
    const view = mount(null, { loading: true }); expect(view.getByText("Loading agents…")).toBeTruthy();
    await view.rerender({ report: { ...report(), agents: [] }, selectedAgent: null, onselectagent() {} });
    expect(view.getByRole("heading", { name: "No running agents to inspect" })).toBeTruthy();
    expect(view.container.textContent).toContain("Only running agents are listed");
  });
  it("retains exact selection across refresh and closes it when its definition disappears", async () => {
    const r = report(), view = mount(r);
    await fireEvent.click(view.getByRole("button", { name: cursorMcp("filesystem") }));
    await view.rerender({ report: structuredClone(r), selectedAgent: "agent:52000", reduced: true, onselectagent() {} });
    expect(details(view).textContent).toContain("filesystem");
    const next = structuredClone(r); next.agents[1].capabilities!.mcp_servers = next.agents[1].capabilities!.mcp_servers.filter((item) => item.name !== "filesystem");
    await view.rerender({ report: next, selectedAgent: "agent:52000", reduced: true, onselectagent() {} });
    await waitFor(() => expect(noDetails(view)).toBe(true));
    expect(view.getByRole("complementary", { name: "Capability agents" })).toBeTruthy();
  });
  it("renders untrusted purpose as text while retaining the folder identity", async () => {
    const r = report(), skill = r.agents[1].capabilities!.skills[1];
    skill.declared_name = "UI verification"; skill.description = '<img src=x onerror="alert(1)">';
    const view = mount(r); await fireEvent.click(view.getByRole("button", { name: frontendSkill("UI verification") }));
    expect(view.container.querySelector("img")).toBeNull(); expect(details(view).textContent).toContain(skill.description);
    await tab(view, "Source"); expect(details(view).textContent).toContain("frontend-checks");
  });
  it("opens a card's details inline right after it and closes them with Escape", async () => {
    const view = mount(), card = view.getByRole("button", { name: frontendSkill() });
    card.focus(); await fireEvent.click(card);
    const panel = view.getByRole("region", { name: "Capability details" });
    expect(card.nextElementSibling).toBe(panel);
    expect(card.getAttribute("aria-expanded")).toBe("true");
    expect(card.getAttribute("aria-controls")).toBe(panel.id);
    expect(view.queryByRole("complementary", { name: "Capability details" })).toBeNull();
    await waitFor(() => expect(panel.textContent).toContain("Scan with…"));
    await fireEvent.keyDown(view.getByRole("heading", { name: "frontend-checks" }), { key: "Escape" });
    expect(noDetails(view)).toBe(true);
    expect(document.activeElement).toBe(card);
    await fireEvent.click(card); await fireEvent.click(card);
    expect(noDetails(view)).toBe(true);
  });
  it("lists contexts by project folder and opens an agent under its process row", async () => {
    const view = mount();
    const cursor = view.getByRole("button", { name: /^Explore context Cursor,/ });
    expect(cursor.textContent).toContain("docs +1");
    expect(cursor.textContent).not.toContain("/Users/dev/code");
    expect(cursor.getAttribute("title")).toContain("/Users/dev/code/shop-web");
    await fireEvent.click(view.getByRole("button", { name: "Show processes for Cursor" }));
    const row = view.getByRole("button", { name: "Inspect agent card Cursor, PID 52000" });
    row.focus(); await fireEvent.click(row);
    const panel = view.getByRole("region", { name: "Capability details" });
    expect(row.nextElementSibling).toBe(panel);
    expect(panel.textContent).toContain("PID 52000");
    await fireEvent.keyDown(panel, { key: "Escape" });
    expect(noDetails(view)).toBe(true);
    expect(document.activeElement).toBe(row);
  });
  it("labels Compare contexts by name and project folder only", async () => {
    const view = mount(report(), { selectedAgent: null });
    await fireEvent.click(view.getByRole("radio", { name: "Compare" }));
    await fireEvent.click(view.getByRole("combobox", { name: /^Left context:/ }));
    for (const option of view.getAllByRole("option")) {
      expect(option.textContent).not.toMatch(/\/Users\/|process/);
      expect(option.textContent!.length).toBeLessThan(40);
    }
  });
  it("keeps the Overview entry point explicitly opt-in", async () => {
    const r = mockAgents(), oncapabilities = vi.fn();
    const view = render(AgentCard, { agent: r.agents[1], report: r, entries: MOCK_SNAPSHOT.entries, selected: true, color: "var(--accent)", onselect() {}, onentry() {}, onreveal() {}, oneditor() {}, onstop() {}, onstopall() {}, oncapabilities });
    expect(oncapabilities).not.toHaveBeenCalled();
    await fireEvent.click(view.getByRole("button", { name: /MCP & skills.*inspect sources/ }));
    expect(oncapabilities).toHaveBeenCalledOnce(); expect(r.agents[1].capabilities).toBeUndefined();
  });
});
