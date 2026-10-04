// @vitest-environment jsdom
// The details pane under live polling: a fresh entry object with new figures must update the
// text in place, never remount the panel, flash the skeleton or scroll back to the top.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { tick } from "svelte";
import DetailPane from "./DetailPane.svelte";
import NetworkPanel from "./NetworkPanel.svelte";
import ProcessPanel from "./ProcessPanel.svelte";
import { MOCK_SNAPSHOT, mockDetails, mockExplain } from "../../lib/mock";
import type { PortEntry } from "../../lib/types";

beforeAll(() => {
  const anim = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(anim);
  Element.prototype.scrollTo ??= () => {};
});
afterEach(cleanup);

const base = MOCK_SNAPSHOT.entries.find((e) => e.port === 3000)!;
/** What a poll hands over: a structurally new object, here with a new memory figure. */
const polled = (mem: number): PortEntry => ({ ...structuredClone(base), process: { ...base.process!, memory_bytes: mem } });
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
