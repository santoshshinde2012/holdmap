// @vitest-environment jsdom
// The details pane under live polling: a fresh entry object with new figures must update the
// text in place, never remount the panel, flash the skeleton or scroll back to the top.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { tick } from "svelte";
import DetailPane from "./DetailPane.svelte";
import { MOCK_SNAPSHOT, mockExplain } from "../../lib/mock";
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
