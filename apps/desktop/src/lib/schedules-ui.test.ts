// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import ShutdownDialog from "../components/ShutdownDialog.svelte";
import ConfirmDialog from "../components/ConfirmDialog.svelte";
import SchedulesDialog from "../components/SchedulesDialog.svelte";
import { MOCK_SNAPSHOT, mockPlan } from "./mock";
import type { ScheduledAction } from "./schedules";

beforeAll(() => {
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} }));
  Element.prototype.scrollIntoView ??= vi.fn();
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });

const powerProps = () => ({ preview: { confirmation_id: "power-token", platform: "macOS", hostname: "My computer", expires_in_secs: 60 }, loading: false, error: null, busy: false, requested: false, onconfirm: vi.fn(), onschedule: vi.fn(), onclose: vi.fn() });
const entry = MOCK_SNAPSHOT.entries.find((item) => item.port === 3000)!;
const stopProps = () => ({ entry, plan: { ...mockPlan("3000/tcp", false), confirmation_id: "stop-token" }, phase: "confirm" as const, log: [], report: null, onconfirm: vi.fn(), onschedule: vi.fn(), oncancel: vi.fn(), onoverride: vi.fn(), onreview: vi.fn() });
async function timing(label: string) {
  await fireEvent.click(screen.getByRole("combobox", { name: "When" }));
  await fireEvent.click(screen.getByRole("option", { name: label }));
}
function localDateTime(ms: number) {
  const date = new Date(ms);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

describe("shutdown schedule confirmation", () => {
  it("defaults to now, resets acknowledgement after a timing change, and submits the selected delay once", async () => {
    const props = powerProps();
    render(ShutdownDialog, props);
    expect(screen.getByRole("combobox", { name: "When" }).textContent).toContain("Now");
    await fireEvent.click(screen.getByRole("checkbox"));
    await timing("In 1 hour");
    expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(false);
    const submit = screen.getByRole("button", { name: "Schedule shutdown" }) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(submit); await fireEvent.click(submit);
    expect(props.onschedule).toHaveBeenCalledExactlyOnceWith(3600);
    expect(props.onconfirm).not.toHaveBeenCalled();
  });

  it("blocks invalid custom delays instead of submitting an old valid delay", async () => {
    const props = powerProps();
    render(ShutdownDialog, props);
    await timing("Custom delay…");
    await fireEvent.input(screen.getByRole("spinbutton", { name: "Delay in minutes" }), { target: { value: "0" } });
    expect(screen.getByText("Enter a whole number between 1 and 1,440 minutes.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Schedule shutdown" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("checkbox") as HTMLInputElement).disabled).toBe(true);
    expect(props.onschedule).not.toHaveBeenCalled();
  });

  it("keeps acknowledgement while time passes and resolves an absolute deadline at confirmation", async () => {
    vi.useFakeTimers();
    const base = new Date(2026, 9, 11, 10, 0, 0).getTime();
    vi.setSystemTime(base);
    const props = powerProps();
    render(ShutdownDialog, props);
    await timing("At a time…");
    await fireEvent.input(screen.getByLabelText("Local date and time"), { target: { value: localDateTime(base + 3600000) } });
    await fireEvent.click(screen.getByRole("checkbox"));
    await vi.advanceTimersByTimeAsync(30000);
    expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: "Schedule shutdown" }));
    expect(props.onschedule).toHaveBeenCalledExactlyOnceWith(3570);
  });

  it("presents a scheduled request separately from an executed shutdown", async () => {
    const props = powerProps();
    render(ShutdownDialog, { ...props, scheduled: schedule("shutdown"), onviewschedules: vi.fn() });
    expect(screen.getByText("Shutdown scheduled")).toBeTruthy();
    expect(screen.queryByText("Shutdown requested")).toBeNull();
    expect(screen.getByRole("button", { name: "View scheduled actions" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Shut down this computer" })).toBeNull();
  });
});

describe("reviewed service scheduling", () => {
  it("sends a schedule rather than an immediate stop and prevents resubmission", async () => {
    const props = stopProps();
    render(ConfirmDialog, props);
    await timing("In 30 min");
    const submit = screen.getByRole("button", { name: "Schedule stop :3000" });
    await fireEvent.click(submit); await fireEvent.click(submit);
    expect(props.onschedule).toHaveBeenCalledExactlyOnceWith(1800);
    expect(props.onconfirm).not.toHaveBeenCalled();
    expect((submit as HTMLButtonElement).disabled).toBe(true);
  });

  it("keeps scheduling unavailable for force stops, protection overrides and restarts", () => {
    for (const change of [{ restart: true }, { plan: { ...mockPlan("3000/tcp", true), confirmation_id: "force-token" } }, { plan: { ...mockPlan("3000/tcp", false), allow_protected: true } }]) {
      render(ConfirmDialog, { ...stopProps(), ...change });
      expect(screen.queryByRole("combobox", { name: "When" })).toBeNull();
      cleanup();
    }
  });

  it("requires an explicit fresh review after an expired plan or scheduling error", async () => {
    const props = stopProps();
    const { rerender } = render(ConfirmDialog, props);
    await timing("In 1 hour");
    await rerender({ ...props, schedulingError: "The reviewed plan expired." });
    expect((screen.getByRole("button", { name: "Schedule stop :3000" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: "Review again" }));
    expect(props.onreview).toHaveBeenCalledOnce();
    expect(props.onschedule).not.toHaveBeenCalled();
  });

  it("prevents Escape and timing edits while the schedule is being acknowledged", async () => {
    const props = stopProps();
    render(ConfirmDialog, { ...props, schedulingBusy: true });
    await fireEvent.keyDown(screen.getByRole("alertdialog"), { key: "Escape" });
    expect(props.oncancel).not.toHaveBeenCalled();
    expect((screen.getByRole("combobox", { name: "When" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Cancel" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

function schedule(kind: ScheduledAction["kind"] = "port", status: ScheduledAction["status"] = "pending"): ScheduledAction {
  return { id: "schedule-token", kind, label: kind === "shutdown" ? "Shutdown My computer" : "Stop :3000/tcp", targets: kind === "shutdown" ? [] : ["3000/tcp"], created_at_ms: Date.now(), due_at_ms: Date.now() + 3600000, status, message: null, cancellable: status === "pending" || status === "checking" };
}

describe("schedule management", () => {
  it("waits for cancellation acknowledgement and shows failures without claiming success", async () => {
    let reject!: (failure: Error) => void;
    const oncancel = vi.fn(() => new Promise<void>((_, fail) => { reject = fail; }));
    render(SchedulesDialog, { schedules: [schedule()], onclose: vi.fn(), oncancel });
    const cancel = screen.getByRole("button", { name: "Cancel Stop :3000/tcp" }) as HTMLButtonElement;
    await fireEvent.click(cancel);
    expect(cancel.disabled).toBe(true);
    expect(screen.getByRole("status").textContent).toBe("");
    reject(new Error("Execution already began."));
    await tick(); await tick();
    expect(screen.getByText("Cancellation was not confirmed")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("");
    expect(oncancel).toHaveBeenCalledExactlyOnceWith("schedule-token");
  });

  it("allows cancellation while checking but hides it once running, and never live-announces each countdown tick", async () => {
    const oncancel = vi.fn(async () => {});
    const { rerender } = render(SchedulesDialog, { schedules: [schedule("port", "checking")], onclose: vi.fn(), oncancel });
    expect(screen.getByRole("button", { name: "Cancel Stop :3000/tcp" })).toBeTruthy();
    await rerender({ schedules: [schedule("port", "running")], onclose: vi.fn(), oncancel });
    expect(screen.queryByRole("button", { name: "Cancel Stop :3000/tcp" })).toBeNull();
    expect(screen.getByRole("status").textContent).toBe("");
    expect(screen.getByText("Running")).toBeTruthy();
  });

  it("explains review-only recovery and browser simulation without unsafe automatic retries", () => {
    render(SchedulesDialog, { schedules: [{ ...schedule("agent", "needs_review"), message: "The process changed." }], onclose: vi.fn(), oncancel: vi.fn(async () => {}), demo: true });
    expect(screen.getByText("Needs review")).toBeTruthy();
    expect(screen.getByText(/Nothing runs automatically. Start a fresh review/)).toBeTruthy();
    expect(screen.getByText(/agent itself stays running/)).toBeTruthy();
    expect(screen.getByText(/Your computer, agents and ports are never changed/)).toBeTruthy();
  });
});
