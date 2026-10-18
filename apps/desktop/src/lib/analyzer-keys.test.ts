// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";

const calls: unknown[][] = [];
vi.mock("./api", () => ({
  setAnalyzerSecret: vi.fn(async (...a: unknown[]) => { calls.push(["set", ...a]); }),
  deleteAnalyzerSecret: vi.fn(async (...a: unknown[]) => { calls.push(["delete", ...a]); return true; }),
}));
const { default: AnalyzerKeys } = await import("../components/analyzers/AnalyzerKeys.svelte");

afterEach(() => { cleanup(); calls.length = 0; });

describe("analyzer API keys", () => {
  it("sends a pasted key once and clears the field", async () => {
    const onchange = vi.fn();
    render(AnalyzerKeys, { id: "acme.scan", name: "Acme Scan", secrets: [{ name: "ACME_API_KEY", stored: false }], onchange });
    const field = screen.getByLabelText(/ACME_API_KEY/) as HTMLInputElement;
    expect(field.type).toBe("password");
    await fireEvent.input(field, { target: { value: "k-secret" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save ACME_API_KEY" }));
    expect(calls).toEqual([["set", "acme.scan", "ACME_API_KEY", "k-secret"]]);
    expect(field.value).toBe("");
    expect(onchange).toHaveBeenCalled();
  });

  it("offers Delete only for a stored key and renders nothing without keys", async () => {
    render(AnalyzerKeys, { id: "acme.scan", name: "Acme Scan", secrets: [{ name: "ACME_API_KEY", stored: true }] });
    expect(screen.getByText("stored")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Delete ACME_API_KEY" }));
    expect(calls).toEqual([["delete", "acme.scan", "ACME_API_KEY"]]);
    cleanup();
    const { container } = render(AnalyzerKeys, { id: "x", name: "X", secrets: [] });
    expect(container.textContent?.trim()).toBe("");
  });
});
