// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import AgentConfigPanel from "./AgentConfigPanel.svelte";
import { mockAgents } from "../../lib/mock";

afterEach(cleanup);

describe("AgentConfigPanel", () => {
  const claude = mockAgents(undefined, true).agents.find((a) => a.product === "claude-code")!;
  const cursor = mockAgents(undefined, true).agents.find((a) => a.product === "cursor")!;

  it("separates configured servers from observed running ones and lists names only", () => {
    const view = render(AgentConfigPanel, { agent: cursor });
    expect(view.container.querySelector(".run.tone-ok")?.textContent).toBe("Running · 52070");
    expect(view.getByText("unpinned")).toBeTruthy();
    expect(view.getByText("api.githubcopilot.com")).toBeTruthy();
    expect(view.getByText("headers Authorization")).toBeTruthy();
    expect(view.getByText("Disabled")).toBeTruthy();
  });

  it("shows permission rules, settings, instructions and documented built-ins", () => {
    const view = render(AgentConfigPanel, { agent: claude });
    expect(view.getByText("Bash(curl:*)")).toBeTruthy();
    expect(view.getByText("Denied")).toBeTruthy();
    expect(view.getByText("permissions.defaultMode")).toBeTruthy();
    expect(view.getByText("Run checks and open a pull request.")).toBeTruthy();
    expect(view.getByText("env GITHUB_PERSONAL_ACCESS_TOKEN")).toBeTruthy();
    expect(view.getByText("(documented, not observed)")).toBeTruthy();
  });

  it("offers to read configuration when it wasn't inspected", async () => {
    const oninspect = vi.fn();
    const plain = mockAgents().agents[0];
    const view = render(AgentConfigPanel, { agent: plain, oninspect });
    await fireEvent.click(view.getByRole("button", { name: "Read configuration" }));
    expect(oninspect).toHaveBeenCalled();
  });
});
