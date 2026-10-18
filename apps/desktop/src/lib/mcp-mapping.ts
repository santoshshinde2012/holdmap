// Pure helpers for the MCP tool mapping editor: read a tool's input schema into fields, parse
// what the user typed, and preview the exact arguments the same way holdmap builds them
// (placeholders filled, values coerced to the schema's types). The desktop app's real preview
// comes from holdmap itself; this only drives the editor as the user types.
import type { JsonSchema, PlaceholderInfo, Suggestion, ToolInfo, ToolReport, ToolSpec } from "./analyzers";

export interface SchemaField {
  name: string;
  types: string[];
  required: boolean;
  description: string | null;
  enumValues: unknown[] | null;
  /** The schema wants an array. */
  list: boolean;
}

export interface SampleSubject { dir: string; skill_md: string; files: string[]; name: string; agents: string[] }

const types = (s: JsonSchema | undefined): string[] => (s?.type === undefined ? [] : Array.isArray(s.type) ? s.type : [s.type]);

/** The fields of a tool's input schema, required first, then by name. */
export function schemaFields(schema: JsonSchema | null | undefined): SchemaField[] {
  const required = new Set(schema?.required ?? []);
  return Object.entries(schema?.properties ?? {})
    .map(([name, p]) => ({
      name,
      types: types(p),
      required: required.has(name),
      description: typeof p.description === "string" ? p.description : null,
      enumValues: Array.isArray(p.enum) ? p.enum : null,
      list: types(p).includes("array"),
    }))
    .sort((a, b) => Number(b.required) - Number(a.required) || a.name.localeCompare(b.name));
}

/** NAME=VALUE value: JSON numbers, booleans, lists and objects; anything else is text. */
export function parseValue(text: string): unknown {
  const t = text.trim();
  if (t === "") return "";
  try {
    const v = JSON.parse(t);
    if (typeof v === "number" || typeof v === "boolean" || Array.isArray(v) || (v && typeof v === "object")) return v;
  } catch { /* text */ }
  return text;
}

/** How a mapped value is shown in an input box. */
export function valueText(v: unknown): string {
  return typeof v === "string" ? v : v === undefined ? "" : JSON.stringify(v);
}

function resolve(v: unknown, s: SampleSubject): unknown {
  if (typeof v !== "string") return Array.isArray(v) ? v.map((x) => resolve(x, s)) : v;
  if (v === "{subject.files}") return s.files;
  if (v === "{subject.agents}") return s.agents;
  return v.replaceAll("{subject.dir}", s.dir).replaceAll("{subject.skill_md}", s.skill_md).replaceAll("{subject.name}", s.name);
}

function coerce(v: unknown, schema: JsonSchema | undefined): unknown {
  const ts = types(schema);
  if (!ts.length) return v;
  if (Array.isArray(v) && !ts.includes("array") && ts.includes("string")) return v.map(String).join("\n");
  if (!Array.isArray(v) && ts.includes("array") && !ts.includes(typeof v as string)) return [coerce(v, schema?.items)];
  if (typeof v === "string" && !ts.includes("string")) {
    if (ts.includes("integer") && /^-?\d+$/.test(v.trim())) return Number(v);
    if (ts.includes("number") && v.trim() !== "" && !Number.isNaN(Number(v))) return Number(v);
    if (ts.includes("boolean") && (v === "true" || v === "false")) return v === "true";
  }
  return v;
}

/** The arguments a mapping would send for `sample`, coerced to the schema. */
export function previewArguments(args: Record<string, unknown>, schema: JsonSchema | null | undefined, sample: SampleSubject): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(args)) out[k] = coerce(resolve(v, sample), schema?.properties?.[k]);
  return out;
}

const typeOf = (v: unknown): string => (Array.isArray(v) ? "array" : v === null ? "null" : Number.isInteger(v) ? "integer" : typeof v);

/** Problems with arguments against the schema: missing required fields, types, enums, extras. */
export function checkArguments(args: Record<string, unknown>, schema: JsonSchema | null | undefined): string[] {
  const problems: string[] = [];
  for (const r of schema?.required ?? []) if (!(r in args)) problems.push(`${r} is required`);
  for (const [k, v] of Object.entries(args)) {
    const p = schema?.properties?.[k];
    if (!p) {
      if (schema?.additionalProperties === false) problems.push(`${k} isn't an input of this tool`);
      continue;
    }
    const ts = types(p);
    const t = typeOf(v);
    if (ts.length && !ts.includes(t) && !(t === "integer" && ts.includes("number"))) problems.push(`${k} should be ${ts.join(" or ")}`);
    if (p.enum && !p.enum.some((e) => JSON.stringify(e) === JSON.stringify(v))) problems.push(`${k} must be one of ${p.enum.map((e) => JSON.stringify(e)).join(", ")}`);
  }
  return problems;
}

/** Placeholders that fit a field (lists only where the schema takes a list or text). */
export function placeholdersFor(field: SchemaField, all: PlaceholderInfo[]): PlaceholderInfo[] {
  return all.filter((p) => !p.list || field.list || field.types.includes("string") || !field.types.length);
}

/** The starting mapping for a tool: what is saved, else holdmap's suggestion. */
export function initialSpec(report: ToolReport, tool: ToolInfo): ToolSpec {
  const saved = report.chosen.find((c) => c.name === tool.name);
  if (saved) return structuredClone(saved);
  const s: Suggestion | undefined = report.suggestions.find((x) => x.tool === tool.name);
  return { name: tool.name, args: { ...(s?.args ?? {}) } };
}

/** Tools worth pre-selecting: fully suggested, no warnings, not marked destructive. */
export function preselect(report: ToolReport): string[] {
  if (report.chosen.length) return report.chosen.map((c) => c.name);
  return report.snapshot.tools
    .filter((t) => !report.flags[t.name]?.length && t.annotations?.destructiveHint !== true)
    .filter((t) => report.suggestions.some((s) => s.tool === t.name && !s.missing.length && Object.keys(s.args).length))
    .map((t) => t.name);
}
