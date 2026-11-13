// Sample documented locations for the demo's Cursor and Claude Code. They load only when the
// demo reads configuration, so the first screen does not carry them.
import type { CapabilityPlace } from "./types";

const COMBINE: Record<string, CapabilityPlace["combine"]> = {
  settings: "override", mcp_config: "merge", skills: "merge", commands: "merge", subagents: "merge", extensions: "merge",
  builtin: "merge", plugin: "merge", instructions: "join", rules: "join", hooks: "join", agent_memory: "join",
  session_data: "not_config", secret_store: "not_config",
};
const RANK: Record<string, number> = { managed: 0, local: 10, project: 20, user: 30, system_default: 35, plugin: 40, account: 45, builtin: 50 };

type PlaceInput = Pick<CapabilityPlace, "id" | "kind" | "scope" | "path" | "status"> & Partial<CapabilityPlace>;

/** A documented location with the census defaults; `present` follows the status. */
function place(input: PlaceInput): CapabilityPlace {
  return {
    symlink_target: null, status_reason: null, file_count: null, byte_size: null, candidates: [], entries: [], label: "", note: null,
    ignored_count: null, line_count: null, load_limit_lines: null, modified_unix_ms: Date.now() - 86_400_000,
    rank: RANK[input.scope] ?? 30, combine: COMBINE[input.kind], present: input.status === "present",
    ...input,
  };
}

export function cursorPlaces(project: string): CapabilityPlace[] {
  const edited = Date.now() - 3_600_000;
  return [
    place({ id: "cursor:project:skills", kind: "skills", scope: "project", label: "Skills", path: `${project}/.agents/skills`, status: "present", file_count: 1, byte_size: 2048, modified_unix_ms: edited, candidates: ["frontend-checks"], entries: [{ name: "frontend-checks", kind: "directory", bytes: null, modified_unix_ms: edited }] }),
    place({ id: "cursor:project:rules", kind: "rules", scope: "project", label: "Project rules", path: `${project}/.cursor/rules`, status: "present", file_count: 1, byte_size: 860, modified_unix_ms: edited, candidates: ["react-conventions"], ignored_count: 1, status_reason: "1 .md file is ignored here; only .mdc rule files load.", note: "Cursor loads .mdc rule files; plain .md files in this folder are ignored." }),
    place({ id: "cursor:project:context", kind: "instructions", scope: "project", label: "AGENTS.md", path: `${project}/AGENTS.md`, status: "present", file_count: 1, byte_size: 640, modified_unix_ms: edited, candidates: ["AGENTS.md"] }),
    place({ id: "cursor:project:mcp", kind: "mcp_config", scope: "project", label: "mcp.json", path: `${project}/.cursor/mcp.json`, status: "present", file_count: 1, byte_size: 420, modified_unix_ms: edited, candidates: ["mcp.json"] }),
    place({ id: "cursor:user:skills", kind: "skills", scope: "user", label: "Skills", path: "/Users/dev/.cursor/skills", status: "present", symlink_target: "/Users/dev/.agents/skills", status_reason: "The path is a symbolic link. holdmap does not follow it, so size and children are unknown." }),
    place({ id: "cursor:user:mcp", kind: "mcp_config", scope: "user", label: "mcp.json", path: "/Users/dev/.cursor/mcp.json", status: "present", file_count: 1, byte_size: 300, modified_unix_ms: edited, candidates: ["mcp.json"] }),
    place({ id: "cursor:user:commands", kind: "commands", scope: "user", label: "Commands", path: "/Users/dev/.cursor/commands", status: "unreadable", status_reason: "Could not inspect the path (access denied)" }),
    place({ id: "cursor:managed:hooks", kind: "hooks", scope: "managed", label: "Enterprise hooks.json", path: "/Library/Application Support/Cursor/hooks.json", status: "missing" }),
    place({ id: "cursor:account:rules", kind: "rules", scope: "account", label: "User and Team rules", path: "", status: "skipped", modified_unix_ms: null, status_reason: "User rules live in Cursor Settings and Team rules in the dashboard; neither is a file holdmap can see." }),
    place({ id: "cursor:plugin", kind: "plugin", scope: "plugin", label: "Marketplace and cloud installs", path: "", status: "skipped", modified_unix_ms: null, status_reason: "Marketplace and cloud skill installs are not a single documented folder this census stats. Account databases are not opened." }),
    place({ id: "cursor:builtin", kind: "builtin", scope: "builtin", label: "Built-in tools", path: "", status: "present", modified_unix_ms: null, candidates: ["Read", "Edit", "Grep"] }),
  ];
}

export function claudePlaces(project: string): CapabilityPlace[] {
  const edited = Date.now() - 7_200_000;
  const encoded = project.replace(/[^A-Za-z0-9]/g, "-");
  return [
    place({ id: "claude:managed:settings", kind: "settings", scope: "managed", label: "managed-settings.json", path: "/Library/Application Support/ClaudeCode/managed-settings.json", status: "missing", note: "Organization policy. Nothing in user, project or local settings overrides it." }),
    place({ id: "claude:local:claude-md", kind: "instructions", scope: "local", label: "CLAUDE.local.md", path: `${project}/CLAUDE.local.md`, status: "missing" }),
    place({ id: "claude:local:settings", kind: "settings", scope: "local", label: "settings.local.json", path: `${project}/.claude/settings.local.json`, status: "present", file_count: 1, byte_size: 120, modified_unix_ms: edited, candidates: ["settings.local.json"] }),
    place({ id: "claude:project:skills", kind: "skills", scope: "project", label: "Skills", path: `${project}/.claude/skills`, status: "present", file_count: 1, byte_size: 1500, modified_unix_ms: edited, candidates: ["release-check"], entries: [{ name: "release-check", kind: "directory", bytes: null, modified_unix_ms: edited }] }),
    place({ id: "claude:project:context", kind: "instructions", scope: "project", label: "CLAUDE.md", path: `${project}/CLAUDE.md`, status: "present", file_count: 1, byte_size: 900, modified_unix_ms: edited, candidates: ["CLAUDE.md"] }),
    place({ id: "claude:project:rules", kind: "rules", scope: "project", label: "Rules", path: `${project}/.claude/rules`, status: "present", file_count: 2, byte_size: 1300, modified_unix_ms: edited, candidates: ["api/errors", "testing"] }),
    place({ id: "claude:project:mcp", kind: "mcp_config", scope: "project", label: ".mcp.json", path: `${project}/.mcp.json`, status: "present", file_count: 1, byte_size: 280, modified_unix_ms: edited, candidates: [".mcp.json"] }),
    place({ id: "claude:user:commands", kind: "commands", scope: "user", label: "Commands", path: "/Users/dev/.claude/commands", status: "present", file_count: 1, byte_size: 400, modified_unix_ms: edited, candidates: ["ship"], entries: [{ name: "ship.md", kind: "file", bytes: 400, modified_unix_ms: edited }] }),
    place({ id: "claude:user:mcp", kind: "mcp_config", scope: "user", label: ".claude.json", path: "/Users/dev/.claude.json", status: "present", file_count: 1, byte_size: 1100, modified_unix_ms: edited, candidates: [".claude.json"] }),
    place({ id: "claude:user:auto-memory", kind: "agent_memory", scope: "user", label: "Auto memory", path: `/Users/dev/.claude/projects/${encoded}/memory`, status: "present", file_count: 3, byte_size: 14_200, modified_unix_ms: edited, line_count: 236, load_limit_lines: 200, status_reason: "MEMORY.md has 236 lines; only the first 200 load each session.", note: "Notes Claude writes for itself. The first 200 lines or 25 KB of MEMORY.md load every session." }),
    place({ id: "claude:user:transcripts", kind: "session_data", scope: "user", label: "Transcripts", path: "/Users/dev/.claude/projects", status: "present", file_count: 412, byte_size: 186_000_000, modified_unix_ms: edited, note: "Plaintext session transcripts. holdmap shows only total size, file count and newest time." }),
    place({ id: "claude:user:credentials", kind: "secret_store", scope: "user", label: "Login credentials", path: "/Users/dev/.claude/.credentials.json", status: "present", modified_unix_ms: null, status_reason: "Secrets store. Present; never opened, listed or sized." }),
    place({ id: "claude:builtin", kind: "builtin", scope: "builtin", label: "Built-in tools", path: "", status: "present", modified_unix_ms: null, candidates: ["Bash", "Read", "Edit", "WebSearch"] }),
  ];
}
