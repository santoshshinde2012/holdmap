// 3D machine view model: each agent's processes, ports, folders, connections and opt-in
// configuration as parts on three layers (board, runtime, configuration), and opt-in recorded
// calls in a lane of their own. Pure and bounded. The same honesty rules as the 2D map and the
// capability map:
// - wires come from process trees, held listeners, sockets, working folders, or configuration
//   files found for that agent; never from matching names;
// - the only configuration ↔ process wire is an MCP server's "running" state, whose PIDs are
//   this agent's own processes;
// - recorded calls stay in their lane: a reported alias or PID never joins a configured server
//   or a process, so no wire touches a call (see calls.ts).
import { bytesOrUnknown, memoryFor, shareText } from "../agent-memory";
import { isUnpinned, launchLine, runState } from "../agent-config";
import { ACCESS_LABEL, accessHeadline, KIND_LABEL } from "../agents";
import { humanBytes, tildify } from "../format";
import type { SkillTrustView, TrustState } from "../trust";
import type { AgentCapabilities, AgentMcpServer, AgentsReport, AgentSkill, AgentTool, Evidence, PortEntry } from "../types";
import type { ActivitySnapshot, RecordingStatus } from "../agent-flow/model";
import { callParts } from "./calls";

function memoryFacts(report: AgentsReport | null | undefined, id: string): [string, string][] {
  const memory = memoryFor(report, id);
  if (!memory) return [];
  return [
    ["Resident", bytesOrUnknown(memory.resident)],
    ["Footprint", bytesOrUnknown(memory.physical_footprint)],
    ["Share", shareText(memory.share_ppm)],
  ];
}

export type Layer = "board" | "runtime" | "config" | "calls";
export type PartKind = "agent" | "dormant" | "project" | "port" | "remote" | "tool" | "mcp" | "skill" | "instructions" | "more" | "call";
/** How sure a part is: from the OS (observed/inferred/unknown), a file on disk (configured), or an
 *  agent integration's own report (reported). */
export type PartEvidence = Evidence | "configured" | "reported";
export type PartTarget = { view: "agents"; agentId: string } | { view: "list"; entryId: string } | null;

export interface Part {
  id: string;
  kind: PartKind;
  layer: Layer;
  /** The agent this part belongs to (a dormant agent owns itself). */
  owner: string;
  label: string;
  sub: string;
  evidence: PartEvidence;
  /** Alive in the last scan: a held port, a running process, an MCP server running now. */
  live: boolean;
  /** Why this part needs you, or null. */
  warn: string | null;
  /** Shape and material hint: tool kind, MCP run tone, skill trust, port exposure… */
  variant: string;
  cpu: number;
  memory: number;
  /** Hosts in a remote group, files in a rules stack, parts behind a "+N" chip, an agent's processes. */
  count: number;
  /** An agent's drawn processes' CPU, revealed as slices inside the tower in X-ray. */
  slices?: number[];
  target: PartTarget;
  /** Inspector rows, already worded. */
  facts: [string, string][];
}

export type WireKind = "process" | "listener" | "folder" | "socket" | "config" | "running";
export interface Wire { id: string; from: string; to: string; kind: WireKind; label: string; evidence: PartEvidence; /** Open connections, for sockets. */ weight?: number }

export interface MachineScene {
  /** Ring order: running agents in report order (matching their colours), then dormant ones. */
  agents: Part[];
  parts: Part[];
  wires: Wire[];
  /** Agents beyond the display bound. */
  omittedAgents: number;
  /** Whether any agent's configuration was read; otherwise the configuration layer is empty by design. */
  inspected: boolean;
  /** Recorded calls drawn in their lane, and whether recording is on now. */
  calls: number;
  recording: boolean;
  /** Layout identity: parts, owners and wires. Resource samples don't change it. */
  signature: string;
}

export const SCENE_LIMITS = { agents: 16, dormant: 4, tools: 8, ports: 8, projects: 4, mcp: 10, skills: 10 } as const;

export const LAYER_LABEL: Record<Layer, string> = { board: "This computer", runtime: "Runtime", config: "Configuration", calls: "Recorded calls" };
export const PART_KIND_LABEL: Record<PartKind, string> = {
  agent: "Agent", dormant: "Agent, not running", project: "Project", port: "Port", remote: "Remote hosts", tool: "Process",
  mcp: "MCP server", skill: "Skill", instructions: "Rules & instructions", more: "More", call: "Recorded call",
};
const TOOL_LABEL: Record<AgentTool["kind"], string> = { mcp_server: "MCP server process", dev_server: "Dev server", shell: "Shell", command: "Command" };
const SCOPE_LABEL = { user: "User", project: "Project", local: "Local" } as const;
const TRUST_WARN: Partial<Record<TrustState, string>> = { changed: "Changed since you trusted it", new: "Not trusted yet" };

export interface SceneInput {
  report: AgentsReport | null;
  entries?: readonly PortEntry[] | null;
  /** Agents whose "Needs you" items concern the agent itself (root, elevated, risky settings…). */
  hot?: ReadonlySet<string>;
  /** "Needs you" titles by the agent they belong to, as Home groups them. */
  needs?: ReadonlyMap<string, readonly string[]>;
  /** Skill trust; joined to a skill only by its exact folder path. */
  trust?: readonly SkillTrustView[];
  /** Opt-in recorded calls and the recorder's state. */
  activity?: ActivitySnapshot | null;
  recording?: RecordingStatus | null;
}

const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;
const pct = (n: number) => `${n < 10 ? n.toFixed(1) : Math.round(n)}%`;

function part(p: Omit<Part, "cpu" | "memory" | "count" | "warn" | "live" | "facts"> & Partial<Pick<Part, "cpu" | "memory" | "count" | "warn" | "live" | "facts">>): Part {
  return { cpu: 0, memory: 0, count: 0, warn: null, live: false, facts: [], ...p };
}

/** A "+N more" chip on a layer, so a bound never reads as absence. */
function more(owner: string, layer: Layer, what: string, n: number): Part {
  return part({ id: `${owner}:more:${layer}:${what}`, kind: "more", layer, owner, label: `+${n}`, sub: `more ${what}`, evidence: "observed", variant: what, count: n, target: { view: "agents", agentId: owner },
    facts: [["Not drawn", `${plural(n, what.replace(/s$/, ""), what)} beyond the 3D view's bound. Agents lists every one.`]] });
}

/** The skill folder of a SKILL.md location: trust is recorded per folder. */
const skillDir = (s: AgentSkill) => s.source_path.replace(/[\\/]SKILL\.md$/i, "");

function mcpPart(owner: string, s: AgentMcpServer): Part {
  const run = runState(s);
  const unpinned = isUnpinned(s);
  return part({
    id: `${owner}:mcp:${s.id}`, kind: "mcp", layer: "config", owner, label: s.name,
    sub: `${run.label} · ${launchLine(s)}`, evidence: "configured", live: run.tone === "ok", variant: run.tone,
    warn: unpinned ? "Fetches its code without a pinned version, so it can change on the next launch" : null,
    target: { view: "agents", agentId: owner },
    facts: [["Run state", `${run.label}. ${run.detail}`], ["Launch", launchLine(s)], ["Transport", s.transport], ["Scope", SCOPE_LABEL[s.scope]], ["Source", tildify(s.source_path)],
      ...(s.env_names?.length ? [["Environment names", s.env_names.join(", ")] as [string, string]] : [])],
  });
}

function skillPart(owner: string, s: AgentSkill, trust: ReadonlyMap<string, SkillTrustView>): Part {
  const t = trust.get(skillDir(s));
  return part({
    id: `${owner}:skill:${s.id}`, kind: "skill", layer: "config", owner, label: s.declared_name || s.name,
    sub: t ? (t.state === "trusted" ? "Trusted" : TRUST_WARN[t.state] ?? "Couldn't read") : `${SCOPE_LABEL[s.scope]} skill`,
    evidence: "configured", variant: t?.state ?? "file", warn: t ? TRUST_WARN[t.state] ?? null : null, target: { view: "agents", agentId: owner },
    facts: [["Purpose", s.description ?? "No description declared"], ["Scope", SCOPE_LABEL[s.scope]], ["File", tildify(s.source_path)],
      ["Activation", "Unknown: a skill file on disk doesn't prove it was loaded"]],
  });
}

function instructionsPart(owner: string, c: AgentCapabilities): Part | null {
  const files = c.instructions ?? [], rules = c.tool_rules ?? [];
  if (!files.length && !rules.length) return null;
  const kinds = new Map<string, number>();
  for (const f of files) kinds.set(f.kind, (kinds.get(f.kind) ?? 0) + 1);
  const decisions = (d: string) => rules.filter((r) => r.decision === d).length;
  return part({
    id: `${owner}:instructions`, kind: "instructions", layer: "config", owner, label: files.length ? plural(files.length, "instruction") : plural(rules.length, "rule"),
    sub: [...kinds].map(([k, n]) => `${n} ${k}`).join(" · ") || "permission rules", evidence: "configured", variant: "stack", count: files.length + rules.length,
    target: { view: "agents", agentId: owner },
    facts: [
      ...files.slice(0, 8).map((f): [string, string] => [f.kind[0].toUpperCase() + f.kind.slice(1), f.name]),
      ...(rules.length ? [["Permission rules", `${decisions("allow")} allow · ${decisions("ask")} ask · ${decisions("deny")} deny`] as [string, string]] : []),
    ],
  });
}

/** Configuration parts and wires for one agent context (running or dormant). */
function configParts(owner: string, c: AgentCapabilities | undefined, trust: ReadonlyMap<string, SkillTrustView>, parts: Part[], wires: Wire[]) {
  if (!c?.inspected) return;
  const servers = c.mcp_servers.slice(0, SCENE_LIMITS.mcp), skills = c.skills.slice(0, SCENE_LIMITS.skills);
  for (const s of servers) {
    const p = mcpPart(owner, s);
    parts.push(p);
    wires.push({ id: `config:${p.id}`, from: owner, to: p.id, kind: "config", label: "Configuration observed · use not established", evidence: "configured" });
  }
  const moreServers = c.mcp_servers.length - servers.length + c.more_mcp_servers;
  if (moreServers > 0) parts.push(more(owner, "config", "MCP servers", moreServers));
  for (const s of skills) {
    const p = skillPart(owner, s, trust);
    parts.push(p);
    wires.push({ id: `config:${p.id}`, from: owner, to: p.id, kind: "config", label: "Skill file observed · activation unknown", evidence: "configured" });
  }
  const moreSkills = c.skills.length - skills.length + c.more_skills;
  if (moreSkills > 0) parts.push(more(owner, "config", "skills", moreSkills));
  const rules = instructionsPart(owner, c);
  if (rules) {
    parts.push(rules);
    wires.push({ id: `config:${rules.id}`, from: owner, to: rules.id, kind: "config", label: "Files observed · use not established", evidence: "configured" });
  }
}

export function buildScene({ report, entries = null, hot = new Set(), needs = new Map(), trust = [], activity = null, recording = null }: SceneInput): MachineScene {
  const parts: Part[] = [], wires: Wire[] = [], agents: Part[] = [];
  const running = (report?.agents ?? []).slice(0, SCENE_LIMITS.agents);
  const trustByDir = new Map(trust.flatMap((t) => (t.subject.kind === "skill" && t.subject.dir ? [[t.subject.dir, t] as const] : [])));
  const entryById = new Map((entries ?? []).map((e) => [e.id, e]));
  // Ports of every drawn agent, so a socket to another agent's listener lands on that exact port.
  const portPart = new Map<string, string>();

  for (const a of running) {
    const head = accessHeadline(a);
    const mine = needs.get(a.id) ?? [];
    const agent = part({
      id: a.id, kind: "agent", layer: "board", owner: a.id, label: a.name, sub: `${KIND_LABEL[a.kind]} · ${ACCESS_LABEL[head.level]}`,
      evidence: "observed", live: true, variant: head.level, cpu: a.cpu_percent, memory: a.memory_bytes,
      count: a.process_ids.length, slices: a.processes.slice(0, 16).map((p) => p.cpu_percent),
      warn: hot.has(a.id) ? mine[0] ?? head.text : null,
      target: { view: "agents", agentId: a.id },
      facts: [["Kind", `${KIND_LABEL[a.kind]} by ${a.vendor}`], ["Access", head.text], ["CPU", pct(a.cpu_percent)], ["Memory", humanBytes(a.memory_bytes)],
        ...memoryFacts(report, a.id),
        ["Processes", String(a.process_ids.length)], ["PID", String(a.pid)], ...(mine.length ? [["Needs you", mine.join("; ")] as [string, string]] : [])],
    });
    agents.push(agent); parts.push(agent);

    // Runtime: child processes, wired along their own parent PIDs when the parent is drawn too.
    const tools = a.tools.slice(0, SCENE_LIMITS.tools);
    const toolIds = new Map(tools.map((t) => [t.pid, `${a.id}:tool:${t.pid}`]));
    for (const t of tools) {
      const id = toolIds.get(t.pid)!;
      parts.push(part({
        id, kind: "tool", layer: "runtime", owner: a.id, label: t.name, sub: `${TOOL_LABEL[t.kind]} · PID ${t.pid}`, evidence: t.evidence, live: true, variant: t.kind,
        cpu: t.cpu_percent, memory: t.memory_bytes, target: { view: "agents", agentId: a.id },
        facts: [["Kind", TOOL_LABEL[t.kind]], ["PID", String(t.pid)], ["CPU", pct(t.cpu_percent)], ["Memory", humanBytes(t.memory_bytes)],
          ...(t.cwd ? [["Folder", tildify(t.cwd)] as [string, string]] : []), ...(t.ports.length ? [["Ports", t.ports.map((p) => `:${p}`).join(", ")] as [string, string]] : []),
          ["How it's known", t.evidence === "observed" ? "Seen in the process table" : "Classified from its name and command"]],
      }));
      const parent = t.ppid !== null ? toolIds.get(t.ppid) : undefined;
      wires.push({ id: `process:${id}`, from: parent ?? a.id, to: id, kind: "process", label: "Child process", evidence: t.evidence });
    }
    const moreTools = a.tools.length - tools.length + a.more_tools;
    if (moreTools > 0) parts.push(more(a.id, "runtime", "processes", moreTools));

    // Board: listeners, held by the exact drawn process when there is one.
    const ports = a.ports.slice(0, SCENE_LIMITS.ports);
    for (const p of ports) {
      const id = `${a.id}:port:${p.entry_id}`;
      const exposed = p.exposure === "all_interfaces";
      const entry = entryById.get(p.entry_id);
      portPart.set(p.entry_id, id);
      parts.push(part({
        id, kind: "port", layer: "board", owner: a.id, label: `:${p.port}`, sub: p.framework ?? p.label, evidence: "observed", live: true, variant: p.exposure,
        warn: exposed ? "Reachable from other machines on your network" : null, target: { view: "list", entryId: p.entry_id },
        facts: [["Protocol", p.protocol.toUpperCase()], ["Bound to", exposed ? "All interfaces" : p.exposure === "loopback" ? "This computer only" : "A specific interface"],
          ["Held by", p.process ? `${p.process}${p.pid ? ` · PID ${p.pid}` : ""}` : "Unknown process"], ...(p.project ? [["Project", p.project] as [string, string]] : []),
          ...(entry?.protected ? [["Protected", "holdmap won't stop it"] as [string, string]] : [])],
      }));
      const holder = p.pid !== null ? toolIds.get(p.pid) : undefined;
      wires.push({ id: `listener:${id}`, from: holder ?? a.id, to: id, kind: "listener", label: `Holds :${p.port}`, evidence: "observed" });
    }
    if (a.ports.length > ports.length) parts.push(more(a.id, "board", "ports", a.ports.length - ports.length));

    // Board: working folders (recent history isn't attributed to this instance, so it's left out, as on the map).
    const folders = [...new Map(a.folders.filter((f) => f.source !== "recent").map((f) => [f.project?.name ?? f.label, f])).values()];
    for (const f of folders.slice(0, SCENE_LIMITS.projects)) {
      const name = f.project?.name ?? f.label;
      const id = `${a.id}:project:${name}`;
      parts.push(part({
        id, kind: "project", layer: "board", owner: a.id, label: name, sub: f.project?.git_branch ? `on ${f.project.git_branch}` : "Working folder", evidence: f.evidence, live: f.pids.length > 0, variant: f.source,
        target: { view: "agents", agentId: a.id }, facts: [["Folder", tildify(f.path)], ...(f.project ? [["Project", `${f.project.kind}${f.project.git_branch ? ` · ${f.project.git_branch}` : ""}`] as [string, string]] : []), ["Processes there", String(f.pids.length)]],
      }));
      wires.push({ id: `folder:${id}`, from: a.id, to: id, kind: "folder", label: "Works in this folder", evidence: f.evidence });
    }
    if (folders.length > SCENE_LIMITS.projects) parts.push(more(a.id, "board", "folders", folders.length - SCENE_LIMITS.projects));

    // Remote hosts: one group per agent, so a chatty agent doesn't bury its own parts.
    const remotes = a.links.filter((l) => l.kind === "remote");
    if (remotes.length) {
      const id = `${a.id}:remotes`;
      const connections = remotes.reduce((n, l) => n + l.connections, 0);
      const services = [...new Set(remotes.map((l) => l.service).filter((s): s is string => !!s))];
      parts.push(part({
        id, kind: "remote", layer: "board", owner: a.id, label: remotes.length === 1 ? remotes[0].address : plural(remotes.length, "remote host"),
        sub: `${plural(connections, "connection")}${services.length ? ` · ${services.slice(0, 2).join(", ")}` : ""}`, evidence: "observed", live: true, variant: "remote", count: remotes.length,
        target: { view: "agents", agentId: a.id }, facts: [...remotes.slice(0, 8).map((l): [string, string] => [l.address, plural(l.connections, "connection")]),
          ...(remotes.length > 8 || a.more_links ? [["More", `${remotes.length - Math.min(8, remotes.length) + a.more_links} not listed`] as [string, string]] : []),
          ["Names", "Shown by address; holdmap doesn't look names up"]],
      }));
      wires.push({ id: `socket:${id}`, from: a.id, to: id, kind: "socket", label: plural(connections, "open connection"), evidence: "observed", weight: connections });
    }

    configParts(a.id, a.capabilities, trustByDir, parts, wires);
    // Running MCP servers: the one config ↔ process wire, to this agent's exact process when drawn.
    for (const s of a.capabilities?.inspected ? a.capabilities.mcp_servers.slice(0, SCENE_LIMITS.mcp) : []) {
      if (s.runtime?.state !== "running") continue;
      const pid = s.runtime.pids.find((p) => toolIds.has(p));
      wires.push({ id: `running:${a.id}:${s.id}`, from: `${a.id}:mcp:${s.id}`, to: pid !== undefined ? toolIds.get(pid)! : a.id, kind: "running",
        label: `Running now: PID ${s.runtime.pids.join(", ")}`, evidence: "observed" });
    }
  }

  // Sockets from one agent to a listener another (or the same) drawn agent holds: exact entry IDs only.
  for (const a of running) {
    for (const l of a.links) {
      const to = l.kind === "local" && l.entry_id ? portPart.get(l.entry_id) : undefined;
      if (!to || to.startsWith(`${a.id}:`)) continue;
      wires.push({ id: `socket:${a.id}:${l.id}`, from: a.id, to, kind: "socket", label: `${plural(l.connections, "connection")} to :${l.port}`, evidence: "observed", weight: l.connections });
    }
  }

  // Agents that aren't running but whose default configuration was readable (opt-in inventory).
  for (const c of (report?.configured ?? []).slice(0, SCENE_LIMITS.dormant)) {
    const id = `configured:${c.product}`;
    const agent = part({ id, kind: "dormant", layer: "board", owner: id, label: c.name, sub: "Not running · configuration found", evidence: "configured", variant: "dormant",
      target: null, facts: [["State", "Not running: known only from its default configuration files"]] });
    agents.push(agent); parts.push(agent);
    configParts(id, c.capabilities, trustByDir, parts, wires);
  }

  const calls = callParts(activity);
  parts.push(...calls);

  const inspected = (report?.agents ?? []).some((a) => a.capabilities?.inspected) || !!report?.configured?.length;
  return {
    agents, parts, wires, inspected, calls: calls.length, recording: !!recording?.active,
    omittedAgents: Math.max(0, (report?.agents.length ?? 0) - running.length) + Math.max(0, (report?.configured?.length ?? 0) - SCENE_LIMITS.dormant),
    signature: JSON.stringify([parts.map((p) => [p.id, p.owner]), wires.map((w) => w.id)]),
  };
}

export interface Reading {
  id: "cpu" | "memory" | "ports" | "runtime" | "mcp" | "skills" | "needs" | "calls";
  label: string;
  /** Null when the figure wasn't collected (configuration not read): never shown as zero. */
  value: string | null;
  sub: string;
  tone?: "warn" | "ok";
  /** 0–1 fill for the meter, when a share makes sense. */
  ratio?: number;
}

/** Live readings for the whole machine, or for one agent. Every figure comes from the drawn scene.
 *  Before the first scan (`scanned` false) every figure reads "…", as Home's summary does, never 0. */
export function readings(scene: MachineScene, focus: string | null = null, scanned = true): Reading[] {
  const mine = (p: Part) => !focus || p.owner === focus;
  const agents = scene.agents.filter((a) => a.kind === "agent" && mine(a));
  const of = (kind: PartKind) => scene.parts.filter((p) => p.kind === kind && mine(p));
  const cpu = agents.reduce((n, a) => n + a.cpu, 0), memory = agents.reduce((n, a) => n + a.memory, 0);
  const ports = of("port"), exposed = ports.filter((p) => p.warn).length;
  const tools = of("tool"), mcp = of("mcp"), skills = of("skill");
  const runningMcp = mcp.filter((m) => m.live).length;
  const flagged = skills.filter((s) => s.warn).length;
  const needs = scene.parts.filter((p) => p.warn && p.kind !== "call" && mine(p)).length;
  const config = !scene.inspected;
  if (!scanned) {
    const label = { cpu: "CPU", memory: "Memory", ports: "Ports", runtime: "Processes", mcp: "MCP running", skills: "Skills", needs: "Needs you" } as const;
    return (Object.keys(label) as (keyof typeof label)[]).map((id) => ({ id, label: label[id], value: "…", sub: "first scan" }));
  }
  return [
    { id: "cpu", label: "CPU", value: pct(cpu), sub: focus ? "this agent" : plural(agents.length, "agent"), ratio: Math.min(1, cpu / 100) },
    { id: "memory", label: "Memory", value: humanBytes(memory), sub: focus ? "this agent" : "all agents" },
    { id: "ports", label: "Ports", value: String(ports.length), sub: exposed ? `${exposed} exposed` : "local only", tone: exposed ? "warn" : undefined },
    { id: "runtime", label: "Processes", value: String(tools.length), sub: `${tools.filter((t) => t.variant === "mcp_server").length} MCP · ${tools.filter((t) => t.variant === "dev_server").length} dev` },
    { id: "mcp", label: "MCP running", value: config ? null : String(runningMcp), sub: config ? "opt-in, off" : `of ${mcp.length} configured`, ratio: mcp.length ? runningMcp / mcp.length : undefined, tone: runningMcp ? "ok" : undefined },
    { id: "skills", label: "Skills", value: config ? null : String(skills.length), sub: config ? "opt-in, off" : flagged ? `${flagged} to review` : "files found", tone: flagged ? "warn" : undefined },
    { id: "needs", label: "Needs you", value: String(needs), sub: needs ? "marked in amber" : "all clear", tone: needs ? "warn" : "ok" },
    // Calls aren't any agent's: the reading stays machine-wide, and appears only once recording has been used.
    ...(scene.calls || scene.recording ? [{ id: "calls" as const, label: "Calls", value: String(scene.calls), sub: scene.recording ? "recording" : "recording off",
      tone: scene.parts.some((p) => p.kind === "call" && p.warn) ? "warn" as const : undefined }] : []),
  ];
}

/** Readings that light up their parts when clicked, and which parts they mean. */
export const GAUGES = ["ports", "runtime", "mcp", "skills", "needs", "calls"] as const satisfies readonly Reading["id"][];
export type Gauge = (typeof GAUGES)[number];
export const isGauge = (id: Reading["id"]): id is Gauge => (GAUGES as readonly string[]).includes(id);

/** The exact parts a reading counts, for the whole machine or the agent in focus. "MCP running"
 *  lights running servers and, through the scene's own running wires, the processes that run them. */
export function gaugeParts(scene: MachineScene, gauge: Gauge, focus: string | null = null): Set<string> {
  const mine = (p: Part) => gauge === "calls" || !focus || p.owner === focus;
  const pick = (test: (p: Part) => boolean) => new Set(scene.parts.filter((p) => mine(p) && test(p)).map((p) => p.id));
  switch (gauge) {
    case "ports": return pick((p) => p.kind === "port");
    case "runtime": return pick((p) => p.kind === "tool");
    case "skills": return pick((p) => p.kind === "skill");
    case "needs": return pick((p) => !!p.warn && p.kind !== "call");
    case "calls": return pick((p) => p.kind === "call");
    case "mcp": {
      const running = pick((p) => p.kind === "mcp" && p.live);
      for (const w of scene.wires) if (w.kind === "running" && running.has(w.from)) running.add(w.to);
      return running;
    }
  }
}

/** Lenses: one concern at a time. Each lights the parts it is about, dims the rest, and sets
 *  aside the floors that would only stand in the way. */
export const LENSES = ["all", "network", "runtime", "config", "needs", "calls"] as const;
export type Lens = (typeof LENSES)[number];
export const LENS_LABEL: Record<Lens, string> = { all: "All", network: "Network", runtime: "Runtime", config: "Config", needs: "Needs you", calls: "Calls" };
export const LENS_TITLE: Record<Lens, string> = {
  all: "Everything on this computer",
  network: "Ports, who holds them, connections and where they go; configuration set aside",
  runtime: "Processes, their tree and the ports they hold; configuration set aside",
  config: "MCP servers, skills and rules on disk, and the processes running them",
  needs: "Only what needs you, and the agents it belongs to",
  calls: "Recorded calls, on their own shelf",
};

export function lensView(scene: MachineScene, lens: Lens): { lit: Set<string> | null; hidden: Set<Layer> } {
  const pick = (test: (p: Part) => boolean) => new Set(scene.parts.filter(test).map((p) => p.id));
  const along = (lit: Set<string>, kinds: readonly WireKind[]) => {
    for (const w of scene.wires) if (kinds.includes(w.kind)) { lit.add(w.from); lit.add(w.to); }
    return lit;
  };
  switch (lens) {
    case "all": return { lit: null, hidden: new Set() };
    case "network": return { lit: along(pick((p) => p.kind === "port" || p.kind === "remote"), ["listener", "socket"]), hidden: new Set(["config", "calls"]) };
    case "runtime": return { lit: along(pick((p) => p.kind === "tool" || p.kind === "more" && p.layer === "runtime"), ["process", "listener"]), hidden: new Set(["config", "calls"]) };
    case "config": return { lit: along(pick((p) => p.layer === "config"), ["running"]), hidden: new Set(["calls"]) };
    case "needs": {
      const lit = pick((p) => !!p.warn && p.kind !== "call");
      for (const p of scene.parts) if (lit.has(p.id)) lit.add(p.owner);
      return { lit, hidden: new Set() };
    }
    case "calls": return { lit: pick((p) => p.kind === "call"), hidden: new Set() };
  }
}

/** One sentence about an agent, for the tour caption and the rail. */
export function agentSummary(scene: MachineScene, id: string): string {
  const parts = scene.parts.filter((p) => p.owner === id);
  const n = (kind: PartKind) => parts.filter((p) => p.kind === kind).length;
  const mcp = parts.filter((p) => p.kind === "mcp"), running = mcp.filter((m) => m.live).length;
  const bits = [n("tool") && plural(n("tool"), "process", "processes"), n("port") && plural(n("port"), "port"),
    mcp.length && `${mcp.length} MCP configured${running ? `, ${running} running` : ""}`, n("skill") && plural(n("skill"), "skill")].filter(Boolean);
  return bits.join(" · ") || "Nothing else drawn";
}
