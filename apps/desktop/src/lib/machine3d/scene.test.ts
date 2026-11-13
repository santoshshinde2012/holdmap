import { describe, expect, it } from "vitest";
import { MOCK_SNAPSHOT } from "../mock";
import { mockAgents } from "../mock-agents";
import { attention, hotAgents, needsByAgent } from "../home";
import { agentSummary, buildScene, gaugeParts, readings, SCENE_LIMITS } from "./scene";
import { CALL_LIMIT } from "./calls";
import { partChip } from "./describe";
import { demoActivity } from "../activity-demo";
import type { Agent, AgentsReport } from "../types";
import type { SkillTrustView } from "../trust";

const report = mockAgents(undefined, true);
/** What Home passes in: agents that need you themselves, and every "Needs you" title by agent. */
const attended = (r: AgentsReport | null) => { const items = attention(MOCK_SNAPSHOT.entries, r); return { hot: hotAgents(items), needs: needsByAgent(items, r) }; };
const scene = (r: AgentsReport | null = report, extra: Partial<Parameters<typeof buildScene>[0]> = {}) =>
  buildScene({ report: r, entries: MOCK_SNAPSHOT.entries, ...attended(r), ...extra });

describe("3D machine scene", () => {
  it("puts every running agent on the board in report order, then dormant configured agents", () => {
    const s = scene();
    expect(s.agents.map((a) => a.label)).toEqual([...report.agents.map((a) => a.name), "Codex CLI"]);
    expect(s.agents.at(-1)).toMatchObject({ kind: "dormant", evidence: "configured", target: null });
    expect(s.inspected).toBe(true);
  });

  it("draws processes on the runtime layer and wires them along their own parent PIDs", () => {
    const s = scene();
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const next = s.parts.find((p) => p.id === `${cursor.id}:tool:43000`)!;
    expect(next).toMatchObject({ layer: "runtime", variant: "dev_server", evidence: "observed" });
    // Next.js runs under the zsh that is drawn too, so the wire starts at that shell, not the agent.
    expect(s.wires.find((w) => w.to === next.id)).toMatchObject({ kind: "process", from: `${cursor.id}:tool:52040` });
  });

  it("holds a port from the exact process that holds it, and flags ports open to the network", () => {
    const s = scene();
    const claude = report.agents.find((a) => a.product === "claude-code")!;
    const port = s.parts.find((p) => p.kind === "port" && p.owner === claude.id)!;
    const holder = claude.ports[0].pid!;
    expect(s.wires.find((w) => w.to === port.id)).toMatchObject({ kind: "listener", from: `${claude.id}:tool:${holder}` });
    const exposedReport = { ...report, agents: report.agents.map((a) => a.id === claude.id ? { ...a, ports: a.ports.map((p) => ({ ...p, exposure: "all_interfaces" as const })) } : a) };
    expect(scene(exposedReport).parts.find((p) => p.id === port.id)!.warn).toMatch(/other machines/);
  });

  it("only joins configuration to a process through a running MCP server's own PIDs", () => {
    const s = scene();
    const running = s.wires.filter((w) => w.kind === "running");
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const fs = cursor.capabilities!.mcp_servers.find((m) => m.runtime?.state === "running")!;
    expect(running).toContainEqual(expect.objectContaining({ from: `${cursor.id}:mcp:${fs.id}`, to: `${cursor.id}:tool:${fs.runtime!.pids[0]}`, evidence: "observed" }));
    // Every other configuration wire starts at the agent that owns the file, never at a process.
    for (const w of s.wires.filter((w) => w.kind === "config")) expect(s.agents.map((a) => a.id)).toContain(w.from);
    // Every running wire comes from a server whose run state says running.
    for (const w of running) expect(s.parts.find((p) => p.id === w.from)).toMatchObject({ kind: "mcp", live: true });
  });

  it("never joins a same-named server across agents", () => {
    const s = scene();
    const claude = report.agents.find((a) => a.product === "claude-code")!;
    // Claude Code also configures "filesystem", but no process of Claude Code runs it.
    const fs = s.parts.find((p) => p.owner === claude.id && p.kind === "mcp" && p.label === "filesystem")!;
    expect(fs.live).toBe(false);
    expect(s.wires.filter((w) => w.from === fs.id || w.to === fs.id).map((w) => w.kind)).toEqual(["config"]);
  });

  it("marks unpinned MCP servers and disabled ones without hiding them", () => {
    const s = scene();
    expect(s.parts.filter((p) => p.kind === "mcp" && p.warn).length).toBeGreaterThan(0);
    expect(s.parts.find((p) => p.kind === "mcp" && p.label === "legacy-browser")).toMatchObject({ variant: "off", live: false });
  });

  it("links skill trust only by the exact skill folder", () => {
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const skill = cursor.capabilities!.skills[0];
    const dir = skill.source_path.replace(/\/SKILL\.md$/, "");
    const view = (d: string, state: SkillTrustView["state"]): SkillTrustView => ({ subject: { id: d, kind: "skill", name: skill.name, agents: [], dir: d }, state, file_count: 1, total_bytes: 1, partial: false, first_seen_ms: 0, seen_ms: 0, changed_since_last_scan: false });
    const exact = scene(report, { trust: [view(dir, "changed")] }).parts.find((p) => p.id === `${cursor.id}:skill:${skill.id}`)!;
    expect(exact).toMatchObject({ variant: "changed", warn: "Changed since you trusted it" });
    // A different folder with the same skill name doesn't count.
    const other = scene(report, { trust: [view(`/elsewhere/${skill.name}`, "changed")] }).parts.find((p) => p.id === `${cursor.id}:skill:${skill.id}`)!;
    expect(other.warn).toBeNull();
  });

  it("joins a socket to another agent's listener by exact entry ID", () => {
    const s = scene();
    const claude = report.agents.find((a) => a.product === "claude-code")!;
    const docker = report.agents.find((a) => a.product === "docker-desktop")!;
    const pg = docker.ports.find((p) => p.port === 5432)!;
    expect(s.wires).toContainEqual(expect.objectContaining({ kind: "socket", from: claude.id, to: `${docker.id}:port:${pg.entry_id}`, weight: 3 }));
  });

  it("leaves recent-history folders out, as the map does", () => {
    const s = scene();
    const claude = report.agents.find((a) => a.product === "claude-code")!;
    expect(s.parts.filter((p) => p.owner === claude.id && p.kind === "project").map((p) => p.label)).toEqual(["shop-api"]);
  });

  it("bounds every list and says how many were left out", () => {
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const tools = Array.from({ length: 12 }, (_, i) => ({ ...cursor.tools[0], pid: 60000 + i, ppid: 1 }));
    const busy: Agent = { ...cursor, tools, more_tools: 3 };
    const s = scene({ ...report, agents: [busy] });
    expect(s.parts.filter((p) => p.kind === "tool")).toHaveLength(SCENE_LIMITS.tools);
    expect(s.parts.find((p) => p.kind === "more" && p.layer === "runtime")).toMatchObject({ count: 12 - SCENE_LIMITS.tools + 3, label: `+${12 - SCENE_LIMITS.tools + 3}` });
    const many = scene({ ...report, agents: Array.from({ length: SCENE_LIMITS.agents + 3 }, (_, i) => ({ ...cursor, id: `agent:${i}` })) });
    expect(many.agents.filter((a) => a.kind === "agent")).toHaveLength(SCENE_LIMITS.agents);
    expect(many.omittedAgents).toBe(3);
  });

  it("keeps the configuration layer empty and readings unknown when configuration wasn't read", () => {
    const plain = mockAgents();
    const s = scene(plain);
    expect(s.inspected).toBe(false);
    expect(s.parts.some((p) => p.layer === "config")).toBe(false);
    const r = readings(s);
    expect(r.find((x) => x.id === "mcp")).toMatchObject({ value: null, sub: "opt-in, off" });
    expect(r.find((x) => x.id === "skills")!.value).toBeNull();
  });

  it("keeps the layout identity when only resource samples change", () => {
    const a = scene();
    const sampled = { ...report, agents: report.agents.map((x) => ({ ...x, cpu_percent: x.cpu_percent + 7, memory_bytes: x.memory_bytes * 2 })) };
    expect(scene(sampled).signature).toBe(a.signature);
  });

  it("reads the machine or one agent from the same parts it draws", () => {
    const s = scene();
    const all = readings(s);
    const cpu = report.agents.reduce((n, a) => n + a.cpu_percent, 0);
    expect(all.find((r) => r.id === "cpu")!.value).toBe(`${cpu < 10 ? cpu.toFixed(1) : Math.round(cpu)}%`);
    expect(all.find((r) => r.id === "ports")!.value).toBe(String(s.parts.filter((p) => p.kind === "port").length));
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const one = readings(s, cursor.id);
    expect(one.find((r) => r.id === "cpu")).toMatchObject({ value: `${cursor.cpu_percent.toFixed(1)}%`, sub: "this agent" });
    expect(one.find((r) => r.id === "mcp")).toMatchObject({ value: "1", sub: `of ${cursor.capabilities!.mcp_servers.length} configured` });
    expect(agentSummary(s, cursor.id)).toMatch(/processes · 2 ports · 3 MCP configured, 1 running · 2 skills/);
  });

  it("reads … before the first scan, never 0", () => {
    const s = buildScene({ report: null });
    expect(s.parts).toEqual([]);
    expect(readings(s, null, false).map((r) => r.value)).toEqual(Array(7).fill("…"));
    expect(readings(s, null, false).map((r) => r.id)).toEqual(readings(s).map((r) => r.id));
  });

  it("puts recorded calls in their own lane, newest first, with nothing tying them to a process or a file", () => {
    const activity = demoActivity();
    const s = scene(report, { activity, recording: { active: true, endpoint: null, started_at_ms: 0, received: 1, dropped: 0 } });
    const calls = s.parts.filter((p) => p.kind === "call");
    expect(calls).toHaveLength(Math.min(CALL_LIMIT, activity.spans.length));
    expect(s.calls).toBe(calls.length);
    for (const c of calls) expect(c).toMatchObject({ layer: "calls", evidence: "reported", target: null });
    // No wire touches a call: a reported PID or alias never joins a process or a configured server.
    const ids = new Set(calls.map((c) => c.id));
    expect(s.wires.filter((w) => ids.has(w.from) || ids.has(w.to))).toEqual([]);
    const times = calls.map((c) => activity.spans.find((sp) => c.id.endsWith(`:${sp.id}`))!).map((sp) => sp.start_at_ms ?? sp.end_at_ms ?? -Infinity);
    expect(times).toEqual([...times].sort((a, b) => b - a));
    expect(calls.find((c) => c.variant === "error")).toMatchObject({ warn: "Reported as failed" });
    expect(calls.find((c) => c.label === "Review agent")!.facts).toContainEqual(["Reported parent", "Delegate to reviewer"]);
    expect(partChip(calls[0])).toMatchObject({ label: "Reported" });
    // The reading appears with recording; failed calls don't count as Needs you.
    expect(readings(s).find((r) => r.id === "calls")).toMatchObject({ value: String(calls.length), sub: "recording" });
    expect(readings(s).find((r) => r.id === "needs")!.value).toBe(readings(scene()).find((r) => r.id === "needs")!.value);
    expect(readings(scene()).some((r) => r.id === "calls")).toBe(false);
  });

  it("bounds the calls drawn", () => {
    const one = demoActivity().spans[0];
    const many = { ...demoActivity(), spans: Array.from({ length: CALL_LIMIT + 9 }, (_, i) => ({ ...one, id: `s${i}`, start_at_ms: i })) };
    expect(scene(report, { activity: many }).calls).toBe(CALL_LIMIT);
  });

  it("lights exactly the parts a reading counts, for the machine or the agent in focus", () => {
    const s = scene(report, { activity: demoActivity() });
    const ids = (pred: (p: (typeof s.parts)[number]) => boolean) => new Set(s.parts.filter(pred).map((p) => p.id));
    expect(gaugeParts(s, "ports")).toEqual(ids((p) => p.kind === "port"));
    expect(gaugeParts(s, "runtime")).toEqual(ids((p) => p.kind === "tool"));
    expect(gaugeParts(s, "skills")).toEqual(ids((p) => p.kind === "skill"));
    expect(gaugeParts(s, "needs")).toEqual(ids((p) => !!p.warn && p.kind !== "call"));
    expect(gaugeParts(s, "calls")).toEqual(ids((p) => p.kind === "call"));
    // MCP running: the running server and, through its own running wire, the process that runs it.
    const cursor = report.agents.find((a) => a.product === "cursor")!;
    const fs = cursor.capabilities!.mcp_servers.find((m) => m.runtime?.state === "running")!;
    expect(gaugeParts(s, "mcp")).toEqual(new Set([`${cursor.id}:mcp:${fs.id}`, `${cursor.id}:tool:${fs.runtime!.pids[0]}`]));
    // Scoped to an agent, except calls, which belong to no agent.
    for (const id of gaugeParts(s, "ports", cursor.id)) expect(id.startsWith(`${cursor.id}:`)).toBe(true);
    expect(gaugeParts(s, "calls", cursor.id)).toEqual(gaugeParts(s, "calls"));
  });
});
