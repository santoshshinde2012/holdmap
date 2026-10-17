// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import HomeView from "./HomeView.svelte";
import { MOCK_SNAPSHOT, mockAgents } from "../../lib/mock";

beforeAll(() => {
  window.matchMedia ??= ((media: string) => ({ matches: true, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  const animate = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(animate);
  Element.prototype.scrollIntoView ??= vi.fn();
});
afterEach(cleanup);

describe("HomeView", () => {
  it("opens an agent in one click", async () => {
    const report = mockAgents();
    const onopenagent = vi.fn();
    const view = render(HomeView, { entries: MOCK_SNAPSHOT.entries, report, onopenagent, onopenentry: vi.fn(), onview: vi.fn() });
    const agent = report.agents[0];
    await fireEvent.click(view.getByRole("button", { name: new RegExp(agent.name) }));
    expect(onopenagent).toHaveBeenCalledWith(agent.id);
  });

  it("shows unknown counts before the first scan instead of zero", () => {
    const view = render(HomeView, { entries: null, report: null, onopenagent: vi.fn(), onopenentry: vi.fn(), onview: vi.fn() });
    expect(view.getByRole("button", { name: "Listening ports: unknown" })).toBeTruthy();
    expect(view.getByText("Waiting for the first scan…")).toBeTruthy();
  });

  it("switches views from the summary tiles", async () => {
    const onview = vi.fn();
    const view = render(HomeView, { entries: MOCK_SNAPSHOT.entries, report: mockAgents(), onopenagent: vi.fn(), onopenentry: vi.fn(), onview });
    await fireEvent.click(view.getByRole("button", { name: /^Agents & tools:/ }));
    expect(onview).toHaveBeenCalledWith("agents");
  });
});
