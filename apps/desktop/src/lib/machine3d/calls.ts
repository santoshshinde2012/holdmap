// Recorded calls (opt-in local recording) as parts in their own lane above the machine. They are
// reported by an agent integration, not observed, so they never join a running process or a
// configured server: no wire touches them. Order in the lane follows reported time, so a call
// sits near the calls around it; its reported parent is named in its details.
import { activityName, activityStatus } from "../agent-flow/build";
import { PROVIDER_LABEL, SOURCE_LABEL, type ActivitySnapshot, type ActivitySpan } from "../agent-flow/model";
import type { Part } from "./scene";

/** Newest calls drawn; older ones stay in Agents → Flow → Calls. */
export const CALL_LIMIT = 36;
export const CALLS_OWNER = "calls";

const KIND_LABEL: Record<ActivitySpan["kind"], string> = {
  agent: "Agent run", tool: "Tool call", model: "Model request", handoff: "Handoff", step: "Step", skill: "Skill activation",
};

const run = (s: ActivitySpan) => s.trace_id || s.session_id || "event";
const time = (s: ActivitySpan) => s.start_at_ms ?? s.end_at_ms ?? Number.NEGATIVE_INFINITY;
export const callId = (s: ActivitySpan) => `call:${run(s)}:${s.id}`;

function duration(ms: number): string {
  return ms < 1000 ? `${Math.round(ms)} ms` : ms < 60_000 ? `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s` : `${Math.round(ms / 60_000)} min`;
}
const bytes = (n: number | null) => (n === null ? "?" : n < 1024 ? `${n} B` : `${(n / 1024).toFixed(1)} KB`);

/** The newest calls, newest first. */
export function callParts(activity: ActivitySnapshot | null | undefined): Part[] {
  const all = activity?.spans ?? [];
  const byRun = new Map(all.map((s) => [`${run(s)}:${s.id}`, s]));
  const spans = [...all].sort((a, b) => time(b) - time(a) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)).slice(0, CALL_LIMIT);
  const parts: Part[] = [];
  for (const s of spans) {
    const parent = s.parent_id ? byRun.get(`${run(s)}:${s.parent_id}`) : undefined;
    const id = callId(s);
    const status = activityStatus(s);
    parts.push({
      id, kind: "call", layer: "calls", owner: CALLS_OWNER, label: activityName(s),
      sub: [KIND_LABEL[s.kind], status, s.duration_ms !== null ? duration(s.duration_ms) : null].filter(Boolean).join(" · "),
      evidence: "reported", live: s.status === "running" || s.status === "requested", warn: s.status === "error" ? "Reported as failed" : null,
      variant: s.status === "error" ? "error" : s.kind, cpu: 0, memory: 0, count: 0, target: null,
      facts: [
        ["Reported by", `${PROVIDER_LABEL[s.provider]} · ${SOURCE_LABEL[s.source]}`],
        ["Kind", KIND_LABEL[s.kind]], ["Status", status],
        ...(s.duration_ms !== null ? [["Duration", duration(s.duration_ms)] as [string, string]] : []),
        ...(parent ? [["Reported parent", activityName(parent)] as [string, string]] : []),
        ...(s.mcp_server ? [["MCP server (reported)", s.mcp_server] as [string, string]] : []),
        ...(s.input_bytes !== null || s.output_bytes !== null ? [["Sizes", `${bytes(s.input_bytes)} in · ${bytes(s.output_bytes)} out`] as [string, string]] : []),
        ["Matched to", "Nothing: a reported call isn't tied to a process or a configured server"],
      ],
    });
  }
  return parts;
}
