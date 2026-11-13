// Sample agents for the browser demo, tests and `?demo=1`. Kept apart from the port sample so
// the demo loads it only when the Agents data is first asked for.
import type { AccessFact, Agent, AgentLink, AgentMemoryReport, AgentPort, AgentsReport, CapabilityPlace, ConfiguredAgent, MemoryRole, ObservedCount, ProcessMemoryRow, Snapshot } from "./types";
import { MOCK_SNAPSHOT } from "./mock";

/** Browser-preview agents: Cursor running shop-web and docs in its terminal, and Claude Code
 *  (started from that terminal) working on shop-api. Neutral sample projects and documentation
 *  IP ranges; ports the demo has stopped drop out, like the topology. */
/** Where the sample locations come from; the demo passes the lazily loaded `mock-places`. */
export interface PlaceSource {
  cursorPlaces(project: string): CapabilityPlace[];
  claudePlaces(project: string): CapabilityPlace[];
}

export function mockAgents(s: Snapshot = MOCK_SNAPSHOT, includeCapabilities = false, places: PlaceSource | null = null): AgentsReport {
  const byPort = (p: number) => s.entries.find((e) => e.port === p);
  const port = (p: number, role: AgentPort["role"]): AgentPort[] => {
    const e = byPort(p);
    return e ? [{ entry_id: e.id, port: p, protocol: e.protocol, exposure: e.exposure, pid: e.pid, process: e.process?.name ?? null, label: e.label, role, project: e.project?.name ?? null, framework: e.framework?.name ?? null }] : [];
  };
  const local = (p: number, connections: number, service: string | null = null): AgentLink[] => {
    const e = byPort(p);
    return e ? [{ id: `local:${e.id}`, kind: "local", label: e.label, address: `127.0.0.1:${p}`, port: p, connections, entry_id: e.id, process: e.process?.name ?? null, pid: e.pid, service }] : [];
  };
  const remote = (address: string, connections: number): AgentLink => ({ id: `remote:${address}`, kind: "remote", label: address, address, port: 443, connections, entry_id: null, process: null, pid: null, service: "HTTPS" });
  const fact = (topic: AccessFact["topic"], level: AccessFact["level"], summary: string, evidence: AccessFact["evidence"]): AccessFact => ({ topic, level, summary, evidence });
  const unknownPrivacy = fact("privacy", "unknown", "macOS privacy grants (Full Disk Access, Files and Folders) can't be read without Full Disk Access; holdmap doesn't ask for it.", "unknown");
  const network = (ports: AgentPort[], remotes: number) => {
    const list = ports.map((p) => `:${p.port}`).join(", ");
    const tail = remotes ? ` Talks to ${remotes} remote host${remotes === 1 ? "" : "s"}.` : "";
    return fact("network", "standard", `${ports.length ? `Listens on ${list} on this machine only.` : "No listening sockets."}${tail}`, "observed");
  };
  const shopWeb = byPort(3000)?.project ?? { name: "shop-web", root: "/Users/dev/code/shop-web", kind: "package.json", git_branch: "feat/checkout" };
  const shopApi = byPort(3001)?.project ?? { name: "shop-api", root: "/Users/dev/code/shop-api", kind: "package.json", git_branch: "main" };
  const docs = byPort(5173)?.project ?? { name: "docs", root: "/Users/dev/code/docs", kind: "package.json", git_branch: "main" };
  const started = Math.floor(Date.now() / 1000);

  const cursorPorts = [...port(3000, "dev_server"), ...port(5173, "dev_server")];
  const cursorLinks = [...local(3001, 2), remote("198.51.100.24:443", 4), remote("203.0.113.40:443", 1), remote("[2001:db8:4700:4408::ac40:9bd1]:443", 2), remote("[2001:db8:6bc0::10]:443", 1), remote("192.0.2.183:443", 1), remote("[2001:db8:4700:681a::7e21]:443", 1)];
  const cursor: Agent = {
    id: "agent:52000", product: "cursor", name: "Cursor", vendor: "Anysphere", kind: "ide", pid: 52000, process_name: "Cursor",
    command: "/Applications/Cursor.app/Contents/MacOS/Cursor", started_at: started - 86400, parent: null, memory_bytes: 2.4e9, cpu_percent: 6.2,
    processes: [
      { pid: 52000, ppid: 1, name: "Cursor", command: "/Applications/Cursor.app/Contents/MacOS/Cursor", role: "agent", cwd: "/", memory_bytes: 410e6, cpu_percent: 1.8 },
      { pid: 52011, ppid: 52000, name: "Cursor Helper (Plugin)", command: "Cursor Helper (Plugin) --type=utility", role: "helper", cwd: "/", memory_bytes: 1.4e9, cpu_percent: 2.1 },
      { pid: 52040, ppid: 52011, name: "zsh", command: "-zsh", role: "child", cwd: shopWeb.root, memory_bytes: 6e6, cpu_percent: 0 },
      { pid: 43000, ppid: 52040, name: "node", command: "node node_modules/.bin/next dev", role: "child", cwd: shopWeb.root, memory_bytes: 412e6, cpu_percent: 1.6 },
      { pid: 52060, ppid: 52011, name: "zsh", command: "-zsh", role: "child", cwd: docs.root, memory_bytes: 6e6, cpu_percent: 0 },
      { pid: 45173, ppid: 52060, name: "node", command: "node node_modules/.bin/vite dev", role: "child", cwd: docs.root, memory_bytes: 180e6, cpu_percent: 0.7 },
      { pid: 52070, ppid: 52011, name: "node", command: "node @modelcontextprotocol/server-filesystem", role: "child", cwd: shopWeb.root, memory_bytes: 32e6, cpu_percent: 0.1 },
    ],
    process_ids: [52000, 52011, 52040, 43000, 52060, 45173, 52070, ...Array.from({ length: 8 }, (_, i) => 52080 + i)],
    more_processes: 8,
    folders: [
      { path: shopWeb.root, label: shopWeb.name, project: shopWeb, source: "child", evidence: "observed", pids: [52040, 43000], privacy_area: null, note: null },
      { path: docs.root, label: docs.name, project: docs, source: "child", evidence: "observed", pids: [52060, 45173], privacy_area: null, note: null },
      { path: "/Users/dev/code/design-system", label: "design-system", project: null, source: "recent", evidence: "observed", pids: [], privacy_area: null, note: "Editor window history; account-wide history, not attributed to this running instance." },
    ],
    more_folders: 0,
    tools: [
      { pid: 52070, ppid: 52011, name: "Filesystem MCP", kind: "mcp_server", command: "node @modelcontextprotocol/server-filesystem", cwd: shopWeb.root, evidence: "inferred", ports: [], memory_bytes: 32e6, cpu_percent: 0.1 },
      { pid: 43000, ppid: 52040, name: "Next.js", kind: "dev_server", command: "node node_modules/.bin/next dev", cwd: shopWeb.root, evidence: "observed", ports: [3000], memory_bytes: 412e6, cpu_percent: 1.6 },
      { pid: 45173, ppid: 52060, name: "Vite", kind: "dev_server", command: "node node_modules/.bin/vite dev", cwd: docs.root, evidence: "observed", ports: [5173], memory_bytes: 180e6, cpu_percent: 0.7 },
      { pid: 52040, ppid: 52011, name: "zsh", kind: "shell", command: "-zsh", cwd: shopWeb.root, evidence: "observed", ports: [], memory_bytes: 6e6, cpu_percent: 0 },
      { pid: 52060, ppid: 52011, name: "zsh", kind: "shell", command: "-zsh", cwd: docs.root, evidence: "observed", ports: [], memory_bytes: 6e6, cpu_percent: 0 },
    ],
    more_tools: 0,
    ports: cursorPorts,
    links: cursorLinks,
    more_links: 0,
    access: {
      user: "dev", uid: 501, root: false, mine: true,
      facts: [
        fact("user", "standard", "Runs as you (dev): the same file access as your account.", "observed"),
        fact("sandbox", "unknown", "No sandbox seen right now. Its commands may still be sandboxed when they run; agent settings aren't read.", "unknown"),
        fact("approvals", "unknown", "No approval flags on its command line; its own settings decide (not read).", "unknown"),
        network(cursorPorts, 2),
        unknownPrivacy,
      ],
    },
  };

  const claudePorts = port(3001, "dev_server");
  const claudeLinks = [...local(5432, 3, "PostgreSQL"), remote("203.0.113.10:443", 3)];
  const claude: Agent = {
    id: "agent:51200", product: "claude-code", name: "Claude Code", vendor: "Anthropic", kind: "cli", pid: 51200, process_name: "claude",
    command: "claude --permission-mode acceptEdits", started_at: started - 5400, parent: "agent:52000", memory_bytes: 520e6, cpu_percent: 3.4,
    processes: [
      { pid: 51200, ppid: 52040, name: "claude", command: "claude --permission-mode acceptEdits", role: "agent", cwd: shopApi.root, memory_bytes: 240e6, cpu_percent: 2.2 },
      { pid: 51230, ppid: 51200, name: "sandbox-exec", command: "sandbox-exec -p … npm test", role: "child", cwd: shopApi.root, memory_bytes: 3e6, cpu_percent: 0 },
      { pid: 43001, ppid: 51200, name: "node", command: "node server/index.js", role: "child", cwd: shopApi.root, memory_bytes: 96e6, cpu_percent: 0.9 },
    ],
    process_ids: [51200, 51230, 43001],
    more_processes: 0,
    folders: [
      { path: shopApi.root, label: shopApi.name, project: shopApi, source: "agent", evidence: "observed", pids: [51200, 51230, 43001], privacy_area: null, note: null },
      { path: shopWeb.root, label: shopWeb.name, project: null, source: "recent", evidence: "inferred", pids: [], privacy_area: null, note: "Project directory inferred from account-wide history; not attributed to this running instance." },
    ],
    more_folders: 0,
    tools: [
      { pid: 51230, ppid: 51200, name: "sandbox-exec", kind: "command", command: "sandbox-exec -p … npm test", cwd: shopApi.root, evidence: "observed", ports: [], memory_bytes: 3e6, cpu_percent: 0 },
      { pid: 43001, ppid: 51200, name: "Node.js", kind: "dev_server", command: "node server/index.js", cwd: shopApi.root, evidence: "observed", ports: [3001], memory_bytes: 96e6, cpu_percent: 0.9 },
    ],
    more_tools: 0,
    ports: claudePorts,
    links: claudeLinks,
    more_links: 0,
    access: {
      user: "dev", uid: 501, root: false, mine: true,
      facts: [
        fact("user", "standard", "Runs as you (dev): the same file access as your account.", "observed"),
        fact("sandbox", "restricted", "A current child command runs inside sandbox-exec (macOS Seatbelt).", "observed"),
        fact("approvals", "standard", "Started with --permission-mode acceptEdits: file edits are accepted without asking; commands still ask.", "observed"),
        network(claudePorts, 1),
        unknownPrivacy,
      ],
    },
  };

  const dockerPorts = [...port(5432, "service"), ...port(6379, "service")];
  const docker: Agent = {
    id: "agent:48000", product: "docker-desktop", name: "Docker Desktop", vendor: "Docker", kind: "tool", pid: 48000, process_name: "Docker Desktop",
    command: "/Applications/Docker.app/Contents/MacOS/Docker Desktop", started_at: started - 172800, parent: null, memory_bytes: 1.1e9, cpu_percent: 2.0,
    processes: [
      { pid: 48000, ppid: 1, name: "Docker Desktop", command: "/Applications/Docker.app/Contents/MacOS/Docker Desktop", role: "agent", cwd: "/", memory_bytes: 180e6, cpu_percent: 0.4 },
      { pid: 48020, ppid: 48000, name: "com.docker.backend", command: "com.docker.backend", role: "helper", cwd: "/", memory_bytes: 420e6, cpu_percent: 1.1 },
    ],
    process_ids: [48000, 48020, 48030, 48031, 48032, 48033],
    more_processes: 4,
    folders: [],
    more_folders: 0,
    tools: [],
    more_tools: 0,
    ports: dockerPorts,
    links: [],
    more_links: 0,
    access: {
      user: "dev", uid: 501, root: false, mine: true,
      facts: [
        fact("user", "standard", "Runs as you (dev): the same file access as your account.", "observed"),
        fact("sandbox", "unknown", "No sandbox seen right now. Its commands may still be sandboxed when they run; agent settings aren't read.", "unknown"),
        fact("approvals", "unknown", "No approval flags on its command line; its own settings decide (not read).", "unknown"),
        network(dockerPorts, 0),
        unknownPrivacy,
      ],
    },
  };

  // A stopped dev server's process is gone from its agent too.
  if (includeCapabilities) {
    cursor.capabilities = {
      inspected: true,
      mcp_servers: [
        { id: "cursor:user:github", name: "github", scope: "user", source_path: "/Users/dev/.cursor/mcp.json", project_path: null, transport: "http", enabled: true, evidence: "observed", remote_host: "api.githubcopilot.com", header_names: ["Authorization"], env_names: [], runtime: { state: "unknown", pids: [] } },
        { id: "cursor:project:filesystem", name: "filesystem", scope: "project", source_path: `${shopWeb.root}/.cursor/mcp.json`, project_path: shopWeb.root, transport: "stdio", enabled: true, evidence: "observed", launch: { runner: "npx", package: "@modelcontextprotocol/server-filesystem", pinned: false }, env_names: [], header_names: [], runtime: { state: "running", pids: [52070] } },
        { id: "cursor:project:legacy-browser", name: "legacy-browser", scope: "project", source_path: `${shopWeb.root}/.cursor/mcp.json`, project_path: shopWeb.root, transport: "stdio", enabled: false, evidence: "observed", launch: { runner: "npx", package: "puppeteer-mcp-server@0.7.2", pinned: true }, env_names: [], header_names: [], runtime: { state: "unknown", pids: [] } },
      ],
      skills: [
        { id: "shared:user:code-review", name: "code-review", declared_name: "code-review", description: "Review changes for correctness, regressions and maintainability.", scope: "user", source_path: "/Users/dev/.agents/skills/code-review/SKILL.md", project_path: null, evidence: "observed" },
        { id: "shared:project:frontend-checks", name: "frontend-checks", declared_name: "frontend-checks", description: "Check UI behavior, accessibility and responsive layouts before shipping.", scope: "project", source_path: `${shopWeb.root}/.agents/skills/frontend-checks/SKILL.md`, project_path: shopWeb.root, evidence: "observed" },
      ],
      instructions: [
        { id: "cursor:rule:react", kind: "rule", name: "react-conventions", description: "TypeScript and React conventions for the storefront.", scope: "project", source_path: `${shopWeb.root}/.cursor/rules/react-conventions.mdc`, evidence: "observed" },
        { id: "cursor:memory:agents", kind: "memory", name: "AGENTS.md", description: null, scope: "project", source_path: `${shopWeb.root}/AGENTS.md`, evidence: "observed" },
      ],
      tool_rules: [], settings: [],       builtin_tools: [],
      locations: places?.cursorPlaces(shopWeb.root) ?? [],
      more_mcp_servers: 0, more_skills: 0,
      limits: ["Configuration and skill-file presence do not establish loading, connection or invocation. Arguments, URL paths and queries, and environment or header values are not included. Only declared name/description from bounded skill frontmatter are retained; instructions are not collected or executed."],
    };
    claude.capabilities = {
      inspected: true,
      mcp_servers: [
        { id: "claude:project:filesystem", name: "filesystem", scope: "project", source_path: `${shopApi.root}/.mcp.json`, project_path: shopApi.root, transport: "stdio", enabled: null, evidence: "observed", launch: { runner: "npx", package: "@modelcontextprotocol/server-filesystem@2025.8.21", pinned: true }, env_names: [], header_names: [], runtime: { state: "not_seen", pids: [] } },
        { id: "claude:user:github", name: "github", scope: "user", source_path: "/Users/dev/.claude.json", project_path: null, transport: "stdio", enabled: null, evidence: "observed", launch: { runner: "npx", package: "@modelcontextprotocol/server-github", pinned: false }, env_names: ["GITHUB_PERSONAL_ACCESS_TOKEN"], header_names: [], runtime: { state: "not_seen", pids: [] } },
        { id: "claude:user:linear", name: "linear", scope: "user", source_path: "/Users/dev/.claude.json", project_path: null, transport: "http", enabled: null, evidence: "observed", remote_host: "mcp.linear.app", env_names: [], header_names: ["Authorization"], runtime: { state: "unknown", pids: [] } },
        { id: "claude:local:postgres", name: "postgres", scope: "local", source_path: "/Users/dev/.claude.json", project_path: shopApi.root, transport: "stdio", enabled: null, evidence: "observed", launch: { runner: "docker", package: "mcp/postgres:1.2", pinned: true }, env_names: ["DATABASE_URL"], header_names: [], runtime: { state: "not_seen", pids: [] } },
      ],
      skills: [{ id: "claude:project:release-check", name: "release-check", description: "Check release notes, version references and packaging evidence.", scope: "project", source_path: `${shopApi.root}/.claude/skills/release-check/SKILL.md`, project_path: shopApi.root, evidence: "observed" }],
      instructions: [
        { id: "claude:command:ship", kind: "command", name: "ship", description: "Run checks and open a pull request.", scope: "user", source_path: "/Users/dev/.claude/commands/ship.md", evidence: "observed" },
        { id: "claude:subagent:reviewer", kind: "subagent", name: "reviewer", description: "Reviews diffs for bugs and missing tests.", scope: "user", source_path: "/Users/dev/.claude/agents/reviewer.md", evidence: "observed" },
        { id: "claude:memory:claude", kind: "memory", name: "CLAUDE.md", description: null, scope: "project", source_path: `${shopApi.root}/CLAUDE.md`, evidence: "observed" },
      ],
      tool_rules: [
        { rule: "Bash(npm run test:*)", decision: "allow", scope: "user", source_path: "/Users/dev/.claude/settings.json" },
        { rule: "Bash(git status)", decision: "allow", scope: "user", source_path: "/Users/dev/.claude/settings.json" },
        { rule: "WebFetch", decision: "ask", scope: "project", source_path: `${shopApi.root}/.claude/settings.json` },
        { rule: "Bash(curl:*)", decision: "deny", scope: "user", source_path: "/Users/dev/.claude/settings.json" },
        { rule: "Read(./.env)", decision: "deny", scope: "project", source_path: `${shopApi.root}/.claude/settings.json` },
      ],
      settings: [{ key: "permissions.defaultMode", value: "acceptEdits", scope: "user", source_path: "/Users/dev/.claude/settings.json" }],
      builtin_tools: ["Bash", "Edit", "Glob", "Grep", "MultiEdit", "NotebookEdit", "Read", "Task", "TodoWrite", "WebFetch", "WebSearch", "Write"],
      locations: places?.claudePlaces(shopApi.root) ?? [],
      more_mcp_servers: 0, more_skills: 0, limits: [],
    };
    docker.capabilities = { inspected: false, mcp_servers: [], skills: [], more_mcp_servers: 0, more_skills: 0, limits: ["Capability source discovery is not supported for Docker Desktop."] };
  }
  const gone = new Set([3000, 3001, 5173].filter((p) => !byPort(p)).map((p) => 40000 + p));
  const live = (a: Agent): Agent => ({
    ...a,
    processes: a.processes.filter((p) => !gone.has(p.pid)),
    process_ids: a.process_ids.filter((pid) => !gone.has(pid)),
    tools: a.tools.filter((tool) => !gone.has(tool.pid)),
    folders: a.folders.map((f) => ({ ...f, pids: f.pids.filter((p) => !gone.has(p)) })),
  });
  // Codex isn't running; its default configuration declares two skills (opt-in inventory only).
  const configured: ConfiguredAgent[] = includeCapabilities ? [{
    product: "codex", name: "Codex CLI",
    capabilities: {
      inspected: true, mcp_servers: [],
      skills: [
        { id: "shared:user:code-review", name: "code-review", declared_name: "code-review", description: "Review changes for correctness, regressions and maintainability.", scope: "user", source_path: "/Users/dev/.agents/skills/code-review/SKILL.md", project_path: null, evidence: "observed" },
        { id: "codex:user:db-migrations", name: "db-migrations", declared_name: "db-migrations", description: "Write and check database migrations.", scope: "user", source_path: "/Users/dev/.codex/skills/db-migrations/SKILL.md", project_path: null, evidence: "observed" },
      ],
      more_mcp_servers: 0, more_skills: 0, limits: [],
    },
  }] : [];
  const agents = [claude, cursor, docker].map(live);
  return {
    agents,
    memory: agents.map((agent) => demoMemory(agent)),
    ...(configured.length ? { configured } : {}),
    platform: "macos",
    taken_at_ms: s.taken_at_ms,
    limits: [
      "Folders are working directories and the agents' recent-project lists; open files aren't collected.",
      "Remote hosts are shown by IP address; holdmap doesn't look names up.",
      "MCP server hints come from running executable or package names; transports aren't observed and individual tool calls aren't collected.",
      "Recent projects come from account-wide history and aren't attributed to a particular running instance.",
      includeCapabilities ? "Opt-in inventory includes supported configuration names and skill-file locations; chats and credential stores are not inspected." : "Passive scans do not inspect chats, agent settings or credential stores. Recognized credentials in process arguments are redacted heuristically.",
      "macOS privacy grants (TCC) can't be read without Full Disk Access, and folders it guards aren't inspected.",
    ],
  };
}

const DEMO_MACHINE = 16 * 1024 * 1024 * 1024;
const known = (value: number): ObservedCount => ({ value, unknown: null });
const missing = (unknown: string): ObservedCount => ({ value: null, unknown });
const unavailable = () =>
  "proc_pid_rusage (RUSAGE_INFO_V4) does not return this counter. task_for_pid is not used, because it needs a privilege holdmap does not request.";
const PEAKS = "Each process has its own peak. Those peaks are not simultaneous, so holdmap does not add them into a tree peak.";

function demoReading(resident: number, footprint: number, peak: number): Record<string, ObservedCount> {
  return {
    resident: known(resident),
    physical_footprint: known(footprint),
    private_bytes: missing(unavailable()),
    shared_bytes: missing(unavailable()),
    proportional: missing("Proportional set size is a Linux smaps counter. macOS does not report it."),
    compressed: missing(unavailable()),
    virtual_bytes: missing(unavailable()),
    swap_bytes: missing("macOS proc_pid_rusage does not report per-process swap. Compressed memory, which Activity Monitor shows separately, is also not in this call."),
    peak_resident: missing("proc_pid_rusage reports a lifetime peak physical footprint, not a peak resident size."),
    peak_physical: known(peak),
    commit_bytes: missing("Commit charge is a Windows counter. macOS does not report it."),
  };
}

function demoRow(pid: number, name: string, role: MemoryRole, scan: number, resident: number, footprint: number, dominant = false): ProcessMemoryRow {
  return {
    pid, name, role, evidence: role === "mcp_server" || role === "language_server" ? "inferred" : "observed",
    scan_bytes: scan, reading: demoReading(resident, footprint, Math.round(footprint * 1.15)),
    dominant,
    dominant_reason: dominant ? "This process holds at least half of the headline and at least 256 MiB, and it is not the agent process." : null,
  };
}

function demoMemory(agent: Agent): AgentMemoryReport {
  const rows: ProcessMemoryRow[] = agent.product === "cursor"
    ? [
        demoRow(52011, "Cursor Helper (Plugin)", "helper", 1.4e9, 1.1e9, 1.4e9, true),
        demoRow(52000, "Cursor", "agent", 410e6, 380e6, 410e6),
        demoRow(43000, "node", "child", 412e6, 300e6, 412e6),
        demoRow(52080, "rust-analyzer", "language_server", 220e6, 180e6, 220e6),
        demoRow(45173, "node", "child", 180e6, 140e6, 180e6),
        demoRow(52070, "node", "mcp_server", 32e6, 28e6, 32e6),
      ]
    : agent.product === "claude-code"
      ? [
          demoRow(51200, "claude", "agent", 240e6, 210e6, 240e6),
          demoRow(43001, "node", "child", 96e6, 80e6, 96e6),
          demoRow(51230, "sandbox-exec", "helper", 3e6, 2e6, 3e6),
        ]
      : [
          demoRow(48020, "com.docker.backend", "helper", 420e6, 360e6, 420e6),
          demoRow(48000, "Docker Desktop", "agent", 180e6, 150e6, 180e6),
        ];
  const kept = rows.filter((row) => agent.process_ids.includes(row.pid));
  const roles: MemoryRole[] = ["agent", "mcp_server", "language_server", "helper", "child"];
  const groups = roles.flatMap((role) => {
    const processes = kept.filter((row) => row.role === role);
    return processes.length ? [{ role, processes: processes.length, scan_bytes: processes.reduce((sum, row) => sum + row.scan_bytes, 0) }] : [];
  });
  const direction = agent.product === "cursor" ? "rising" as const : agent.product === "claude-code" ? "falling" as const : "flat" as const;
  return {
    agent_id: agent.id, pid: agent.pid, started_at: agent.started_at,
    headline_bytes: agent.memory_bytes, headline_counter: "physical_footprint",
    headline_meaning: "Sum of macOS physical-footprint samples taken with the process table. Activity Monitor's Memory column is based on the same counter, but it samples at another moment and groups processes its own way, so the numbers can differ.",
    headline_label: agent.product === "docker-desktop" ? "Mixed: physical footprint and resident (rss)" : "Physical footprint",
    headline_mixed: agent.product === "docker-desktop" ? "1 of 2 processes used resident size instead of physical footprint: the operating system refused that counter (another user's process, or one that exited during the scan). The total mixes both counters." : null,
    machine_bytes: known(DEMO_MACHINE),
    share_ppm: known(Math.round(agent.memory_bytes / DEMO_MACHINE * 1_000_000)),
    resident: known(kept.reduce((sum, row) => sum + (row.reading.resident.value ?? 0), 0)),
    physical_footprint: known(agent.memory_bytes),
    private_bytes: missing(unavailable()),
    shared_bytes: missing(unavailable()),
    proportional: missing("Proportional set size is a Linux smaps counter. macOS does not report it."),
    compressed: missing(unavailable()),
    virtual_bytes: missing(unavailable()),
    swap_bytes: missing("macOS proc_pid_rusage does not report per-process swap. Compressed memory, which Activity Monitor shows separately, is also not in this call."),
    peak_resident: missing(PEAKS),
    peak_physical: missing(PEAKS),
    commit_bytes: missing("Commit charge is a Windows counter. macOS does not report it."),
    groups, processes: kept,
    trend: {
      samples: direction === "rising"
        ? [
            { at_ms: Date.now() - 18 * 60_000, bytes: Math.round(agent.memory_bytes * 0.80) },
            { at_ms: Date.now() - 12 * 60_000, bytes: Math.round(agent.memory_bytes * 0.87) },
            { at_ms: Date.now() - 6 * 60_000, bytes: Math.round(agent.memory_bytes * 0.94) },
            { at_ms: Date.now(), bytes: agent.memory_bytes },
          ]
        : direction === "falling"
          ? [
              { at_ms: Date.now() - 12 * 60_000, bytes: Math.round(agent.memory_bytes * 1.2) },
              { at_ms: Date.now() - 6 * 60_000, bytes: Math.round(agent.memory_bytes * 1.09) },
              { at_ms: Date.now(), bytes: agent.memory_bytes },
            ]
          : [
              { at_ms: Date.now() - 10 * 60_000, bytes: agent.memory_bytes },
              { at_ms: Date.now(), bytes: agent.memory_bytes },
            ],
      direction, rule: "Newest versus oldest retained headline sample, using a 10% and 1 MiB band.",
    },
    limits: ["Tree peaks are not added. Each process keeps its own peak.", "Private, shared and compressed memory need task_for_pid, which holdmap does not request."],
  };
}
