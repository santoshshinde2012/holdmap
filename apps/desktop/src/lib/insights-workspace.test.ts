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

describe("Insights tab", () => {
  it("shows findings per agent, including agents that aren't running", async () => {
    withDemoFindings();
    const onselectagent = vi.fn();
    render(InsightsWorkspace, { report: mockAgents(undefined, true), inventory: true, onselectagent });
    const list = screen.getByRole("navigation", { name: "Agents with insights" });
    expect(list.textContent).toContain("Codex CLI");
    expect(list.textContent).toContain("not running");
    // The agent with the most severe findings opens first.
    expect(screen.getByRole("heading", { level: 3 }).textContent).toContain("Claude Code");
    expect(screen.getByText("Instruction Override", { exact: false })).toBeTruthy();

    await fireEvent.click(screen.getByRole("button", { name: /^Codex CLI/ }));
    expect(screen.getByRole("heading", { level: 3 }).textContent).toContain("not running");
    expect(screen.getByText("Destructive Shell Command", { exact: false })).toBeTruthy();
    expect(onselectagent).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: /^Cursor/ }));
    expect(onselectagent).toHaveBeenCalledWith("agent:52000");
  });

  it("says when nothing is on and offers the inventory when it's off", async () => {
    const oninventory = vi.fn();
    render(InsightsWorkspace, { report: mockAgents(), inventory: false, oninventory });
    expect(screen.getByText(/No analyzer is on/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Turn on inventory" }));
    expect(oninventory).toHaveBeenCalled();
  });
});
