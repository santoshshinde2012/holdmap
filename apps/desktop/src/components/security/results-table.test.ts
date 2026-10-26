// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import ResultsTable from "./ResultsTable.svelte";
import { PAGE_SIZE, type FindingItem } from "../../lib/security-view";

afterEach(cleanup);

function items(n: number): FindingItem[] {
  return Array.from({ length: n }, (_, i) => ({
    finding: { fingerprint: `fp${i}`, analyzer: "a.b", rule_id: `R${i}`, category: null, severity: "low", title: `Finding ${i}`, subject: "s", location: { file: "SKILL.md", line: i + 1 }, fix: null, refs: [], state: "open" },
    subject: null, analyzer: "a.b", analyzerName: "A", agents: [], where: `SKILL.md:${i + 1}`, evidence: [], folder: null, scope: "unknown", path: null, isNew: i === 0,
  }));
}

describe("ResultsTable at scale", () => {
  it("draws one page of rows and adds more on request", async () => {
    const total = PAGE_SIZE * 2 + 50;
    const t0 = performance.now();
    const view = render(ResultsTable, { items: items(total), sort: { key: "severity", dir: "desc" } });
    const ms = performance.now() - t0;
    const rows = () => view.container.querySelectorAll("tbody tr").length;
    expect(rows()).toBe(PAGE_SIZE);
    expect(view.getByText(`Showing ${PAGE_SIZE} of ${total}.`)).toBeTruthy();
    expect(view.getAllByText("New")).toHaveLength(1);
    await fireEvent.click(view.getByRole("button", { name: `Show ${PAGE_SIZE} more` }));
    expect(rows()).toBe(PAGE_SIZE * 2);
    await fireEvent.click(view.getByRole("button", { name: "Show 50 more" }));
    expect(rows()).toBe(total);
    expect(view.queryByText(/^Showing/)).toBeNull();
    // Generous bound: jsdom on a CI runner; the point is no full 450-row render up front.
    expect(ms).toBeLessThan(5_000);
  });
});
