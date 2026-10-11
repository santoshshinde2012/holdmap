// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import AgentFlowView from "./AgentFlowView.svelte";
import ActivityTimeline from "./ActivityTimeline.svelte";
import FlowInspector from "./FlowInspector.svelte";
import { activityRuns, callGraph, observedGraph, spanNodeId } from "../../../lib/agent-flow/build";
import type { ActivitySpan, AgentFlowReport } from "../../../lib/agent-flow/model";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
const base = { dark: false, reduced: true };
const span = (values: Partial<ActivitySpan> = {}): ActivitySpan => ({ id: "request", trace_id: "", parent_id: null, session_id: "session", agent_id: null, parent_agent_id: null, provider: "codex", source: "hook", kind: "tool", name: "Read file", start_at_ms: 1000, end_at_ms: null, duration_ms: null, status: "requested", input_bytes: 0, output_bytes: null, pid: 123, process_started_at: null, ...values });
const emptyReport: AgentFlowReport = { taken_at_ms: 1000, nodes: [], edges: [], limits: ["Process access was unavailable; this scan is incomplete."], omitted_nodes: 0, omitted_edges: 0 };

describe("flow UI evidence and recording", () => {
  it("shows collection limitations alongside the observed empty state", () => {
    const { container } = render(AgentFlowView, { ...base, flowReport: emptyReport });
    expect(container.textContent).toContain("No observed connections yet");
    expect(container.textContent).toContain("Sockets do not reveal messages or data direction");
    expect(container.querySelector<HTMLDetailsElement>(".limits")?.open).toBe(true);
    expect(container.textContent).toContain("this scan is incomplete");
  });
  it("separates observed connections from missing call instrumentation with keyboard navigation", async () => {
    const changed = vi.fn(), { getByRole, container } = render(AgentFlowView, { ...base, onflowmodechange: changed });
    const connections = getByRole("radio", { name: "Connections" });
    await fireEvent.keyDown(connections, { key: "ArrowRight" });
    expect(getByRole("radio", { name: "Calls" }).getAttribute("aria-checked")).toBe("true");
    expect(container.textContent).toContain("No recorded activity yet");
    expect(container.textContent).toContain("Running agents alone do not provide call telemetry");
    expect(changed).toHaveBeenLastCalledWith("calls");
  });
  it("labels browser sample metadata and prevents claiming local recording", async () => {
    const start = vi.fn(), { getByRole, container } = render(AgentFlowView, { ...base, demo: true, onstartrecording: start });
    await fireEvent.click(getByRole("radio", { name: "Calls" }));
    expect(container.textContent).toContain("Example data");
    expect(container.textContent).toContain("Example recording");
    expect((getByRole("button", { name: "Start recording" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(getByRole("button", { name: "Set up recording" }));
    expect(container.textContent).toContain("Recording is available in the local desktop app");
    expect(start).not.toHaveBeenCalled();
  });
  it("does not expose private configuration tokens in the UI or success message", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined), setup = vi.fn().mockResolvedValue('private_token = "sensitive-local-bearer"');
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const { getByRole, container } = render(AgentFlowView, { ...base, recording: { active: true, endpoint: "http://127.0.0.1:1234", started_at_ms: 1000, received: 0, dropped: 0 }, onactivitysetup: setup });
    await fireEvent.click(getByRole("radio", { name: "Calls" }));
    await fireEvent.click(getByRole("button", { name: "Setup" }));
    expect(setup).not.toHaveBeenCalled();
    await fireEvent.click(getByRole("button", { name: "Copy Codex settings" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('private_token = "sensitive-local-bearer"'));
    expect(setup).toHaveBeenCalledWith("codex");
    expect(container.textContent).toContain("configuration copied");
    expect(container.innerHTML).not.toContain("sensitive-local-bearer");
    vi.unstubAllGlobals();
  });
  it("starts native setup clipboard writing while configuration is still pending", async () => {
    let resolve!: (text: string) => void;
    const configuration = new Promise<string>((done) => { resolve = done; });
    class Item { constructor(public data: Record<string, Promise<Blob>>) {} }
    const write = vi.fn(async (items: Item[]) => { await items[0].data["text/plain"]; });
    vi.stubGlobal("ClipboardItem", Item);
    vi.stubGlobal("navigator", { clipboard: { write } });
    const setup = vi.fn(() => configuration);
    const { getByRole, container } = render(AgentFlowView, { ...base, recording: { active: true, endpoint: null, started_at_ms: null, received: 0, dropped: 0 }, onactivitysetup: setup });
    await fireEvent.click(getByRole("radio", { name: "Calls" }));
    await fireEvent.click(getByRole("button", { name: "Setup" }));
    expect(setup).not.toHaveBeenCalled();
    await fireEvent.click(getByRole("button", { name: "Copy Codex settings" }));
    expect(write).toHaveBeenCalledOnce();
    expect(container.textContent).not.toContain("configuration copied");
    resolve("private-generated-settings");
    await waitFor(() => expect(container.textContent).toContain("configuration copied"));
    expect(container.innerHTML).not.toContain("private-generated-settings");
  });
  it("reports clipboard failure without printing the private snippet or API error", async () => {
    vi.stubGlobal("navigator", { clipboard: { writeText: vi.fn().mockRejectedValue(new Error("secret-token")) } });
    const { getByRole, container } = render(AgentFlowView, { ...base, recording: { active: true, endpoint: null, started_at_ms: null, received: 0, dropped: 0 }, onactivitysetup: vi.fn().mockResolvedValue("secret-config") });
    await fireEvent.click(getByRole("radio", { name: "Calls" })); await fireEvent.click(getByRole("button", { name: "Setup" }));
    await fireEvent.click(getByRole("button", { name: "Copy Claude Code settings" }));
    await waitFor(() => expect(container.textContent).toContain("Configuration could not be copied"));
    expect(container.innerHTML).not.toMatch(/secret-token|secret-config/);
    vi.unstubAllGlobals();
  });
  it("shows an unavailable update state without printing external error content", () => {
    const { container } = render(AgentFlowView, { ...base, flowError: "private:command:token", flowReport: emptyReport });
    expect(container.textContent).toContain("Updates unavailable"); expect(container.innerHTML).not.toContain("private:command:token");
  });
  it("keeps requested calls and missing parents explicit in the accessible timeline", async () => {
    const s = span({ parent_id: "not-received", trace_id: "trace" }), run = activityRuns([s])[0], select = vi.fn();
    const { container, getByRole } = render(ActivityTimeline, { run, chosen: null, onselect: select });
    expect(container.textContent).toContain("Requested"); expect(container.textContent).toContain("No completion received");
    expect(container.textContent).toContain("Parent span not received"); expect(container.textContent).toContain("Event order is not proof of communication");
    await fireEvent.click(getByRole("button")); expect(select).toHaveBeenCalledWith(spanNodeId(s));
  });
  it("labels the known end time for completion-only telemetry without inventing a start", () => {
    const s = span({ source: "otlp", status: "unknown", start_at_ms: null, end_at_ms: 2000 });
    const { container } = render(ActivityTimeline, { run: activityRuns([s])[0], chosen: null, onselect() {} });
    expect(container.textContent).toContain("Ended"); expect(container.textContent).not.toContain("Time unavailable");
    expect(container.textContent).toContain("Status unknown");
  });
  it("distinguishes serialized payload sizes and unverified reported PIDs in call inspection", () => {
    const s = span(), graph = callGraph(activityRuns([s])[0]);
    const { container } = render(FlowInspector, { graph, chosen: spanNodeId(s), onclose() {} });
    expect(container.textContent).toContain("Serialized input0 B"); expect(container.textContent).toContain("Serialized outputNot provided");
    expect(container.textContent).toContain("123 · unverified"); expect(container.textContent).toContain("A requested call may not have executed");
  });
  it("explains live socket evidence without communication direction or transferred bytes", () => {
    const report: AgentFlowReport = { ...emptyReport, nodes: [
      { id: "a", kind: "process", label: "Worker", agent_id: null, pid: 99, start_token: 1, entry_id: null, address: null, port: null, protocol: null, tool_kind: null, evidence: "observed" },
      { id: "b", kind: "remote", label: "Endpoint", agent_id: null, pid: null, start_token: null, entry_id: null, address: "192.0.2.1", port: 443, protocol: "tcp", tool_kind: null, evidence: "observed" }],
      edges: [{ id: "socket", from: "a", to: "b", kind: "tcp", evidence: "observed", connections: 3, source_pids: [99] }] };
    const { container } = render(FlowInspector, { graph: observedGraph(report), chosen: "socket", onclose() {} });
    expect(container.textContent).toContain("Observed source PIDs99");
    expect(container.textContent).toContain("Direction, messages and transferred bytes cannot be determined");
  });
});
