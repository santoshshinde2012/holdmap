// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import type { Preview } from "./analyzers";

const addSpy = vi.fn();
vi.mock("./api", () => ({ addAnalyzer: (m: string) => addSpy(m) }));
const { default: AddAnalyzer } = await import("../components/analyzers/AddAnalyzer.svelte");
const { default: PermissionsSheet } = await import("../components/analyzers/PermissionsSheet.svelte");

afterEach(() => { cleanup(); addSpy.mockReset(); });

describe("adding your own analyzer", () => {
  it("sends the pasted manifest and reports the new id", async () => {
    addSpy.mockResolvedValue({ id: "me.scanner" });
    const onadded = vi.fn();
    render(AddAnalyzer, { onadded });
    await fireEvent.click(screen.getByRole("button", { name: "Add your own analyzer…" }));
    const add = screen.getByRole("button", { name: "Add analyzer" }) as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    await fireEvent.input(screen.getByLabelText("Analyzer manifest (TOML)"), { target: { value: "schema = 1" } });
    await fireEvent.click(add);
    expect(addSpy).toHaveBeenCalledWith("schema = 1");
    await vi.waitFor(() => expect(onadded).toHaveBeenCalledWith("me.scanner"));
  });

  it("shows holdmap's validation error", async () => {
    addSpy.mockRejectedValue("invalid manifest: missing runner");
    render(AddAnalyzer, {});
    await fireEvent.click(screen.getByRole("button", { name: "Add your own analyzer…" }));
    await fireEvent.input(screen.getByLabelText("Analyzer manifest (TOML)"), { target: { value: "x" } });
    await fireEvent.click(screen.getByRole("button", { name: "Add analyzer" }));
    expect(await screen.findByText(/missing runner/)).toBeTruthy();
  });

  it("the permissions sheet shows MCP notices and the configured-server warning", () => {
    const preview = {
      analyzer: {
        id: "me.mcp", name: "My MCP scanner", publisher: "me", license: "MIT", homepage: "https://example.com",
        summary: "s", kind: "mcp", state: "off", changes: [], source: "user",
        permissions: { network: [], sends: [] },
      },
      subjects: [], skipped: [], payload: [], env: [], command: ["my-mcp"],
      sandbox: "Basic", schedule: "Only when you press Run.",
      notices: [
        "holdmap starts this MCP server when you press Run and stops it when the run ends.",
        "Warning: this looks like the MCP server Claude Code is configured to use (scanner).",
      ],
    } as unknown as Preview;
    render(PermissionsSheet, { preview, onapprove: () => {}, oncancel: () => {} });
    expect(screen.getByText(/starts this MCP server when you press Run/)).toBeTruthy();
    expect(screen.getByText(/Warning: this looks like/)).toBeTruthy();
    expect(screen.getByText(/never starts your agents' servers/)).toBeTruthy();
  });
});
