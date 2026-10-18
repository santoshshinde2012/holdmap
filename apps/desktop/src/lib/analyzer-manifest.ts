// A small reader for analyzer manifests in the browser demo. It mirrors the main rules of
// holdmap's Rust validator so the simulated "Add your own analyzer" behaves like the app. The
// desktop app always validates with the real one.

type Value = string | number | boolean | string[];
export type ManifestTable = Record<string, Record<string, Value>>;

export interface DemoManifest {
  id: string; name: string; publisher: string; homepage: string; license: string; summary: string;
  kind: "cli" | "mcp"; command: string; args: string[]; tool: string | null;
  toolArgs: Record<string, string>; network: string[]; networkReason: string | null; sends: string[];
}

const SHELLS = new Set(["sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh", "cmd", "cmd.exe", "powershell", "powershell.exe", "pwsh", "pwsh.exe", "env", "sudo", "doas", "osascript"]);

function scalar(raw: string): Value {
  const v = raw.trim();
  if (v.startsWith('"')) return JSON.parse(v) as string;
  if (v === "true" || v === "false") return v === "true";
  if (/^-?\d+$/.test(v)) return Number(v);
  if (v.startsWith("[")) return (JSON.parse(v) as unknown[]).map(String);
  throw new Error(`can't read value ${v.slice(0, 40)}`);
}

/** Parse the TOML subset manifests use: sections, strings, numbers, booleans, string arrays. */
export function parseToml(text: string): ManifestTable {
  const out: ManifestTable = { "": {} };
  let section = "";
  for (const [n, line] of text.split(/\r?\n/).entries()) {
    const l = line.replace(/^\s+|\s+$/g, "");
    if (!l || l.startsWith("#")) continue;
    const head = /^\[([a-z_.]+)\]$/.exec(l);
    if (head) { section = head[1]; out[section] ??= {}; continue; }
    const kv = /^([A-Za-z0-9_]+)\s*=\s*(.+)$/.exec(l);
    if (!kv) throw new Error(`line ${n + 1}: expected key = value`);
    try { out[section][kv[1]] = scalar(kv[2]); } catch (e) { throw new Error(`line ${n + 1}: ${(e as Error).message}`); }
  }
  return out;
}

const str = (t: Record<string, Value> | undefined, k: string) => (typeof t?.[k] === "string" ? (t[k] as string) : null);
const list = (t: Record<string, Value> | undefined, k: string) => (Array.isArray(t?.[k]) ? (t[k] as string[]) : []);

/** Read and check a manifest; throws holdmap-style messages. */
export function readManifest(text: string, reserved: string[]): DemoManifest {
  const t = parseToml(text);
  const top = t[""], runner = t.runner, perms = t.permissions;
  const fail = (m: string): never => { throw new Error(`invalid manifest: ${m}`); };
  if (top.schema !== 1) fail("schema must be 1");
  const field = (k: string) => str(top, k) ?? fail(`missing ${k}`);
  const id = field("id");
  if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(id) || id.length > 64) fail("id must be reverse-DNS: lowercase letters, digits, dots and dashes");
  if (reserved.includes(id)) fail(`${id} is in holdmap's catalog; give your analyzer its own id`);
  const homepage = field("homepage");
  if (!homepage.startsWith("https://")) fail("homepage must be an https:// URL");
  if (!runner) fail("missing [runner]");
  const rawKind = str(runner, "kind");
  const kind: "cli" | "mcp" = rawKind === "cli" || rawKind === "mcp" ? rawKind : fail("runner.kind must be \"cli\" or \"mcp\"");
  const command = str(runner, "command") ?? fail("missing runner.command");
  const base = command.split(/[\\/]/).pop() ?? command;
  if (SHELLS.has(base.toLowerCase())) fail("command must not be a shell or launcher");
  if (/[;&|`$<>]/.test(command)) fail("command must not contain shell metacharacters");
  const toolArgs = (t["runner.tool_args"] ?? {}) as Record<string, string>;
  const spawns = perms?.spawns_mcp_servers === true;
  let tool: string | null = null;
  if (kind === "mcp") {
    tool = str(runner, "tool") ?? fail("an MCP analyzer must name the tool holdmap calls (runner.tool)");
    if (!Object.values(toolArgs).includes("{subject.dir}")) fail("runner.tool_args must pass the skill folder as \"{subject.dir}\"");
    if (!spawns) fail("an MCP analyzer must declare permissions.spawns_mcp_servers = true");
  } else if (spawns) {
    fail("only MCP analyzers may start MCP servers");
  }
  const network = list(perms, "network");
  if (network.length && !str(perms, "network_reason")) fail("network access needs permissions.network_reason");
  return {
    id, name: field("name"), publisher: field("publisher"), homepage, license: field("license"), summary: field("summary"),
    kind, command, args: list(runner, "args"), tool, toolArgs, network, networkReason: str(perms, "network_reason"), sends: list(perms, "sends"),
  };
}
