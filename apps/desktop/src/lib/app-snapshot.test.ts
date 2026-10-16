// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Blob as NodeBlob } from "node:buffer";
import { captureAppSnapshot } from "./app-snapshot";

const render = vi.hoisted(() => ({ createContext: vi.fn(), domToBlob: vi.fn(), destroyContext: vi.fn() }));
vi.mock("modern-screenshot", () => render);

/** Envelope fixture for adapter boundary tests; real decoded pixels are checked in browser fixtures. */
function pngEnvelope(width: number, height: number): Blob {
  const bytes = new Uint8Array(45);
  bytes.set([137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82]);
  const view = new DataView(bytes.buffer);
  view.setUint32(16, width); view.setUint32(20, height);
  bytes.set([0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130], 33);
  return new Blob([bytes], { type: "image/png" });
}

function root(width = 1200, height = 800): HTMLElement {
  const element = document.createElement("div");
  element.style.setProperty("--bg", "#101115");
  element.getBoundingClientRect = () => ({ x: 0, y: 0, top: 0, left: 0, bottom: height, right: width, width, height, toJSON() {} });
  document.body.append(element);
  return element;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.stubGlobal("Blob", NodeBlob);
  vi.stubGlobal("fetch", vi.fn(async () => new Response(new Uint8Array([119, 79, 70, 50, 1, 2, 3, 4]))));
  render.createContext.mockImplementation(async (_element, options) => ({ options }));
  render.domToBlob.mockImplementation(async (context) => pngEnvelope(Math.floor(context.options.width * context.options.scale), Math.floor(context.options.height * context.options.scale)));
});
afterEach(() => { document.body.replaceChildren(); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("Holdmap view snapshots", () => {
  it("loads only its two bundled fonts and preserves viewport, SVG and scrolled-content options", async () => {
    const element = root();
    const result = await captureAppSnapshot(element, new AbortController().signal);
    expect(result.source).toBe("app");
    expect(result.width).toBe(1200); expect(result.height).toBe(800);
    expect(result.png.filename).toMatch(/^holdmap-snapshot-.*\.png$/);
    expect(fetch).toHaveBeenCalledTimes(2);
    for (const [url, options] of vi.mocked(fetch).mock.calls) {
      expect(new URL(String(url)).origin).toBe(new URL(document.baseURI).origin);
      expect(String(url)).toMatch(/\/(inter-variable|jetbrains-mono-variable)\.woff2/);
      expect(options).toMatchObject({ mode: "same-origin", credentials: "omit", redirect: "error", cache: "force-cache" });
      expect(options?.signal).toBeInstanceOf(AbortSignal);
    }
    const options = render.createContext.mock.calls[0][1];
    expect(options.font.cssText).toContain('font-family:"Inter Variable"');
    expect(options.font.cssText).toContain('font-family:"JetBrains Mono Variable"');
    expect(options.font.cssText).toContain("data:font/woff2;base64,d09GMgECAwQ=");
    expect(options.features).toMatchObject({ restoreScrollPosition: true, fixSvgXmlDecode: true });
    expect(options.workerNumber).toBe(0);
    expect(render.destroyContext).toHaveBeenCalledExactlyOnceWith(await render.createContext.mock.results[0].value);
  });

  it("bounds high-DPI output before allocating the renderer canvas", async () => {
    vi.stubGlobal("devicePixelRatio", 2);
    const result = await captureAppSnapshot(root(8000, 4500), new AbortController().signal);
    expect({ width: result.width, height: result.height }).toEqual({ width: 3840, height: 2160 });
    const options = render.createContext.mock.calls[0][1];
    expect(options.width * options.scale).toBe(3840);
    expect(options.height * options.scale).toBe(2160);
  });

  it("excludes capture overlays, hidden media helpers and frames while retaining inline SVG definitions", async () => {
    const element = root();
    element.innerHTML = '<div data-snapshot-ui><img src="https://private.invalid/image"></div><iframe></iframe><object></object><embed><video style="opacity:0"></video><svg xmlns="http://www.w3.org/2000/svg"><defs><marker id="arrow"></marker></defs><path marker-end="url(#arrow)"></path></svg>';
    render.domToBlob.mockImplementation(async ({ options }) => {
      for (const excluded of element.querySelectorAll("[data-snapshot-ui], [data-snapshot-ui] img, iframe, object, embed, video")) expect(options.filter(excluded)).toBe(false);
      for (const included of element.querySelectorAll("svg, defs, marker, path")) expect(options.filter(included)).toBe(true);
      const path = element.querySelector("path")!;
      path.style.markerEnd = `url("${document.baseURI}#arrow")`;
      options.onCloneEachNode(path);
      expect(path.style.markerEnd).toBe('url("#arrow")');
      await expect(options.fetchFn("https://private.invalid/image")).rejects.toThrow("cannot be included");
      return pngEnvelope(1200, 800);
    });
    await captureAppSnapshot(element, new AbortController().signal);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("rejects remote images and SVG resources before any asset request or rendering", async () => {
    for (const html of ['<img src="https://private.invalid/image">', '<img src="/other.svg">', '<img src="/icon.svg?unapproved=1">', '<svg><use href="https://private.invalid/icons.svg#icon"></use></svg>']) {
      const element = root(); element.innerHTML = html;
      await expect(captureAppSnapshot(element, new AbortController().signal)).rejects.toThrow(/cannot be included/);
      element.remove();
    }
    expect(fetch).not.toHaveBeenCalled();
    expect(render.createContext).not.toHaveBeenCalled();
  });

  it("embeds only the exact bundled brand image and bounds its response", async () => {
    const element = root(); element.innerHTML = '<img src="/icon.svg">';
    render.domToBlob.mockImplementation(async ({ options }) => {
      await expect(options.fetchFn(new URL("/icon.svg", document.baseURI).href)).resolves.toMatch(/^data:image\/svg\+xml;base64,/);
      await expect(options.fetchFn("/other.svg")).rejects.toThrow("cannot be included");
      return pngEnvelope(1200, 800);
    });
    await captureAppSnapshot(element, new AbortController().signal);
    expect(fetch).toHaveBeenCalledTimes(3);
    expect(vi.mocked(fetch).mock.calls.some(([url]) => new URL(String(url)).pathname === "/icon.svg")).toBe(true);
    vi.mocked(fetch).mockImplementation(async (url) => new Response(String(url).endsWith("/icon.svg") ? new Uint8Array(64 * 1024 + 1) : new Uint8Array([119, 79, 70, 50])));
    await expect(captureAppSnapshot(element, new AbortController().signal)).rejects.toThrow("asset is too large");
    expect(render.createContext).toHaveBeenCalledOnce();
  });

  it("rejects oversized streamed fonts even without a Content-Length", async () => {
    vi.mocked(fetch).mockImplementation(async () => new Response(new ReadableStream({ start(controller) { controller.enqueue(new Uint8Array(512 * 1024 + 1)); controller.close(); } })));
    await expect(captureAppSnapshot(root(), new AbortController().signal)).rejects.toThrow("asset is too large");
    expect(render.createContext).not.toHaveBeenCalled();
  });

  it("never renders a detached, empty, or already cancelled view", async () => {
    const detached = root(); detached.remove();
    await expect(captureAppSnapshot(detached, new AbortController().signal)).rejects.toThrow("no longer available");
    await expect(captureAppSnapshot(root(0, 0), new AbortController().signal)).rejects.toThrow("no readable image");
    const controller = new AbortController(); controller.abort();
    await expect(captureAppSnapshot(root(), controller.signal)).rejects.toMatchObject({ name: "AbortError" });
    expect(fetch).not.toHaveBeenCalled();
    expect(render.createContext).not.toHaveBeenCalled();
  });

  it("destroys a late context after cancellation and never publishes its PNG", async () => {
    const context = deferred<{ options: object }>();
    render.createContext.mockReturnValue(context.promise);
    const controller = new AbortController();
    const pending = captureAppSnapshot(root(), controller.signal);
    await vi.waitFor(() => expect(render.createContext).toHaveBeenCalledOnce());
    controller.abort();
    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    const late = { options: {} }; context.resolve(late);
    await vi.waitFor(() => expect(render.destroyContext).toHaveBeenCalledExactlyOnceWith(late));
    expect(render.domToBlob).not.toHaveBeenCalled();
  });

  it("cancels bounded font streams without creating a renderer or retaining work", async () => {
    const cancelled = vi.fn();
    vi.mocked(fetch).mockImplementation(async () => new Response(new ReadableStream({ cancel: cancelled })));
    const controller = new AbortController();
    const pending = captureAppSnapshot(root(), controller.signal);
    await vi.waitFor(() => expect(fetch).toHaveBeenCalledTimes(2));
    controller.abort();
    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    await vi.waitFor(() => expect(cancelled).toHaveBeenCalledTimes(2));
    expect(render.createContext).not.toHaveBeenCalled();
  });

  it("bounds a stalled render, destroys its context and ignores a late encoded result", async () => {
    vi.useFakeTimers();
    const encoded = deferred<Blob>();
    render.domToBlob.mockReturnValue(encoded.promise);
    const pending = captureAppSnapshot(root(), new AbortController().signal);
    const rejected = expect(pending).rejects.toThrow("took too long");
    await vi.advanceTimersByTimeAsync(1);
    expect(render.domToBlob).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(15_000);
    await rejected;
    expect(render.destroyContext).toHaveBeenCalledOnce();
    encoded.resolve(pngEnvelope(1200, 800));
    await vi.advanceTimersByTimeAsync(1);
    expect(render.destroyContext).toHaveBeenCalledOnce();
  });
});
