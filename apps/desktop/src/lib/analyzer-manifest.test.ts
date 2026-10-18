import { describe, expect, it } from "vitest";
import { parseToml, readManifest } from "./analyzer-manifest";
import { DemoAnalyzers } from "./analyzers-demo";
import { mockAgents } from "./mock";

const MCP = `schema = 1
id = "example.mcp-scan"
name = "Example MCP Scan"
publisher = "Example"
homepage = "https://example.com"
license = "MIT"
summary = "Scans a skill folder through a local MCP server."

[runner]
kind = "mcp"
command = "example-mcp"
args = ["--stdio"]
per_subject = "skill"
tool = "scan_skill"

[runner.tool_args]
path = "{subject.dir}"

[permissions]
read = ["{subject.dir}"]
spawns_mcp_servers = true

[output]
format = "sarif"
`;

describe("demo analyzer manifests", () => {
  it("reads the TOML subset manifests use", () => {
    const t = parseToml(MCP);
    expect(t[""].schema).toBe(1);
    expect(t.runner.args).toEqual(["--stdio"]);
    expect(t["runner.tool_args"].path).toBe("{subject.dir}");
    expect(t.permissions.spawns_mcp_servers).toBe(true);
    expect(() => parseToml("nonsense")).toThrow(/line 1/);
  });

  it("applies holdmap's main rules", () => {
    expect(readManifest(MCP, []).tool).toBe("scan_skill");
    const bad: [string, string, RegExp][] = [
      ['id = "example.mcp-scan"', 'id = "Bad_ID"', /reverse-DNS/],
      ['command = "example-mcp"', 'command = "bash"', /shell or launcher/],
      ["spawns_mcp_servers = true", "spawns_mcp_servers = false", /spawns_mcp_servers/],
      ['path = "{subject.dir}"', 'path = "/etc"', /subject\.dir/],
      ['homepage = "https://example.com"', 'homepage = "http://example.com"', /https/],
    ];
    for (const [from, to, msg] of bad) expect(() => readManifest(MCP.replace(from, to), []), to).toThrow(msg);
    expect(() => readManifest(MCP, ["example.mcp-scan"])).toThrow(/catalog/);
  });

  it("the demo adds, previews, refuses automatic runs for MCP, runs and removes", async () => {
    const demo = new DemoAnalyzers();
    const view = demo.add(MCP);
    expect(view).toMatchObject({ id: "example.mcp-scan", source: "user", kind: "mcp", state: "not_approved" });
    expect(demo.list().map((a) => a.id)).toContain("example.mcp-scan");
    expect(() => demo.add(MCP)).toThrow(/already registered/);
    const report = mockAgents(undefined, true);
    const p = demo.preview("example.mcp-scan", report);
    expect(p.notices?.[0]).toMatch(/starts it only when you run this analyzer/);
    expect(p.command?.at(-1)).toMatch(/calls tool scan_skill\(path=<copy of the skill folder>\) once per skill/);
    expect(p.schedule).toMatch(/never run on their own/);
    demo.approve("example.mcp-scan");
    expect(() => demo.setBackground("example.mcp-scan", true)).toThrow(/only when you click Run now/);
    const r = demo.run("example.mcp-scan", report);
    expect(r.notes[0]).toMatch(/no MCP server was started/);
    expect(() => demo.remove("nvidia.skillspector")).toThrow(/can't be removed/);
    expect(demo.remove("example.mcp-scan")).toBe(true);
    expect(demo.list().map((a) => a.id)).not.toContain("example.mcp-scan");
  });
});
