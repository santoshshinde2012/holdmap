// Keyboard users must be able to scroll any region that scrolls (WCAG 2.1.1). This action makes
// an element focusable only while its content overflows and it has nothing focusable inside.

const FOCUSABLE = "a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex='-1'])";

/** Whether `el` scrolls and none of its descendants can take focus. */
export function needsScrollFocus(el: HTMLElement): boolean {
  const overflows = el.scrollHeight > el.clientHeight + 1 || el.scrollWidth > el.clientWidth + 1;
  return overflows && !el.querySelector(FOCUSABLE);
}

export function scrollFocus(el: HTMLElement) {
  const own = el.hasAttribute("tabindex");
  const update = () => {
    if (own) return;
    if (needsScrollFocus(el)) el.tabIndex = 0;
    else el.removeAttribute("tabindex");
  };
  update();
  const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(update) : null;
  ro?.observe(el);
  const mo = typeof MutationObserver !== "undefined" ? new MutationObserver(update) : null;
  mo?.observe(el, { childList: true, subtree: true });
  return { destroy: () => { ro?.disconnect(); mo?.disconnect(); } };
}
