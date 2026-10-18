// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import type { Preview, ToolReport } from "./analyzers";
import { SAMPLE_TOOLS, PLACEHOLDERS } from "./analyzers-demo-mcp";

const api = {
  addAnalyzer: vi.fn(), addMcpAnalyzer: vi.fn(), analyzerPresets: vi.fn(), addAnalyzerPreset: vi.fn(), configuredMcpServers: vi.fn(), analyzerTools: vi.fn(),
  listAnalyzerTools: vi.fn(), mapAnalyzerTools: vi.fn(), mapAnalyzerOutput: vi.fn(),
};
vi.mock("./api", () => api);
const { default: AddAnalyzer } = await import("../components/analyzers/AddAnalyzer.svelte");
const { default: PermissionsSheet } = await import("../components/analyzers/PermissionsSheet.svelte");

afterEach(() => { cleanup(); for (const f of Object.values(api)) f.mockReset(); });

const report: ToolReport = {
  snapshot: { endpoint: "my-mcp --stdio", server: { name: "sample", version: "1", instructions: null }, tools: SAMPLE_TOOLS, sha256: "ab".repeat(32), listed_at_ms: 1, truncated: false },
  flags: { helper: ["mentions credential or configuration files"] },
  server_flags: [],
  suggestions: [
    { tool: "list_files", args: { files: "{subject.files}", skill_name: "{subject.name}" }, missing: [] },
    { tool: "scan_skill", args: { path: "{subject.dir}" }, missing: [] },
    { tool: "word_count", args: { content: "{subject.skill_md}" }, missing: [] },
    { tool: "helper", args: {}, missing: [] },
  ],
  changes: [], approved: false, chosen: [], placeholders: PLACEHOLDERS,
  output: {}, output_suggestions: { list_files: { items: "$.issues[*]", rule_id: "code", severity: "level", title: "msg", file: "file" } },
};

async function openMode(label: string) {
  api.analyzerPresets.mockResolvedValue([]);
  await fireEvent.click(screen.getByRole("button", { name: "Add your own analyzer…" }));
  await fireEvent.click(screen.getByRole("radio", { name: label }));
}

describe("adding your own analyzer", () => {
  it("opens on presets and adds one by id, off until approved", async () => {
    api.analyzerPresets.mockResolvedValue([
      { id: "pycqa.bandit", name: "Bandit", publisher: "PyCQA", summary: "Risky Python.", homepage: "https://github.com/PyCQA/bandit", license: "Apache-2.0", install: "uv tool install bandit", network: [], added: false, installed: false },
      { id: "google.osv-scanner", name: "OSV-Scanner", publisher: "Google", summary: "Vulnerable deps.", homepage: "https://github.com/google/osv-scanner", license: "Apache-2.0", install: "go install osv", network: ["api.osv.dev"], added: true, installed: true },
    ]);
    api.addAnalyzerPreset.mockResolvedValue({ id: "pycqa.bandit", kind: "cli" });
    const onadded = vi.fn();
    render(AddAnalyzer, { onadded });
    await fireEvent.click(screen.getByRole("button", { name: "Add your own analyzer…" }));
    expect(await screen.findByText("Bandit")).toBeTruthy();
    expect(screen.getByText(/no network/)).toBeTruthy();
    expect(screen.getByText(/contacts api\.osv\.dev/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add OSV-Scanner" })).toBeNull();
    // Installed or not, with the install command only where it's still needed.
    expect(screen.getByText("Not installed")).toBeTruthy();
    expect(screen.getByText("Installed")).toBeTruthy();
    expect(screen.getByText("uv tool install bandit")).toBeTruthy();
    expect(screen.queryByText("go install osv")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Add Bandit" }));
    expect(api.addAnalyzerPreset).toHaveBeenCalledWith("pycqa.bandit");
    await vi.waitFor(() => expect(onadded).toHaveBeenCalledWith("pycqa.bandit"));
  });

  it("sends the pasted manifest and reports the new id", async () => {
    api.addAnalyzer.mockResolvedValue({ id: "me.scanner", kind: "cli" });
    const onadded = vi.fn();
    render(AddAnalyzer, { onadded });
    await openMode("Manifest");
    const add = screen.getByRole("button", { name: "Add analyzer" }) as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    await fireEvent.input(screen.getByLabelText("Analyzer manifest (TOML)"), { target: { value: "schema = 1" } });
    await fireEvent.click(add);
    expect(api.addAnalyzer).toHaveBeenCalledWith("schema = 1");
    await vi.waitFor(() => expect(onadded).toHaveBeenCalledWith("me.scanner"));
  });

  it("shows holdmap's validation error", async () => {
    api.addAnalyzer.mockRejectedValue("invalid manifest: missing runner");
    render(AddAnalyzer, {});
    await openMode("Manifest");
    await fireEvent.input(screen.getByLabelText("Analyzer manifest (TOML)"), { target: { value: "x" } });
    await fireEvent.click(screen.getByRole("button", { name: "Add analyzer" }));
    expect(await screen.findByText(/missing runner/)).toBeTruthy();
  });

  it("adds any MCP server from a command, lists its tools only when asked and saves the suggested mapping", async () => {
    api.addMcpAnalyzer.mockResolvedValue({ id: "local.scan", name: "Scan", kind: "mcp" });
    api.analyzerTools.mockRejectedValue("Scan's tools haven't been listed yet.");
    api.listAnalyzerTools.mockResolvedValue(report);
    api.mapAnalyzerTools.mockResolvedValue({});
    api.mapAnalyzerOutput.mockResolvedValue({});
    const onadded = vi.fn();
    render(AddAnalyzer, { onadded });
    await openMode("Command");
    await fireEvent.input(screen.getByLabelText("Id"), { target: { value: "local.scan" } });
    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Scan" } });
    await fireEvent.input(screen.getByLabelText("Command"), { target: { value: "/opt/bin/my-mcp" } });
    await fireEvent.input(screen.getByLabelText("Arguments (one per line)"), { target: { value: "--stdio\n\n--quiet" } });
    await fireEvent.click(screen.getByRole("button", { name: "Add and choose tools…" }));
    expect(api.addMcpAnalyzer).toHaveBeenCalledWith({ id: "local.scan", name: "Scan", transport: "stdio", command: "/opt/bin/my-mcp", args: ["--stdio", "--quiet"], secrets: [] });
    const list = await screen.findByRole("button", { name: "List tools" });
    expect(api.listAnalyzerTools).not.toHaveBeenCalled();
    await fireEvent.click(list);
    expect(await screen.findByText("mentions credential or configuration files", { exact: false })).toBeTruthy();
    // Fully suggested, unflagged tools are preselected; the flagged one isn't.
    expect((screen.getByRole("checkbox", { name: "List scripts (list_files)" }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByRole("checkbox", { name: "helper" }) as HTMLInputElement).checked).toBe(false);
    const args = screen.getByLabelText("Example arguments for list_files").textContent ?? "";
    expect(JSON.parse(args)).toEqual({ files: ["SKILL.md", "scripts/run.sh"], skill_name: "example-skill" });
    await fireEvent.click(screen.getByRole("button", { name: "Use the mapping suggested by list_files" }));
    await fireEvent.click(screen.getByRole("button", { name: "Save tools" }));
    await vi.waitFor(() => expect(onadded).toHaveBeenCalledWith("local.scan"));
    const saved = api.mapAnalyzerTools.mock.calls[0][1].map((t: { name: string }) => t.name);
    expect(saved).toEqual(["list_files", "scan_skill", "word_count"]);
    expect(api.mapAnalyzerOutput).toHaveBeenCalledWith("local.scan", expect.objectContaining({ items: "$.issues[*]", rule_id: "code" }));
  });

  it("warns that a URL on another machine is metadata only and needs an opt-in", async () => {
    render(AddAnalyzer, {});
    await openMode("Local URL");
    await fireEvent.input(screen.getByLabelText("URL"), { target: { value: "https://scan.example.com/mcp" } });
    expect(screen.getByText(/another machine/)).toBeTruthy();
  });

  it("the permissions sheet shows MCP notices, exact tool calls and the remote opt-in", async () => {
    const preview = {
      analyzer: {
        id: "me.mcp", name: "My MCP scanner", publisher: "me", license: "MIT", homepage: "https://example.com",
        summary: "s", kind: "mcp", state: "not_approved", changes: [], source: "user",
        permissions: { network: ["scan.example.com"], sends: [] },
        mcp: { transport: "http", endpoint: "https://scan.example.com/mcp", remote: true, remote_opt_in: false, tools: ["list_files"], listing: null, needs_tools: false },
      },
      subjects: [], skipped: [], payload: [], env: [], command: ["connects to https://scan.example.com/mcp"],
      sandbox: "Not sandboxed", schedule: "Only when you press Run.",
      notices: ["Warning: this looks like the MCP server Claude Code is configured to use (scanner)."],
      projection: ["file names in the skill folder"],
      tool_calls: [{ tool: "list_files", subject: "deploy", arguments: { files: ["SKILL.md"] }, problems: [] }],
    } as unknown as Preview;
    const onapprove = vi.fn();
    render(PermissionsSheet, { preview, onapprove, oncancel: () => {} });
    expect(screen.getByText(/Warning: this looks like/)).toBeTruthy();
    expect(screen.getByText("file names in the skill folder")).toBeTruthy();
    expect(screen.getByLabelText("Arguments for list_files on deploy").textContent).toContain("SKILL.md");
    const on = screen.getByRole("button", { name: /Turn on My MCP scanner/ }) as HTMLButtonElement;
    expect(on.disabled).toBe(true);
    await fireEvent.click(screen.getByRole("checkbox", { name: /Allow this server on another machine/ }));
    expect(on.disabled).toBe(false);
    await fireEvent.click(on);
    expect(onapprove).toHaveBeenCalledWith(true);
  });
});
