// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { DemoAnalyzers } from "./analyzers-demo";

const demo = new DemoAnalyzers();
const fetchSpy = vi.fn(async () => demo.fetchCatalog());
vi.mock("./api", () => ({
  analyzerCatalog: vi.fn(async () => demo.catalog()),
  fetchAnalyzerCatalog: () => fetchSpy(),
  acceptAnalyzerCatalog: vi.fn(async () => demo.acceptCatalog()),
  rejectAnalyzerCatalog: vi.fn(async () => demo.rejectCatalog()),
}));
const { default: CatalogUpdate } = await import("../components/analyzers/CatalogUpdate.svelte");

afterEach(() => cleanup());

describe("signed catalog updates", () => {
  it("downloads nothing until asked, then shows the diff and applies it only on Accept", async () => {
    const onchange = vi.fn();
    render(CatalogUpdate, { onchange });
    expect(await screen.findByText(/Catalog version 1 · 2026-10-18 · built into holdmap/)).toBeTruthy();
    expect(fetchSpy).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Check for a catalog update" }));
    const diff = await screen.findByRole("group", { name: "Catalog update to review" });
    expect(diff.textContent).toMatch(/version 1 → 2/);
    expect(diff.textContent).toMatch(/runner\.timeout_secs: 120 → 300/);
    expect(demo.catalog().version).toBe(1);
    await fireEvent.click(screen.getByRole("button", { name: "Accept update" }));
    expect(await screen.findByText(/Catalog version 2 · 2026-11-02 · update you accepted/)).toBeTruthy();
    expect(onchange).toHaveBeenCalled();
  });

  it("rejecting discards the staged update", async () => {
    render(CatalogUpdate, {});
    await fireEvent.click(await screen.findByRole("button", { name: "Check for a catalog update" }));
    await screen.findByRole("group", { name: "Catalog update to review" });
    await fireEvent.click(screen.getByRole("button", { name: "Reject" }));
    expect(await screen.findByRole("button", { name: "Check for a catalog update" })).toBeTruthy();
    expect(demo.catalog().pending).toBeNull();
  });
});
