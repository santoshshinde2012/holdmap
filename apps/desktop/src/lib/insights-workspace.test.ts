// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import InsightsWorkspace from "../components/analyzers/InsightsWorkspace.svelte";
import { inventories } from "./agent-config";
import { DemoAnalyzers } from "./analyzers-demo";
import { insightsStore } from "./insights-store.svelte";
import { mockAgents } from "./mock";

afterEach(() => { cleanup(); insightsStore.insights = null; insightsStore.analyzers = []; });

function withDemoFindings() {
  const demo = new DemoAnalyzers();
  insightsStore.insights = demo.insights();
  insightsStore.analyzers = demo.list();
}

describe("inventories", () => {
  it("lists running agents, then configured agents that aren't running", () => {
    const list = inventories(mockAgents(undefined, true));
    expect(list.filter((i) => i.running).map((i) => i.product)).toEqual(["claude-code", "cursor", "docker-desktop"]);
    expect(list.filter((i) => !i.running)).toMatchObject([{ product: "codex", name: "Codex CLI", id: null }]);
    expect(inventories(mockAgents())).toHaveLength(3);
    expect(inventories(null)).toEqual([]);
  });
});

describe("Security tab", () => {
  it("shows a summary and findings grouped by severity, expandable inline", async () => {
    withDemoFindings();
    render(InsightsWorkspace, { report: mockAgents(undefined, true), inventory: true });
    const summary = screen.getByRole("group", { name: "Security summary" });
    expect(summary.textContent).toContain("Open findings");
    expect(summary.textContent).toContain("Agents scanned");
    expect(screen.getByRole("region", { name: "High findings" })).toBeTruthy();
    const row = screen.getByRole("button", { name: /Instruction Override/ });
    expect(row.getAttribute("aria-expanded")).toBe("false");
    await fireEvent.click(row);
    expect(row.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Remove text that tells the agent to ignore prompts or safety rules.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Mark reviewed" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Rescan with SkillSpector" })).toBeTruthy();
  });

  it("filters by severity from the summary and by search", async () => {
    withDemoFindings();
    render(InsightsWorkspace, { report: mockAgents(undefined, true), inventory: true });
    await fireEvent.click(screen.getByRole("button", { name: "2 Medium" }));
    expect(screen.queryByRole("region", { name: "High findings" })).toBeNull();
    expect(screen.getByRole("region", { name: "Medium findings" })).toBeTruthy();
    await fireEvent.input(screen.getByRole("searchbox", { name: "Search findings" }), { target: { value: "zzz" } });
    expect(screen.getByText("No findings match")).toBeTruthy();
  });

  it("selects the app agent when a running agent is picked in the filter", async () => {
    withDemoFindings();
    const onselectagent = vi.fn();
    render(InsightsWorkspace, { report: mockAgents(undefined, true), inventory: true, onselectagent });
    await fireEvent.change(screen.getByRole("combobox", { name: "Agent" }), { target: { value: "cursor" } });
    expect(onselectagent).toHaveBeenCalledWith("agent:52000");
  });

  it("guides first setup when nothing is on, and offers the inventory when it's off", async () => {
    const oninventory = vi.fn();
    render(InsightsWorkspace, { report: mockAgents(), inventory: false, oninventory });
    expect(screen.getByText("No analyzer is on")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Turn on inventory" }));
    expect(oninventory).toHaveBeenCalled();
  });
});
