// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import StatStrip from "./StatStrip.svelte";
import AnalyzerGallery from "../analyzers/AnalyzerGallery.svelte";

afterEach(cleanup);

describe("StatStrip", () => {
  it("renders tiles, buttons only for tiles that open something, and a pending value", async () => {
    const go = vi.fn();
    const view = render(StatStrip, { stats: [
      { id: "a", label: "Agents", value: 3, go, aria: "Agents 3" },
      { id: "b", label: "Exposed", value: 2, tone: "warn" },
      { id: "c", label: "MCP", value: null, go },
      { id: "d", label: "Skills scanned", value: 1, of: 4, bar: true },
    ] });
    expect(view.getByRole("group", { name: "Summary" })).toBeTruthy();
    expect(view.getAllByRole("button")).toHaveLength(1);
    await fireEvent.click(view.getByRole("button", { name: "Agents 3" }));
    expect(go).toHaveBeenCalledOnce();
    expect(view.getByText("…")).toBeTruthy();
    expect(view.container.querySelector(".bar i")?.getAttribute("style")).toContain("25%");
  });
});

describe("AnalyzerGallery add your own", () => {
  it("opens the add form in place, without presets, and closes on cancel", async () => {
    const view = render(AnalyzerGallery, {});
    await fireEvent.click(view.getByRole("button", { name: /Add your own/ }));
    expect(view.getByTestId("add-analyzer")).toBeTruthy();
    expect(view.queryByRole("radio", { name: "Presets" })).toBeNull();
    expect(view.getByRole("radio", { name: "Command" })).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
    expect(view.queryByTestId("add-analyzer")).toBeNull();
    expect(view.getByRole("button", { name: /Add your own/ })).toBeTruthy();
  });
});

import EmptyPanel from "./EmptyPanel.svelte";
describe("EmptyPanel", () => {
  it("shows a title and an optional hint as a status", () => {
    const view = render(EmptyPanel, { icon: "check", title: "All clear", hint: "Nothing needs you right now.", tone: "ok", compact: true });
    expect(view.getByRole("status").textContent).toContain("All clear");
    expect(view.getByText("Nothing needs you right now.")).toBeTruthy();
  });
});
