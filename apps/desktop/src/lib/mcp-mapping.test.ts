import { describe, expect, it } from "vitest";
import { checkArguments, initialSpec, parseValue, placeholdersFor, preselect, previewArguments, schemaFields, valueText } from "./mcp-mapping";
import { PLACEHOLDERS, SAMPLE_TOOLS } from "./analyzers-demo-mcp";
import type { ToolReport } from "./analyzers";

const sample = { dir: "/copy/a", skill_md: "# A", files: ["SKILL.md", "run.sh"], name: "a", agents: ["cursor", "codex"] };

describe("MCP tool mapping", () => {
  it("reads schema fields, required first", () => {
    const f = schemaFields({ type: "object", required: ["path"], properties: { format: { type: "string", enum: ["sarif"] }, path: { type: "string", description: "Folder" }, tags: { type: "array" } } });
    expect(f.map((x) => x.name)).toEqual(["path", "format", "tags"]);
    expect(f[0]).toMatchObject({ required: true, description: "Folder", list: false });
    expect(f[1].enumValues).toEqual(["sarif"]);
    expect(f[2].list).toBe(true);
    expect(schemaFields(null)).toEqual([]);
  });

  it("parses typed values like the CLI and shows them back", () => {
    expect(parseValue("{subject.dir}")).toBe("{subject.dir}");
    expect(parseValue("2")).toBe(2);
    expect(parseValue("true")).toBe(true);
    expect(parseValue('["a"]')).toEqual(["a"]);
    expect(parseValue('"q"')).toBe('"q"');
    expect(valueText(["a"])).toBe('["a"]');
    expect(valueText("x")).toBe("x");
  });

  it("previews arguments with placeholders filled and coerced to the schema", () => {
    const schema = { type: "object", properties: { files: { type: "array", items: { type: "string" } }, text: { type: "string" }, list: { type: "array" }, depth: { type: "integer" }, on: { type: "boolean" } } };
    expect(previewArguments({ files: "{subject.files}", text: "{subject.agents}", list: "{subject.name}", depth: "3", on: "true" }, schema, sample))
      .toEqual({ files: ["SKILL.md", "run.sh"], text: "cursor\ncodex", list: ["a"], depth: 3, on: true });
    expect(previewArguments({ path: "{subject.dir}/x" }, null, sample)).toEqual({ path: "/copy/a/x" });
  });

  it("checks required inputs, types, enums and extras", () => {
    const schema = { type: "object", additionalProperties: false, required: ["path"], properties: { path: { type: "string" }, format: { type: "string", enum: ["sarif"] }, n: { type: "number" } } };
    expect(checkArguments({ path: "x", format: "sarif", n: 2 }, schema)).toEqual([]);
    expect(checkArguments({ format: "json", n: "2", extra: 1 }, schema)).toEqual(["path is required", "format must be one of \"sarif\"", "n should be number", "extra isn't an input of this tool"]);
  });

  it("offers list placeholders only where a list or text fits", () => {
    const [path] = schemaFields({ properties: { path: { type: "integer" } } });
    expect(placeholdersFor(path, PLACEHOLDERS).some((p) => p.list)).toBe(false);
    const [files] = schemaFields({ properties: { files: { type: "array" } } });
    expect(placeholdersFor(files, PLACEHOLDERS).some((p) => p.token === "{subject.files}")).toBe(true);
  });

  it("starts from the saved mapping, else the suggestion, and preselects only safe complete tools", () => {
    const report = {
      snapshot: { tools: SAMPLE_TOOLS }, chosen: [], flags: { helper: ["x"] },
      suggestions: [{ tool: "list_files", args: { files: "{subject.files}" }, missing: [] }, { tool: "scan_skill", args: {}, missing: ["path"] }, { tool: "helper", args: {}, missing: [] }],
    } as unknown as ToolReport;
    expect(preselect(report)).toEqual(["list_files"]);
    expect(initialSpec(report, SAMPLE_TOOLS[0])).toEqual({ name: "list_files", args: { files: "{subject.files}" } });
    const saved = { ...report, chosen: [{ name: "scan_skill", args: { path: "/fixed" } }] } as ToolReport;
    expect(preselect(saved)).toEqual(["scan_skill"]);
    expect(initialSpec(saved, SAMPLE_TOOLS[1]).args).toEqual({ path: "/fixed" });
  });
});
