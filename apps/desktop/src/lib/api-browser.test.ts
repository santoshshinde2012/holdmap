import { afterEach, describe, expect, it, vi } from "vitest";
import { agents, MAX_MOCK_PORTS, scan, topology } from "./api";
import { MOCK_SNAPSHOT, mockTopology } from "./mock";

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("bounded browser load fixtures", () => {
  it.each(["Infinity", "NaN", "-1", "-Infinity"])("ignores invalid mockPorts=%s", async (count) => {
    vi.useFakeTimers();
    vi.stubGlobal("location", { search: `?mockPorts=${count}` });
    const pending = scan(false);
    await vi.advanceTimersByTimeAsync(250);
    expect((await pending).entries).toHaveLength(MOCK_SNAPSHOT.entries.length);
  });

  it.each(["1000000000", "1e100"])("caps mockPorts=%s before allocating entries", async (count) => {
    vi.useFakeTimers();
    vi.stubGlobal("location", { search: `?mockPorts=${count}` });
    const pending = scan(false);
    await vi.advanceTimersByTimeAsync(250);
    const snapshot = await pending;
    expect(snapshot.entries).toHaveLength(MAX_MOCK_PORTS);
    expect(new Set(snapshot.entries.map((entry) => entry.id)).size).toBe(MAX_MOCK_PORTS);
  });

  it("retains supported finite load-test sizes", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("location", { search: "?mockPorts=100.5" });
    const pending = scan(false);
    await vi.advanceTimersByTimeAsync(250);
    expect((await pending).entries).toHaveLength(100);
  });

  it("keeps sample agents in unit tests instead of calling a live collector", async () => {
    const report = await agents();
    expect(report.agents.map((agent) => agent.name)).toEqual(["Claude Code", "Cursor", "Docker Desktop"]);
    expect(report.agents.some((agent) => agent.folders.some((folder) => folder.label === "shop-web"))).toBe(true);
  });

  it("expands All services from matching sample listeners without inventing links", async () => {
    vi.stubGlobal("location", { search: "?mockPorts=40" });
    vi.useFakeTimers();
    const pending = scan(false);
    await vi.advanceTimersByTimeAsync(250);
    const snapshot = await pending;
    const normal = await topology(false), all = await topology(true);
    const expected = snapshot.entries.filter((entry) => entry.protocol === "tcp" && entry.state === "listen");
    expect(normal.nodes).toHaveLength(mockTopology().nodes.length);
    expect(all.nodes.length).toBeGreaterThan(25);
    expect(all.nodes.flatMap((node) => node.ports.map((port) => port.entry_id)).sort()).toEqual(expected.map((entry) => entry.id).sort());
    expect(new Set(all.nodes.map((node) => node.id)).size).toBe(all.nodes.length);
    expect(all.edges).toEqual(normal.edges);
    const added = new Set(all.nodes.filter((node) => !normal.nodes.some((base) => base.id === node.id)).map((node) => node.id));
    expect(all.edges.every((edge) => !added.has(edge.from) && !added.has(edge.to))).toBe(true);
    expect((await topology(false)).nodes.map((node) => node.id)).toEqual(normal.nodes.map((node) => node.id));
  });

  it("retains listener identity, protection and resource metadata in the expanded graph", () => {
    const sample = structuredClone(MOCK_SNAPSHOT);
    const ollama = sample.entries.find((entry) => entry.port === 11434)!;
    ollama.app_memory_bytes = 3_000_000_000;
    ollama.protected = true;
    sample.entries.push({ ...ollama, id: "established-sample", port: 19000, state: "established" });
    const graph = mockTopology(sample, true);
    const node = graph.nodes.find((node) => node.ports.some((port) => port.entry_id === ollama.id))!;
    expect(node.root_pid).toBe(ollama.pid);
    expect(node.pids).toEqual(ollama.pids);
    expect(node.memory_bytes).toBe(ollama.app_memory_bytes);
    expect(node.protected).toBe(true);
    expect(node.framework).toEqual(ollama.framework);
    expect(node.ports).toEqual([{ port: ollama.port, protocol: ollama.protocol, exposure: ollama.exposure, entry_id: ollama.id }]);
    expect(graph.nodes.some((node) => node.ports.some((port) => port.port === 8125))).toBe(false);
    expect(graph.nodes.some((node) => node.ports.some((port) => port.entry_id === "established-sample"))).toBe(false);
  });

  it("removes stopped sample listeners and their links without mutating the snapshot", () => {
    const sample = structuredClone(MOCK_SNAPSHOT);
    sample.entries = sample.entries.filter((entry) => ![3001, 11434].includes(entry.port));
    const before = structuredClone(sample);
    const graph = mockTopology(sample, true);
    expect(graph.nodes.some((node) => node.ports.some((port) => [3001, 11434].includes(port.port)))).toBe(false);
    const ids = new Set(graph.nodes.map((node) => node.id));
    expect(graph.edges.every((edge) => ids.has(edge.from) && ids.has(edge.to))).toBe(true);
    expect(graph.clusters[0].nodes).not.toContain("api");
    expect(graph.stats).toEqual({ nodes: graph.nodes.length, edges: graph.edges.length, clusters: graph.clusters.length, connections: graph.edges.reduce((count, edge) => count + edge.connections, 0) });
    expect(sample).toEqual(before);
  });
});
