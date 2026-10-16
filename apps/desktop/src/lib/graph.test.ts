import { describe, expect, it } from "vitest";
import { edgeLabel, fitEdgeLabel, graphForQuery, graphNodeMatches, linksLabel, localLinkCounts, needsMiniMap, force, FORCE_NODE_LIMIT, groupId, layered, layoutModeFor, NODE_H, NODE_W, nodeForEntry, nodeSubtitle, orderFromSummary, related, sectionsByCluster, separate, toFlow, trafficDots } from "./graph";
import { MOCK_SNAPSHOT, mockTopology } from "./mock";
import type { Graph } from "./types";

const g = mockTopology();

describe("graph search", () => {
  it("preserves the original graph when no search is requested", () => {
    expect(graphForQuery(null, "shop")).toBeNull();
    expect(graphForQuery(g, "  ")).toBe(g);
    expect(g.nodes.every((node) => graphNodeMatches(node, ""))).toBe(true);
  });

  it("matches case-insensitive metadata terms together without searching commands", () => {
    const node = { ...g.nodes[0], id: "node-identity", label: "Shop API", subtitle: "Application", project: "checkout",
      project_root: "/work/checkout", cluster: "compose:shop", framework: { name: "Express", category: "app_server" as const },
      container: { id: "container", name: "api", image: "node:22-alpine", runtime: "docker", private_port: 3000, compose_project: "shop", compose_service: "api" },
      command: "SECRET_COMMAND_SHOULD_NOT_BE_SEARCHED" };
    for (const query of ["SHOP express", "checkout /work", "node:22 compose:shop", "node-identity APPLICATION", "shop pid:43000"]) {
      expect(graphNodeMatches(node, query), query).toBe(true);
    }
    expect(graphNodeMatches(node, "shop missing-term")).toBe(false);
    expect(graphNodeMatches(node, "SECRET_COMMAND_SHOULD_NOT_BE_SEARCHED")).toBe(false);
  });

  it("matches explicit PID and port tokens exactly, including their combination", () => {
    const node = { ...g.nodes[0], root_pid: 42, pids: [42, 420], ports: [{ ...g.nodes[0].ports[0], port: 3000 }] };
    for (const query of [":3000", "port:3000", "PID:42", "pid:420 port:3000", "pid:042 :03000"]) expect(graphNodeMatches(node, query), query).toBe(true);
    for (const query of [":300", ":30001", "port:300", "pid:4", "pid:4200", "pid:9007199254740993"]) expect(graphNodeMatches(node, query), query).toBe(false);
  });

  it("retains one-hop observed context, trims clusters and recomputes visible statistics", () => {
    const graph = { ...g, nodes: g.nodes.slice(0, 4).map((node, i) => ({ ...node, id: `node-${i}`, label: i === 0 ? "unique-target" : `context-${i}`, cluster: "scope" })),
      clusters: [{ ...g.clusters[0], id: "scope", nodes: ["node-0", "node-1", "node-2", "node-3"] }],
      edges: [[0, 1], [1, 2], [2, 3]].map(([from, to], index) => ({ ...g.edges[0], id: `edge-${index}`, from: `node-${from}`, to: `node-${to}`, connections: index + 2 })) };
    const before = structuredClone(graph);
    const shown = graphForQuery(graph, "unique-target")!;
    expect(shown.nodes.map((node) => node.id)).toEqual(["node-0", "node-1"]);
    expect(shown.edges.map((edge) => edge.id)).toEqual(["edge-0"]);
    expect(shown.clusters[0].nodes).toEqual(["node-0", "node-1"]);
    expect(shown.stats).toEqual({ nodes: 2, edges: 1, clusters: 1, connections: 2 });
    expect(shown.taken_at_ms).toBe(graph.taken_at_ms);
    expect(graph).toEqual(before);
    expect(graphNodeMatches(shown.nodes[1], "unique-target")).toBe(false);
  });

  it("drops missing endpoints and never fabricates edges for matching services", () => {
    const graph = { ...g, nodes: g.nodes.slice(0, 2).map((node) => ({ ...node, label: "matching" })), edges: [{ ...g.edges[0], from: g.nodes[0].id, to: "missing" }] };
    const shown = graphForQuery(graph, "matching")!;
    expect(shown.nodes).toHaveLength(2);
    expect(shown.edges).toEqual([]);
    const empty = graphForQuery(graph, "no-such-node")!;
    expect(empty.nodes).toEqual([]);
    expect(empty.edges).toEqual([]);
    expect(empty.clusters).toEqual([]);
    expect(empty.stats).toEqual({ nodes: 0, edges: 0, clusters: 0, connections: 0 });
  });
});

describe("layout", () => {
  it("layered puts clients left of the services they use", () => {
    const p = layered(g);
    expect(p.size).toBe(g.nodes.length);
    expect(p.get("web")!.x).toBeLessThan(p.get("api")!.x);
    expect(p.get("api")!.x).toBeLessThan(p.get("db")!.x);
  });

  it("force layout is deterministic and overlap-free", () => {
    const a = force(g), b = force(g);
    expect([...a.entries()]).toEqual([...b.entries()]);
    const pts = [...a.values()];
    for (let i = 0; i < pts.length; i++)
      for (let j = i + 1; j < pts.length; j++)
        expect(Math.abs(pts[i].x - pts[j].x) >= NODE_W || Math.abs(pts[i].y - pts[j].y) >= NODE_H).toBe(true);
  });

  it("force keeps nodes with no edges close enough to fit on screen", () => {
    const lone = { ...g, nodes: g.nodes.filter((n) => n.id === "redis" || n.id === "docs").map((n) => ({ ...n, cluster: null })), edges: [], clusters: [] };
    const p = force(lone);
    expect(Math.abs(p.get("redis")!.x - p.get("docs")!.x)).toBeLessThan(1200);
  });

  it("uses the layered layout for large graphs even with a saved force preference", () => {
    const large = { ...g, nodes: Array.from({ length: 1000 }, (_, index) => ({ ...g.nodes[0], id: `large-${index}`, cluster: null })), edges: [], clusters: [] };
    expect(layoutModeFor(FORCE_NODE_LIMIT, "force")).toBe("force");
    expect(layoutModeFor(large.nodes.length, "force")).toBe("layered");
    expect(force(large)).toEqual(layered(large));
  });

  it("separate pushes stacked nodes apart", () => {
    const p = new Map([["a", { x: 0, y: 0 }], ["b", { x: 0, y: 0 }]]);
    separate(p);
    expect(Math.abs(p.get("a")!.x - p.get("b")!.x) >= NODE_W || Math.abs(p.get("a")!.y - p.get("b")!.y) >= NODE_H).toBe(true);
  });
});

describe("toFlow", () => {
  it("emits cluster groups before their children with relative positions inside the hull", () => {
    const { nodes, edges } = toFlow(g, layered(g));
    const group = nodes.find((n) => n.id === groupId("shop"))!;
    expect(group.type).toBe("cluster");
    expect(nodes.indexOf(group)).toBeLessThan(nodes.findIndex((n) => n.id === "web"));
    for (const id of ["web", "api", "db", "redis"]) {
      const n = nodes.find((x) => x.id === id)!;
      expect(n.parentId).toBe(group.id);
      expect(n.position.x).toBeGreaterThanOrEqual(0);
      expect(n.position.y).toBeGreaterThanOrEqual(0);
      expect(n.position.x + NODE_W).toBeLessThanOrEqual(group.width!);
      expect(n.position.y + NODE_H).toBeLessThanOrEqual(group.height!);
    }
    expect(nodes.find((n) => n.id === "docs")!.parentId).toBeUndefined();
    expect(edges).toHaveLength(g.edges.length);
    expect(edges[0].markerEnd.type).toBe("arrowclosed");
  });

  it("dims everything outside the hovered node's dependency chain", () => {
    const { nodes, edges } = toFlow(g, layered(g), { focus: "web", animate: false });
    const dim = (id: string) => nodes.find((n) => n.id === id)!.data.dim;
    expect(dim("web") || dim("api") || dim("db") || dim("redis")).toBe(false);
    expect(dim("docs")).toBe(true);
    expect(edges.find((e) => e.id === "web->api")!.data.hl).toBe(true);
    expect(edges.find((e) => e.id === "docs->api")!.data.dim).toBe(true);
    expect(edges.every((e) => e.data.animate === false)).toBe(true);
  });
});

describe("helpers", () => {
  it("related walks both directions transitively", () => {
    const r = related(g, "api");
    expect([...r.nodes].sort()).toEqual(["api", "db", "docs", "redis", "web"]);
    expect(r.edges.has("ml->db")).toBe(false);
  });

  it("counts distinct local peers and excludes outbound links", () => {
    const counts = localLinkCounts({ ...g, edges: [...g.edges, { ...g.edges[0], id: "duplicate" }] });
    for (const node of g.nodes) {
      const expected = new Set(g.edges.filter((edge) => edge.kind === "local" && (edge.from === node.id || edge.to === node.id)).map((edge) => edge.from === node.id ? edge.to : edge.from)).size;
      for (const port of node.ports) expect(counts.get(port.entry_id)).toBe(expected);
    }
    expect(localLinkCounts(null).size).toBe(0);
  });

  it("walks cycles without adding another branch through a shared dependency", () => {
    const edges = [
      { ...g.edges[0], id: "a-b", from: "a", to: "b" },
      { ...g.edges[0], id: "b-a", from: "b", to: "a" },
      { ...g.edges[0], id: "a-db", from: "a", to: "db" },
      { ...g.edges[0], id: "other-db", from: "other", to: "db" },
    ];
    const result = related({ ...g, edges }, "a");
    expect([...result.nodes].sort()).toEqual(["a", "b", "db"]);
    expect([...result.edges].sort()).toEqual(["a-b", "a-db", "b-a"]);
  });

  it("parses the stop order from a cluster plan summary", () => {
    expect(orderFromSummary("Stop cluster shop (3 services) in dependency order: web :3000 → api :3001 → db :5432.")).toEqual(["web :3000", "api :3001", "db :5432"]);
    expect(orderFromSummary("Stop node")).toEqual([]);
  });

  it("groups list entries by cluster with the rest last", () => {
    const secs = sectionsByCluster(MOCK_SNAPSHOT.entries, g);
    expect(secs[0].title).toBe("shop");
    expect(secs[0].items.map((e) => e.port).sort()).toEqual([3000, 3001, 5432, 6379]);
    expect(secs.at(-1)!.title).toBe("Not in a cluster");
    expect(sectionsByCluster(MOCK_SNAPSHOT.entries, null)[0].title).toBe("");
  });

  it("maps entries to nodes and labels edges", () => {
    const web = MOCK_SNAPSHOT.entries.find((e) => e.port === 3000)!;
    expect(nodeForEntry(g, web.id)?.id).toBe("web");
    expect(edgeLabel(g.edges[0])).toBe(":3001 ×4");
    expect(edgeLabel(g.edges.find((e) => e.kind === "outbound")!)).toBe("api.openai.com:443, huggingface.co:443");
    expect(trafficDots(1).count).toBe(1);
    expect(trafficDots(100).count).toBe(4);
  });

  it("shortens an edge label so it never covers the nodes it joins", () => {
    const hosts = "api.openai.com:443, huggingface.co:443";
    // Short horizontal hop between neighbouring nodes: clipped with an ellipsis, within the gap.
    const short = fitEdgeLabel(hosts, 0, 0, 112, 10);
    expect(short.clipped).toBe(true);
    expect(short.text.endsWith("…")).toBe(true);
    expect(short.text.length * 6.7 + 14).toBeLessThanOrEqual(112 - 28);
    // Plenty of room, or a vertical edge that passes beside the nodes: left alone.
    expect(fitEdgeLabel(":3001 ×4", 0, 0, 112, 0)).toEqual({ text: ":3001 ×4", clipped: false });
    expect(fitEdgeLabel(":5432", 0, 0, 10, 160).clipped).toBe(false);
    // Never collapses to nothing.
    expect(fitEdgeLabel(hosts, 0, 0, 4, 0).text.length).toBeGreaterThanOrEqual(3);
  });

  it("shows the minimap only while part of the graph is off-screen", () => {
    const b = { x: 0, y: 0, width: 1000, height: 600 };
    // Fitted: everything visible, so no minimap to cover nodes.
    expect(needsMiniMap(b, { x: 20, y: 80, zoom: 0.9 }, 1000, 700)).toBe(false);
    // Zoomed in or panned: content spills past an edge.
    expect(needsMiniMap(b, { x: 0, y: 0, zoom: 1.5 }, 1000, 700)).toBe(true);
    expect(needsMiniMap(b, { x: -200, y: 80, zoom: 0.9 }, 1000, 700)).toBe(true);
    // A few px of rounding at the edge doesn't count; unmeasured canvas never shows it.
    expect(needsMiniMap(b, { x: 4, y: 4, zoom: 1 }, 1000, 600)).toBe(false);
    expect(needsMiniMap(b, { x: 0, y: 0, zoom: 1 }, 0, 0)).toBe(false);
  });

  it("labels the Graph badge count in words", () => {
    expect(linksLabel(1)).toBe("1 link");
    expect(linksLabel(6)).toBe("6 links");
  });

  it("does not repeat the service name as a subtitle", () => {
    const ollama = g.nodes[0];
    expect(nodeSubtitle({ ...ollama, label: "Ollama", framework: { name: "Ollama", category: "app" }, kind: "service", subtitle: "Ollama", container: null })).toBe("");
    expect(nodeSubtitle({ ...ollama, label: "shop-web", framework: { name: "Next.js", category: "dev_server" }, kind: "service", subtitle: "Next.js" })).toBe("Next.js");
    expect(nodeSubtitle({ ...ollama, kind: "external", label: "External", framework: null, subtitle: null })).toBe("Internet & remote hosts");
  });
});

describe("disconnected services", () => {
  const makeSparse = (count: number, connected = false): Graph => {
    const nodes = Array.from({ length: count }, (_, index) => ({
      ...g.nodes[0], id: `service-${String(index).padStart(4, "0")}`, cluster: null,
    }));
    const edges = connected ? [[0, 1], [2, 3], [4, 5], [6, 7]].map(([from, to], index) => ({
      ...g.edges[0], id: `link-${index}`, from: nodes[from].id, to: nodes[to].id,
    })) : [];
    return { ...g, nodes, edges, clusters: [] };
  };
  const bounds = (positions: ReturnType<typeof layered>) => {
    const points = [...positions.values()];
    return {
      width: Math.max(...points.map((p) => p.x)) - Math.min(...points.map((p) => p.x)) + NODE_W,
      height: Math.max(...points.map((p) => p.y)) - Math.min(...points.map((p) => p.y)) + NODE_H,
    };
  };
  const expectSeparated = (positions: ReturnType<typeof layered>) => {
    const points = [...positions.values()];
    for (let i = 0; i < points.length; i++) for (let j = i + 1; j < points.length; j++) {
      expect(Math.abs(points[i].x - points[j].x) >= NODE_W || Math.abs(points[i].y - points[j].y) >= NODE_H).toBe(true);
    }
  };

  it.each([layered, force])("packs loose services beside connected components without a long empty rank (%#)", (layout) => {
    const extra = ["a", "b", "c", "d", "e"].map((id, i) => ({ ...g.nodes.find((n) => n.id === "docs")!, id, label: id, ports: [{ ...g.nodes[0].ports[0], port: 9000 + i, entry_id: `x${i}` }] }));
    const gg = { ...g, nodes: [...g.nodes, ...extra] };
    const p = layout(gg);
    const rows = new Set(extra.map((n) => p.get(n.id)!.y));
    expect(rows.size).toBeLessThan(extra.length);
    expect(p.size).toBe(gg.nodes.length);
    expectSeparated(p);
  });

  it.each([layered, force])("keeps 21 services with only four links compact and preserves the observed graph (%#)", (layout) => {
    const sparse = makeSparse(21, true);
    const before = structuredClone(sparse);
    const positions = layout(sparse);
    const size = bounds(positions);
    expect(size.width).toBeLessThan(1500);
    expect(size.height).toBeLessThan(1000);
    expectSeparated(positions);
    expect(sparse).toEqual(before);
    expect(toFlow(sparse, positions).edges.map((e) => e.id)).toEqual(sparse.edges.map((e) => e.id));
  });

  it.each([21, 48])("keeps %i isolated services in a compact grid in both modes", (count) => {
    const graph = makeSparse(count);
    const layeredPositions = layered(graph);
    expect(force(graph)).toEqual(layeredPositions);
    const size = bounds(layeredPositions);
    expect(size.width / size.height).toBeGreaterThan(1);
    expect(size.width / size.height).toBeLessThan(2.5);
    expect(size.width * size.height).toBeLessThan(count * NODE_W * NODE_H * 2.5);
    expectSeparated(layeredPositions);
  });

  it.each([layered, force])("packs disconnected cluster hulls without covering another cluster or loose service (%#)", (layout) => {
    const graph = makeSparse(9);
    const cluster = g.clusters[0];
    graph.clusters = ["first", "second"].map((id, index) => ({ ...cluster, id, nodes: graph.nodes.slice(index * 3, index * 3 + 3).map((n) => n.id) }));
    graph.nodes = graph.nodes.map((node, index) => ({ ...node, cluster: index < 6 ? graph.clusters[Math.floor(index / 3)].id : null }));
    const positions = layout(graph);
    const groups = toFlow(graph, positions).nodes.filter((node) => node.type === "cluster");
    expect(groups).toHaveLength(2);
    const separated = (a: { x: number; y: number; width: number; height: number }, b: { x: number; y: number; width: number; height: number }) =>
      a.x + a.width <= b.x || b.x + b.width <= a.x || a.y + a.height <= b.y || b.y + b.height <= a.y;
    const first = { ...groups[0].position, width: groups[0].width!, height: groups[0].height! };
    const second = { ...groups[1].position, width: groups[1].width!, height: groups[1].height! };
    expect(separated(first, second)).toBe(true);
    for (const node of graph.nodes.filter((node) => !node.cluster)) {
      const point = positions.get(node.id)!;
      const box = { x: point.x - NODE_W / 2, y: point.y - NODE_H / 2, width: NODE_W, height: NODE_H };
      expect(separated(first, box)).toBe(true);
      expect(separated(second, box)).toBe(true);
    }
    expectSeparated(positions);
  });

  it.each([layered, force])("keeps connected chains directed and stable when scan input order changes (%#)", (layout) => {
    const graph = makeSparse(7);
    graph.edges = graph.nodes.slice(1).map((node, index) => ({ ...g.edges[0], id: `chain-${index}`, from: graph.nodes[index].id, to: node.id }));
    const positions = layout(graph);
    for (const edge of graph.edges) expect(positions.get(edge.from)!.x).toBeLessThan(positions.get(edge.to)!.x);
    const reordered = { ...graph, nodes: [...graph.nodes].reverse(), edges: [...graph.edges].reverse(), clusters: [...graph.clusters].reverse() };
    expect(layout(reordered)).toEqual(positions);
    expectSeparated(positions);
  });

  it("packs 1000 isolated services within the bounded Force fallback without a simulation per service", () => {
    const graph = makeSparse(1000);
    const start = performance.now();
    const positions = force(graph);
    expect(positions.size).toBe(1000);
    expect(positions).toEqual(layered(graph));
    expect(performance.now() - start).toBeLessThan(1500);
    const size = bounds(positions);
    expect(size.width / size.height).toBeGreaterThan(1);
    expect(size.width / size.height).toBeLessThan(2.5);
  });

  it("returns an empty layout for an empty graph", () => {
    const empty = { ...g, nodes: [], edges: [], clusters: [] };
    expect(layered(empty).size).toBe(0);
    expect(force(empty).size).toBe(0);
  });
});
