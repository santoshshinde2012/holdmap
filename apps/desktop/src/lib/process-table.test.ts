import { describe, expect, it } from "vitest";
import { nextSort, processTableRows, sortProcessRows } from "./process-table";
import { mockAgents } from "./mock-agents";

const report = mockAgents();
const agent = report.agents[1];
const memory = (report.memory ?? []).find((m) => m.agent_id === agent.id) ?? null;

describe("process table", () => {
  it("lists each process once in tree order with platform counters when reported", () => {
    const rows = processTableRows(agent, memory);
    expect(new Set(rows.map((r) => r.pid)).size).toBe(rows.length);
    expect(rows[0].depth).toBe(0);
    for (const row of memory?.processes ?? []) expect(rows.find((r) => r.pid === row.pid)?.bytes).toBe(row.scan_bytes);
  });

  it("falls back to the process table when no counters were reported", () => {
    const rows = processTableRows(agent, null);
    expect(rows.map((r) => r.bytes)).toEqual(rows.map((r) => agent.processes.find((p) => p.pid === r.pid)!.memory_bytes));
    expect(rows.every((r) => r.bits === "")).toBe(true);
  });

  it("sorts by memory, CPU and name, keeping unknown CPU last", () => {
    const rows = [
      { pid: 3, name: "b", bytes: 10, cpu: null },
      { pid: 1, name: "a", bytes: 30, cpu: 2 },
      { pid: 2, name: "c", bytes: 20, cpu: 9 },
    ].map((r) => ({ ...r, role: "helper", roleLabel: "Helper", depth: 0, command: "", dominant: false, dominantReason: null, bits: "" }));
    expect(sortProcessRows(rows, "memory", "desc").map((r) => r.pid)).toEqual([1, 2, 3]);
    expect(sortProcessRows(rows, "cpu", "desc").map((r) => r.pid)).toEqual([2, 1, 3]);
    expect(sortProcessRows(rows, "cpu", "asc").map((r) => r.pid)).toEqual([1, 2, 3]);
    expect(sortProcessRows(rows, "name", "asc").map((r) => r.pid)).toEqual([1, 3, 2]);
  });

  it("flips the same column and starts a new one at its natural order", () => {
    expect(nextSort({ key: "memory", direction: "desc" }, "memory")).toEqual({ key: "memory", direction: "asc" });
    expect(nextSort({ key: "memory", direction: "desc" }, "name")).toEqual({ key: "name", direction: "asc" });
    expect(nextSort({ key: "name", direction: "asc" }, "cpu")).toEqual({ key: "cpu", direction: "desc" });
  });
});
