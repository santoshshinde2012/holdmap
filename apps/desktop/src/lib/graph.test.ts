import { describe, expect, it } from "vitest";
import { edgeLabel, fitEdgeLabel, linksLabel, needsMiniMap, force, groupId, layered, NODE_H, NODE_W, nodeForEntry, orderFromSummary, related, sectionsByCluster, separate, toFlow, trafficDots } from "./graph";
import { MOCK_SNAPSHOT, mockTopology } from "./mock";

const g = mockTopology();

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
});

describe("isolated services", () => {
  it("are gridded below the connected graph instead of one tall column", () => {
    const extra = ["a", "b", "c", "d", "e"].map((id, i) => ({ ...g.nodes.find((n) => n.id === "docs")!, id, label: id, ports: [{ ...g.nodes[0].ports[0], port: 9000 + i, entry_id: `x${i}` }] }));
    const gg = { ...g, nodes: [...g.nodes, ...extra] };
    const p = layered(gg);
    const connectedBottom = Math.max(...g.nodes.map((n) => p.get(n.id)!.y));
    const rows = new Set(extra.map((n) => p.get(n.id)!.y));
    expect(Math.min(...rows)).toBeGreaterThan(connectedBottom);
    expect(rows.size).toBeLessThan(extra.length);
  });
});
