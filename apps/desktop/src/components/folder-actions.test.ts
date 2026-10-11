// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import FolderActions from "./FolderActions.svelte";

beforeAll(() => {
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} }));
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

const target = { agentId: "agent:52000", path: "/Users/dev/code/shop-web" };
const props = () => ({ target, ownerName: "Cursor", available: true, platform: "macos", demo: false, onreveal: vi.fn(async () => true), oneditor: vi.fn(async () => true), onclose: vi.fn() });
const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

describe("folder chooser", () => {
  it("shows the exact full path, focuses Close, and Escape cancels without opening", async () => {
    const p = props();
    render(FolderActions, { props: p });
    await tick();
    expect(screen.getByRole("dialog", { name: "Folder actions" })).toBeTruthy();
    expect(screen.getByText(target.path).tagName).toBe("CODE");
    expect(document.activeElement).toBe(button("Close"));
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(p.onclose).toHaveBeenCalledOnce();
    expect(p.oneditor).not.toHaveBeenCalled();
    expect(p.onreveal).not.toHaveBeenCalled();
  });

  it("passes only the chosen owner/path to either opener and closes on success", async () => {
    const p = props();
    render(FolderActions, { props: p });
    await fireEvent.click(button("Open in Finder"));
    expect(p.onreveal).toHaveBeenCalledExactlyOnceWith(target.agentId, target.path);
    expect(p.onclose).toHaveBeenCalledOnce();
    expect(p.oneditor).not.toHaveBeenCalled();
  });

  it("blocks duplicate and competing opens until the callback settles", async () => {
    let resolve!: (result: boolean) => void;
    const p = { ...props(), oneditor: vi.fn(() => new Promise<boolean>((done) => (resolve = done))) };
    render(FolderActions, { props: p });
    button("Open in editor").focus();
    await fireEvent.click(button("Open in editor"));
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab" });
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
    expect(button("Open in editor").disabled).toBe(true);
    expect(button("Open in Finder").disabled).toBe(true);
    await fireEvent.click(button("Open in editor"));
    await fireEvent.click(button("Open in Finder"));
    expect(p.oneditor).toHaveBeenCalledExactlyOnceWith(target.agentId, target.path);
    expect(p.onreveal).not.toHaveBeenCalled();
    resolve(false);
    await waitFor(() => expect(button("Open in editor").disabled).toBe(false));
    expect(p.onclose).not.toHaveBeenCalled();
    expect(screen.getByRole("alert").textContent).toContain("Could not open this folder");
  });

  it("keeps a false result or native exception in the chooser without unhandled rejection", async () => {
    const p = { ...props(), oneditor: vi.fn(async () => { throw new Error('Permission denied <img src=x onerror="alert(1)">'); }) };
    const { container } = render(FolderActions, { props: p });
    await fireEvent.click(button("Open in editor"));
    expect(screen.getByRole("alert").textContent).toContain("Permission denied <img");
    expect(container.querySelector("img")).toBeNull();
    expect(p.onclose).not.toHaveBeenCalled();
  });

  it("disables openings when the current owner/path disappears while leaving clipboard usable", async () => {
    const writeText = vi.fn(async () => {});
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const p = props();
    const { rerender } = render(FolderActions, { props: p });
    button("Open in Finder").focus();
    await rerender({ ...p, available: false });
    expect(document.activeElement).toBe(screen.getByRole("dialog"));
    expect(screen.getByText(/no longer in the current scan/)).toBeTruthy();
    expect(button("Open in Finder").disabled).toBe(true);
    expect(button("Open in editor").disabled).toBe(true);
    expect(button("Copy path").disabled).toBe(false);
    await fireEvent.click(button("Open in Finder"));
    await fireEvent.click(button("Open in editor"));
    expect(p.onreveal).not.toHaveBeenCalled();
    expect(p.oneditor).not.toHaveBeenCalled();
    await fireEvent.click(button("Copy path"));
    expect(writeText).toHaveBeenCalledExactlyOnceWith(target.path);
    expect(screen.getByRole("status").textContent).toBe("Path copied");
    expect(p.onclose).not.toHaveBeenCalled();
  });

  it("explains browser simulation and handles denied clipboard access locally", async () => {
    vi.stubGlobal("navigator", { clipboard: { writeText: vi.fn(async () => { throw new Error("Clipboard permission denied"); }) } });
    render(FolderActions, { props: { ...props(), demo: true, platform: "browser" } });
    expect(screen.getByText(/opening folders and editors is simulated/)).toBeTruthy();
    expect(button("Open in file manager")).toBeTruthy();
    await fireEvent.click(button("Copy path"));
    expect(screen.getByRole("alert").textContent).toContain("Clipboard permission denied");
    expect(button("Copy path").disabled).toBe(false);
  });
});
