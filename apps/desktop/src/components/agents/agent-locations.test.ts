// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, within } from "@testing-library/svelte";
import AgentLocations from "./AgentLocations.svelte";
import { mockAgents } from "../../lib/mock-agents";
import * as places from "../../lib/mock-places";

afterEach(cleanup);
const inspected = () => mockAgents(undefined, true, places).agents.find((a) => a.capabilities?.inspected && (a.capabilities.locations ?? []).some((p) => p.present && p.path))!;

describe("AgentLocations", () => {
  it("shows a present location's folder in one click, and offers nothing to show for a missing one", async () => {
    const agent = inspected();
    const onreveal = vi.fn();
    const view = render(AgentLocations, { agent, onreveal });
    const present = (agent.capabilities!.locations ?? []).find((p) => p.present && p.path)!;
    const row = [...view.container.querySelectorAll<HTMLElement>(".place")].find((li) => li.querySelector(".path")?.getAttribute("title") === present.path)!;
    await fireEvent.click(within(row).getByRole("button", { name: "Show in folder" }));
    expect(onreveal).toHaveBeenCalledExactlyOnceWith(present.path);
    for (const li of view.container.querySelectorAll<HTMLElement>(".place-missing")) expect(within(li).queryByRole("button", { name: "Show in folder" })).toBeNull();
  });

  it("keeps Copy path only where the app can't show folders", () => {
    const view = render(AgentLocations, { agent: inspected() });
    expect(view.queryAllByRole("button", { name: "Show in folder" })).toHaveLength(0);
    expect(view.getAllByRole("button", { name: "Copy path" }).length).toBeGreaterThan(0);
  });
});
