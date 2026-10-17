// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import FirstRun from "./FirstRun.svelte";

const props = { current: { notify: false, autostart: false, singleKeys: true, inventory: false }, canAutostart: true, tray: false, platform: "linux", shortcut: null, mod: "Ctrl" };

beforeAll(() => {
  window.matchMedia ??= ((media: string) => ({ matches: true, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  const animate = () => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} });
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(animate);
  Element.prototype.scrollIntoView ??= vi.fn();
});
afterEach(cleanup);

describe("FirstRun", () => {
  it("shows current settings and applies only what was chosen", async () => {
    const onapply = vi.fn();
    const view = render(FirstRun, { ...props, onapply, onskip: vi.fn() });
    expect(view.getByRole("switch", { name: /Notify me/, checked: false })).toBeTruthy();
    expect(view.getByRole("switch", { name: /Launch holdmap at login/, checked: false })).toBeTruthy();
    await fireEvent.click(view.getByRole("radio", { name: "Agents" }));
    await fireEvent.click(view.getByRole("switch", { name: /Notify me/ }));
    await fireEvent.click(view.getByRole("button", { name: "Start" }));
    expect(onapply).toHaveBeenCalledWith({ start: "agents", notify: true, autostart: false, singleKeys: true, inventory: false });
  });

  it("skips without applying anything", async () => {
    const onapply = vi.fn();
    const onskip = vi.fn();
    const view = render(FirstRun, { ...props, canAutostart: false, onapply, onskip });
    expect(view.queryByRole("switch", { name: /Launch holdmap at login/ })).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "Skip" }));
    expect(onskip).toHaveBeenCalledOnce();
    expect(onapply).not.toHaveBeenCalled();
  });
});
