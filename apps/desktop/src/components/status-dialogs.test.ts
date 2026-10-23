// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import ReviewPendingDialog from "./ReviewPendingDialog.svelte";
import SettingsUnavailableDialog from "./SettingsUnavailableDialog.svelte";

beforeAll(() => {
  window.matchMedia ??= ((media: string) => ({ matches: true, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} }));
});
afterEach(cleanup);

describe("SettingsUnavailableDialog", () => {
  it("says why settings can't open and offers a retry or a way out", async () => {
    const [onretry, onclose] = [vi.fn(), vi.fn()];
    const view = render(SettingsUnavailableDialog, { error: "the settings file is unreadable", loading: false, onretry, onclose });
    expect(view.getByRole("dialog", { name: "Settings unavailable" })).toBeTruthy();
    expect(view.getByRole("alert").textContent).toBe("the settings file is unreadable");
    await fireEvent.click(view.getByRole("button", { name: "Try again" }));
    await fireEvent.click(view.getAllByRole("button", { name: "Close" })[0]);
    expect([onretry.mock.calls.length, onclose.mock.calls.length]).toEqual([1, 1]);
  });
  it("shows a status, not an alert, while the settings are still loading", () => {
    const view = render(SettingsUnavailableDialog, { error: null, loading: true, onretry: () => {}, onclose: () => {} });
    expect(view.getByRole("status").textContent).toBe("Loading your settings…");
    expect(view.queryByRole("alert")).toBeNull();
  });
});

describe("ReviewPendingDialog", () => {
  it("names what is being reviewed and can always be cancelled", async () => {
    const oncancel = vi.fn();
    const view = render(ReviewPendingDialog, { subject: ":3000", oncancel });
    expect(view.getByRole("dialog", { name: "Reviewing :3000…" })).toBeTruthy();
    expect(view.getByRole("status").getAttribute("aria-busy")).toBe("true");
    await fireEvent.click(view.getByRole("button", { name: "Cancel" }));
    expect(oncancel).toHaveBeenCalledOnce();
  });
});
