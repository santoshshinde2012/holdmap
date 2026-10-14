// @vitest-environment jsdom
// The details pane under live polling: a fresh entry object with new figures must update the
// text in place, never remount the panel, flash the skeleton or scroll back to the top.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import DetailPane from "./DetailPane.svelte";
import NetworkPanel from "./NetworkPanel.svelte";
import ProcessPanel from "./ProcessPanel.svelte";
import { MOCK_SNAPSHOT, mockDetails, mockExplain } from "../../lib/mock";
import type { ActionPlan, PortEntry } from "../../lib/types";

beforeAll(() => {
  const anim = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(anim);
  Element.prototype.scrollTo ??= () => {};
});
afterEach(cleanup);

const base = MOCK_SNAPSHOT.entries.find((e) => e.port === 3000)!;
/** What a poll hands over: a structurally new object, here with a new memory figure. */
const polled = (mem: number): PortEntry => ({ ...structuredClone(base), app_memory_bytes: mem, process: { ...base.process!, memory_bytes: mem } });
const props = (entry: PortEntry, loading = false) => ({ entry, explanation: mockExplain(3000), loading, busy: false, onstop() {}, onkill() {}, onopen() {}, oncopy() {} });

describe("DetailPane under polling", () => {
  it("keeps every node and shows no skeleton when a poll replaces the entry", async () => {
    const { container, rerender } = render(DetailPane, props(polled(100 * 2 ** 20)));
    await tick();
    const body = container.querySelector(".body")!;
    const headline = container.querySelector(".headline")!;
    const plan = container.querySelector(".plan")!;
    expect(body.textContent).toContain("100 MB");
    const scroll = vi.spyOn(Element.prototype, "scrollTo");

    // A revalidation in flight (loading) with the last explanation still at hand.
    await rerender(props(polled(140 * 2 ** 20), true));
    await tick();
    expect(container.querySelector(".sk")).toBeNull();
    expect(container.querySelector(".body")).toBe(body);
    expect(container.querySelector(".headline")).toBe(headline);
    expect(container.querySelector(".plan")).toBe(plan);
    expect(body.textContent).toContain("140 MB");
    expect(scroll).not.toHaveBeenCalled();
    scroll.mockRestore();
  });

  it("shows the skeleton only on a port's first load", async () => {
    const { container } = render(DetailPane, { ...props(polled(1)), explanation: null, loading: true });
    await tick();
    expect(container.querySelector(".sk")).not.toBeNull();
  });
});

describe("lazy details", () => {
  it("explains an exposed bind address and lists who is connected", async () => {
    const exposed = MOCK_SNAPSHOT.entries.find((e) => e.exposure === "all_interfaces" && e.process && e.protocol === "tcp")!;
    const d = { ...mockDetails(exposed.port)[0], connections: { total: 2, established: 2, by_state: { established: 2 }, peers: [{ address: "192.168.1.24", connections: 2, local: false, process: null, pid: null }], more_peers: 0 } };
    const { container } = render(NetworkPanel, { entry: exposed, oncopy() {}, details: d });
    await tick();
    expect(container.textContent).toContain("Anyone on your network can connect");
    expect(container.textContent).toContain("Fix:");
    expect(container.textContent).toContain("192.168.1.24");
    expect(container.textContent).toContain("×2");
  });

  it("draws the process tree with the listener marked", async () => {
    const d = mockDetails(3000)[0];
    const { container } = render(ProcessPanel, { entry: base, node: null, oncopy() {}, details: d });
    await tick();
    const items = [...container.querySelectorAll(".tree li")].map((li) => li.textContent ?? "");
    expect(items[0]).toContain("launchd");
    expect(items.find((t) => t.includes("listening"))).toContain(base.process!.name);
    expect(items.at(-1)).toContain("esbuild");
  });
});

describe("port action capabilities", () => {
  it("offers a hide control for the details pane", async () => {
    const onhide = vi.fn();
    const view = render(DetailPane, { ...props(base), onhide });
    await fireEvent.click(view.getByRole("button", { name: "Hide details pane" }));
    expect(onhide).toHaveBeenCalledOnce();
  });

  it("offers a separate scheduled-stop review without running an immediate stop", async () => {
    const onstop = vi.fn();
    const onschedule = vi.fn();
    const view = render(DetailPane, { ...props(base), onstop, onschedule });
    const footer = view.getByLabelText("Port actions");
    expect(footer.textContent).not.toContain("Port :3000 / TCP");
    expect(footer.textContent).toContain("keeps this computer running");
    await fireEvent.click(view.getByRole("button", { name: "Schedule stop…" }));
    expect(onschedule).toHaveBeenCalledTimes(1);
    expect(onstop).not.toHaveBeenCalled();
    await fireEvent.click(view.getByRole("button", { name: /^Stop…/ }));
    expect(onstop).toHaveBeenCalledTimes(1);
  });

  it.each(["busy", "loading"] as const)("disables stop and scheduling while %s", async (state) => {
    const user = userEvent.setup();
    const onstop = vi.fn();
    const onschedule = vi.fn();
    const view = render(DetailPane, { ...props(base), [state]: true, onstop, onschedule });
    const stop = view.getByRole("button", { name: state === "busy" ? "Stopping…" : /^Stop…/ }) as HTMLButtonElement;
    const schedule = view.getByRole("button", { name: "Schedule stop…" }) as HTMLButtonElement;
    expect(stop.disabled).toBe(true);
    expect(schedule.disabled).toBe(true);
    await user.click(stop);
    await user.click(schedule);
    expect(onstop).not.toHaveBeenCalled();
    expect(onschedule).not.toHaveBeenCalled();
  });

  it("waits for the first stop plan before enabling destructive controls", () => {
    const view = render(DetailPane, { ...props(base), explanation: null, loading: true, onschedule() {} });
    expect((view.getByRole("button", { name: /^Stop…/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(view.getByLabelText("Port actions").textContent).toContain("Checking stop permissions");
    expect(view.queryByRole("button", { name: "Schedule stop…" })).toBeNull();
  });

  it("retains the explicit soft-protection override while preventing scheduling or force shortcuts", async () => {
    const entry = { ...base, protected: true };
    const explanation = { ...mockExplain(3000), plan: { ...mockExplain(3000).plan!, blocked: { kind: "protected" as const, message: "Editor session", overridable: true }, steps: [] } };
    const onstop = vi.fn();
    const view = render(DetailPane, { ...props(entry), explanation, onstop, onschedule() {}, onrestart() {} });
    const stop = view.getByRole("button", { name: /^Stop…/ }) as HTMLButtonElement;
    expect(stop.disabled).toBe(false);
    expect(stop.classList.contains("danger-outline")).toBe(true);
    expect(view.getByLabelText("Port actions").textContent).toContain("review an explicit override");
    expect(view.queryByRole("button", { name: "Schedule stop…" })).toBeNull();
    expect(view.queryByRole("button", { name: "Restart" })).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "More actions" }));
    expect(view.queryByRole("menuitem", { name: /Force kill|Copy kill command/ })).toBeNull();
    await fireEvent.click(stop);
    expect(onstop).toHaveBeenCalledTimes(1);
  });

  it("explains hard protection and retains safe browser and copy actions", async () => {
    const entry = { ...base, protected: true };
    const explanation = { ...mockExplain(3000), plan: { ...mockExplain(3000).plan!, blocked: { kind: "protected" as const, message: "Holdmap's own process", overridable: false }, steps: [] } };
    const oncopy = vi.fn();
    const view = render(DetailPane, { ...props(entry), explanation, oncopy, onschedule() {} });
    expect((view.getByRole("button", { name: /^Stop…/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(view.getByLabelText("Port actions").textContent).toContain("Protected process");
    expect((view.getByRole("button", { name: /^Open/ }) as HTMLButtonElement).disabled).toBe(false);
    expect(view.queryByRole("button", { name: "Schedule stop…" })).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "More actions" }));
    await fireEvent.click(view.getByRole("menuitem", { name: /Copy URL/ }));
    expect(oncopy).toHaveBeenCalledWith("http://localhost:3000", "URL");
  });

  it("keeps an unavailable owner visibly disabled with an actionable explanation", () => {
    const entry = { ...base, pid: null, process: null, project: null };
    const view = render(DetailPane, { ...props(entry), explanation: null, onschedule() {} });
    expect((view.getByRole("button", { name: /^Stop…/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(view.getByLabelText("Port actions").textContent).toContain("owner is hidden");
    expect(view.queryByRole("button", { name: "Schedule stop…" })).toBeNull();
  });

  it.each(["remote", "protected", "override", "force", "empty"])("does not schedule a %s plan", (condition) => {
    const entry = { ...base, remote: condition === "remote" ? "dev-host" : null, protected: condition === "protected" };
    const plan: ActionPlan = structuredClone(mockExplain(3000).plan!);
    if (condition === "override") plan.allow_protected = true;
    if (condition === "force") plan.steps = plan.steps.map((s) => s.action === "signal_processes" ? { ...s, force: true } : s);
    if (condition === "empty") plan.steps = [];
    const view = render(DetailPane, { ...props(entry), explanation: { ...mockExplain(3000), plan }, onschedule() {} });
    expect(view.queryByRole("button", { name: "Schedule stop…" })).toBeNull();
  });

  it("requires an explicit scheduling callback and uses an unambiguous container action", () => {
    const entry = MOCK_SNAPSHOT.entries.find((e) => !!e.container)!;
    const view = render(DetailPane, { ...props(entry), explanation: mockExplain(entry.port) });
    expect(view.getByRole("button", { name: "Stop container…" })).toBeTruthy();
    expect(view.queryByRole("button", { name: "Schedule stop…" })).toBeNull();
  });
});

describe("detail navigation and compact activity", () => {
  it("pairs keyboard-selected tabs with their panels and recovers an unavailable tab", async () => {
    const view = render(DetailPane, props(base));
    const overview = view.getByRole("tab", { name: "Overview" });
    overview.focus();
    await fireEvent.keyDown(overview, { key: "End" });
    const commands = view.getByRole("tab", { name: /^Commands/ });
    expect(commands.getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(commands);
    expect(view.getByRole("tabpanel").getAttribute("aria-labelledby")).toBe(commands.id);
    await fireEvent.keyDown(commands, { key: "Home" });
    expect(overview.getAttribute("aria-selected")).toBe("true");
    await view.rerender({ ...props({ ...base, process: null, container: null }), tab: "process" });
    expect(view.getByRole("tab", { name: "Overview" }).getAttribute("aria-selected")).toBe("true");
    expect(view.getByRole("tabpanel").getAttribute("aria-labelledby")).toBe(overview.id);
  });

  it("separates readable activity values from decorative trends and keeps long project information", () => {
    const name = "very-long-project-name-".repeat(8);
    const path = `/Users/dev/projects/${name}`;
    const entry = { ...polled(512 * 2 ** 20), process: { ...base.process!, cpu_percent: 2.5 }, project: { ...base.project!, name, root: path } };
    const view = render(DetailPane, { ...props(entry), cpuTrend: [2, 4, 3], memTrend: [100, 140, 160] });
    const activity = view.getByLabelText("Port activity");
    expect(activity.tagName).toBe("DL");
    expect(activity.textContent).toContain("512 MB");
    expect(activity.textContent).toContain(name);
    expect(activity.textContent).toContain(`~/projects/${name}`);
    expect(activity.querySelectorAll(".trend")).toHaveLength(2);
    expect(activity.querySelector(".sv svg")).toBeNull();
    expect([...activity.querySelectorAll(".trend")].every((trend) => trend.getAttribute("aria-hidden") === "true")).toBe(true);
  });
});
