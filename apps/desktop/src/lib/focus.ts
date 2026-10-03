// Focus management shared by every modal surface (Dialog, Sheet, palette): a Tab trap, initial
// focus and focus restoration, so keyboard users never land behind an overlay.

export const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type=hidden])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
  "[contenteditable='true']",
].join(",");

/** Tabbable descendants of `root`, in DOM order. */
export function focusables(root: ParentNode): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) => !el.closest("[inert]") && el.getAttribute("aria-hidden") !== "true" && !el.hidden,
  );
}

/** Keep Tab / Shift+Tab inside `root`. Returns true when it moved focus. */
export function trapTab(e: KeyboardEvent, root: HTMLElement): boolean {
  if (e.key !== "Tab") return false;
  const items = focusables(root);
  if (!items.length) { e.preventDefault(); root.focus(); return true; }
  const first = items[0], last = items[items.length - 1];
  const active = document.activeElement as HTMLElement | null;
  const outside = !active || !root.contains(active) || active === root;
  if (e.shiftKey && (active === first || outside)) { last.focus(); e.preventDefault(); return true; }
  if (!e.shiftKey && (active === last || outside)) { first.focus(); e.preventDefault(); return true; }
  return false;
}

export interface TrapOptions {
  /** CSS selector (inside the node) to focus first, or "self" for the container itself (no visible ring, the
   *  next Tab enters the content); falls back to [data-autofocus], then the first tabbable, then the node. */
  initial?: string;
  /** Restore focus to the previously focused element on destroy (default true). */
  restore?: boolean;
}

/** Svelte action: trap focus in `node` while it is mounted. */
export function focusTrap(node: HTMLElement, opts: TrapOptions = {}) {
  const previous = document.activeElement as HTMLElement | null;
  const pick = () =>
    (opts.initial === "self" && node) ||
    (opts.initial && opts.initial !== "self" && node.querySelector<HTMLElement>(opts.initial)) ||
    node.querySelector<HTMLElement>("[data-autofocus]") ||
    focusables(node)[0] ||
    node;
  queueMicrotask(() => { if (!node.contains(document.activeElement)) pick().focus(); });
  const onKey = (e: KeyboardEvent) => trapTab(e, node);
  node.addEventListener("keydown", onKey);
  return {
    destroy() {
      node.removeEventListener("keydown", onKey);
      if (opts.restore !== false && previous && document.contains(previous)) queueMicrotask(() => previous.focus());
    },
  };
}
