// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import Notices from "./Notices.svelte";
import type { ScheduledAction } from "../../lib/schedules";

afterEach(cleanup);

const base = { error: null, retrying: false, showError: true, hasSnapshot: false, next: null, activeCount: 0, now: 1_000, demo: false, onretry: () => {}, onschedules: () => {} };
const due = { id: "s1", label: "Stop :3000", status: "scheduled", due_at_ms: 61_000 } as unknown as ScheduledAction;

describe("Notices", () => {
  it("shows nothing when the scan is fine and nothing is scheduled", () => {
    const view = render(Notices, base);
    expect(view.queryByRole("region")).toBeNull();
  });
  it("explains a failed scan, says whether the last good scan is still shown, and offers a retry", async () => {
    const onretry = vi.fn();
    const view = render(Notices, { ...base, error: "the helper is not responding", hasSnapshot: true, onretry });
    expect(view.getByRole("alert").textContent).toContain("the helper is not responding");
    expect(view.getByText(/Showing the last successful scan/)).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Try again" }));
    expect(onretry).toHaveBeenCalledOnce();
  });
  it("leaves the failure to the list's own page before the first scan", () => {
    const view = render(Notices, { ...base, error: "x", showError: false });
    expect(view.queryByRole("alert")).toBeNull();
  });
  it("summarises the next scheduled action and opens the schedules", async () => {
    const onschedules = vi.fn();
    const view = render(Notices, { ...base, next: due, activeCount: 2, demo: true, onschedules });
    const region = view.getByRole("region", { name: "Active scheduled actions" });
    expect(region.textContent).toContain("2 scheduled actions · demo");
    expect(region.textContent).toContain("Stop :3000");
    await fireEvent.click(view.getByRole("button", { name: "View / cancel" }));
    expect(onschedules).toHaveBeenCalledOnce();
  });
});
