import { describe, expect, it } from "vitest";
import { MOCK_SNAPSHOT, mockAgents } from "./mock";
import {
  AGENT_W, FOOT_W, accessFacts, accessHeadline, agentColor, column, columnX, counts, countsLine, edgeGeometry, footprintGraph, folderSourceLabel, guessHome,
  layoutFootprint, monogram, neighbours, parentName, resourcesLine, shortLabel, signature, sizeOf, step, stoppableEntries, stoppablePorts, tildePath, toFootFlow,
} from "./agents";
import type { AgentsReport } from "./types";

const report = () => mockAgents();

describe("footprint graph", () => {
  it("has one node per agent, folder, port, service and remote host, shared nodes once", () => {
    const g = footprintGraph(report());
    const kinds = (k: string) => g.nodes.filter((n) => n.kind === k).map((n) => n.label);
    expect(kinds("agent")).toEqual(["Claude Code", "Cursor", "Docker Desktop"]);
    // shop-web: Cursor works there, Claude lists it as recent — one node, two owners, observed.
    const web = g.nodes.find((n) => n.id === "folder:/Users/dev/code/shop-web")!;
    expect(web.owners).toEqual(["agent:51200", "agent:52000"]);
    expect(web.evidence).toBe("observed");
    expect(web.tone).toBe(null);
    expect(web.sub).toBe("⎇ feat/checkout");
    // shop-api :3001 is Claude's dev server and a service Cursor uses: one "port" node.
    const api = g.nodes.find((n) => n.entryId === MOCK_SNAPSHOT.entries.find((e) => e.port === 3001)!.id)!;
    expect(api.kind).toBe("port");
    expect(api.owners.sort()).toEqual(["agent:51200", "agent:52000"]);
    expect(kinds("remote")).toEqual(["203.0.113.10", "198.51.100.24", "203.0.113.40"]);
    expect(g.edges.find((e) => e.kind === "parent")).toMatchObject({ from: "agent:52000", to: "agent:51200", label: "started" });
    expect(new Set(g.edges.map((e) => e.id)).size).toBe(g.edges.length);
  });

  it("folds remote hosts beyond the limit into one node and can hide recent projects", () => {
    const g = footprintGraph(report(), { remoteLimit: 1, recent: false });
    const more = g.nodes.find((n) => n.id === "more:agent:52000")!;
    expect(more.label).toBe("+1 more");
    expect(more.sub).toBe("1 connection");
    expect(g.nodes.some((n) => n.id === "folder:/Users/dev/code/design-system")).toBe(false);
    expect(g.edges.filter((e) => e.kind === "recent")).toEqual([]);
  });

  it("marks inferred folders and exposed ports", () => {
    const r = report();
    r.agents[0].folders.push({ path: "/srv/billing", label: "billing", project: null, source: "recent", evidence: "inferred", pids: [], privacy_area: null, note: "decoded" });
    r.agents[0].ports[0] = { ...r.agents[0].ports[0], exposure: "all_interfaces" };
    const g = footprintGraph(r);
    expect(g.edges.find((e) => e.to === "folder:/srv/billing")).toMatchObject({ kind: "recent", inferred: true });
    expect(g.nodes.find((n) => n.entryId === r.agents[0].ports[0].entry_id)!.tone).toBe("warn");
  });

  it("is empty without agents", () => {
    const g = footprintGraph({ agents: [], platform: "linux", taken_at_ms: 0, limits: [] });
    expect(g).toEqual({ nodes: [], edges: [] });
  });
});

describe("layout", () => {
  it("puts folders left, agents in the centre column, ports and services right, remote hosts far right", () => {
    const g = footprintGraph(report());
    const pos = layoutFootprint(g);
    const xs = columnX();
    expect(xs[1] - xs[0]).toBeGreaterThan((FOOT_W + AGENT_W) / 2);
    for (const n of g.nodes) expect(pos.get(n.id)!.x).toBe(xs[column(n.kind)]);
  });

  it("never overlaps nodes in a column and is deterministic", () => {
    const g = footprintGraph(report());
    const a = layoutFootprint(g), b = layoutFootprint(g);
    expect([...a.entries()]).toEqual([...b.entries()]);
    for (const c of [0, 1, 2, 3]) {
      const col = g.nodes.filter((n) => column(n.kind) === c).map((n) => ({ y: a.get(n.id)!.y, h: sizeOf(n.kind).h })).sort((x, y) => x.y - y.y);
      for (let i = 1; i < col.length; i++) expect(col[i].y - col[i].h / 2).toBeGreaterThanOrEqual(col[i - 1].y + col[i - 1].h / 2);
    }
  });

  it("puts an agent right after the one it was started from", () => {
    const g = footprintGraph(report());
    const pos = layoutFootprint(g);
    expect(pos.get("agent:52000")!.y).toBeLessThan(pos.get("agent:51200")!.y);
  });

  it("keeps the same signature when only figures change", () => {
    const r = report();
    const s1 = signature(footprintGraph(r));
    r.agents[0].cpu_percent += 5;
    r.agents[0].links[0] = { ...r.agents[0].links[0], connections: 9 };
    expect(signature(footprintGraph(r))).toBe(s1);
  });
});

describe("flow conversion", () => {
  it("dims everything but the focused node's neighbours and colours edges by agent", () => {
    const g = footprintGraph(report());
    const pos = layoutFootprint(g);
    const { nodes, edges } = toFootFlow(g, pos, { focus: "agent:51200", selectedId: "agent:51200" });
    const nb = neighbours(g, "agent:51200");
    expect(nb.nodes.has("agent:52000")).toBe(true);
    const cursorDocs = nodes.find((n) => n.id === "folder:/Users/dev/code/docs")!;
    expect(cursorDocs.data.dim).toBe(true);
    expect(nodes.find((n) => n.id === "agent:51200")!.data.selected).toBe(true);
    expect(nodes.filter((n) => n.type === "header").map((n) => n.data.node.label)).toEqual(["Folders", "Agents", "Ports & services", "Remote hosts"]);
    const e = edges.find((x) => x.source === "agent:51200" && x.target.startsWith("remote:"))!;
    expect(e.data.color).toBe(agentColor(g, "agent:51200"));
    expect(e.data.hl).toBe(true);
  });

  it("draws side-to-side curves and a left bracket within a column", () => {
    const a = { x: 0, y: 0, w: 100, h: 40 }, b = { x: 300, y: 80, w: 100, h: 40 };
    expect(edgeGeometry(a, b).path).toBe("M 50 0 C 150 0, 150 80, 250 80");
    expect(edgeGeometry(b, a).path.startsWith("M 250 80")).toBe(true);
    // Labels sit near the target, so two agents' labels on a shared node don't collide.
    const g = edgeGeometry(a, b);
    expect(g.labelX).toBeGreaterThan(150);
    expect(g.labelY).toBeGreaterThan(40);
    const c = edgeGeometry({ x: 0, y: 0, w: 100, h: 40 }, { x: 0, y: 200, w: 100, h: 40 });
    expect(c.path.startsWith("M -50 0 C")).toBe(true);
    expect(c.labelX).toBeLessThan(-50);
  });
});

describe("cards", () => {
  it("summarise counts and the access headline", () => {
    const [claude, cursor] = report().agents;
    expect(countsLine(counts(claude))).toBe("1 folder · 1 port · 2 links · 1 stoppable");
    expect(counts(cursor).processes).toBe(15);
    expect(counts(cursor).stoppable).toBe(2);
    expect(accessHeadline(claude)).toEqual({ level: "restricted", text: "Sandboxed or limited" });
    expect(accessHeadline(cursor)).toEqual({ level: "standard", text: "Your account's access" });
    const root = { ...cursor, access: { ...cursor.access, root: true } };
    expect(accessHeadline(root).level).toBe("elevated");
    expect(accessFacts(claude).map((f) => f.topic)).toEqual(["user", "sandbox", "approvals", "network", "privacy"]);
    const partial = { ...claude, access: { ...claude.access, facts: [] } };
    expect(accessFacts(partial).every((f) => f.level === "unknown" && f.evidence === "unknown")).toBe(true);
  });

  it("helpers", () => {
    expect(monogram("Claude Code")).toBe("CC");
    expect(monogram("Cursor")).toBe("Cu");
    expect(shortLabel("PostgreSQL · container shop-db-1")).toBe("PostgreSQL");
    expect(tildePath("/Users/dev/code/x", "/Users/dev")).toBe("~/code/x");
    expect(tildePath("/Users/devx/code", "/Users/dev")).toBe("/Users/devx/code");
    expect(guessHome(report())).toBe("/Users/dev");
    expect(step(["a", "b", "c"], null, 1)).toBe("a");
    expect(step(["a", "b", "c"], "c", 1)).toBe("a");
    expect(step(["a", "b", "c"], "a", -1)).toBe("c");
    expect(step([], "a", 1)).toBe(null);
    expect(folderSourceLabel("child")).toBe("child cwd");
    expect(parentName(report(), report().agents[0])).toBe("Cursor");
    expect(resourcesLine(report().agents[0])).toMatch(/MB · .*process/);
    expect(report().agents.some((a) => a.kind === "tool" && a.product === "docker-desktop")).toBe(true);
  });

  it("the demo agents follow ports the demo stopped", () => {
    const snap = { ...MOCK_SNAPSHOT, entries: MOCK_SNAPSHOT.entries.filter((e) => e.port !== 3001) };
    const r: AgentsReport = mockAgents(snap);
    const claude = r.agents.find((a) => a.product === "claude-code")!;
    expect(claude.ports).toEqual([]);
    expect(claude.processes.some((p) => p.pid === 43001)).toBe(false);
    const cursor = r.agents.find((a) => a.product === "cursor")!;
    expect(cursor.links.some((l) => l.port === 3001)).toBe(false);
  });

  it("stoppable ports skip the agent's own listeners", () => {
    const r = report();
    const claude = r.agents.find((a) => a.product === "claude-code")!;
    expect(stoppablePorts(claude).every((p) => p.role !== "agent")).toBe(true);
    expect(stoppableEntries(claude, MOCK_SNAPSHOT.entries).every((e) => !e.protected)).toBe(true);
  });
});
