import { describe, expect, it } from "vitest";
import { DemoAnalyzers } from "./analyzers-demo";
import { mockAgents } from "./mock";

describe("browser demo: any MCP analyzer", () => {
  it("is added, listed, mapped, approved and run with sample data only", () => {
    const d = new DemoAnalyzers();
    const report = mockAgents(undefined, true);
    const v = d.addMcp({ id: "local.scan", name: "Scan", command: "/opt/bin/scan", args: ["--stdio"] });
    expect(v.state).toBe("not_approved");
    expect(v.mcp?.needs_tools).toBe(true);
    expect(() => d.tools("local.scan")).toThrow(/haven't been listed/);
    expect(() => d.approve("local.scan")).toThrow(/no tools chosen/);
    const r = d.listTools("local.scan");
    expect(r.flags.helper.length).toBeGreaterThan(0);
    d.mapTools("local.scan", [{ name: "list_files", args: { files: "{subject.files}" } }]);
    d.mapOutput("local.scan", r.output_suggestions.list_files);
    const p = d.preview("local.scan", report);
    expect(p.projection).toEqual(["file names in the skill folder"]);
    expect(p.tool_calls?.[0].arguments.files).toEqual(["SKILL.md", "scripts/run.sh"]);
    expect(d.approve("local.scan").state).toBe("approved");
    const result = d.run("local.scan", report);
    expect(result.notes[0]).toMatch(/Browser demo/);
    expect(result.tool_calls?.[0].tool).toBe("list_files");
    // Changing the mapping needs approval again.
    d.mapTools("local.scan", [{ name: "word_count", args: { content: "{subject.skill_md}" } }]);
    expect(d.list().find((a) => a.id === "local.scan")?.state).toBe("changed");
    expect(() => d.run("local.scan", report)).toThrow(/changed/);
  });

  it("keeps remote servers metadata-only and behind an explicit opt-in", () => {
    const d = new DemoAnalyzers();
    expect(() => d.addMcp({ id: "local.r", name: "R", transport: "http", url: "http://scan.example.com/mcp" })).toThrow(/https/);
    d.addMcp({ id: "local.r", name: "R", transport: "http", url: "https://scan.example.com/mcp" });
    d.listTools("local.r");
    expect(() => d.mapTools("local.r", [{ name: "scan_skill", args: { path: "{subject.dir}" } }])).toThrow(/metadata only/);
    d.mapTools("local.r", [{ name: "list_files", args: { files: "{subject.files}" } }]);
    expect(() => d.approve("local.r")).toThrow(/another machine/);
    expect(d.approve("local.r", true).mcp?.remote_opt_in).toBe(true);
    expect(d.list().find((a) => a.id === "local.r")?.permissions.network).toEqual(["scan.example.com"]);
  });
});
