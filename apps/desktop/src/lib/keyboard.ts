/** Keyboard policy shared by global shortcuts and text-based command surfaces. */
type CompositionEvent = Pick<KeyboardEvent, "isComposing" | "keyCode">;

/** WebKit can identify the final IME key with legacy code 229 after isComposing clears. */
export function isComposingKey(event: CompositionEvent): boolean {
  return event.isComposing || event.keyCode === 229;
}

function editing(element: Element | null): boolean {
  if (!element) return false;
  if (element.closest("input, textarea, [role=combobox], [role=textbox], [role=searchbox]")) return true;
  // The nearest declaration wins: contenteditable=false opts out of an editable ancestor.
  const declaration = element.closest("[contenteditable]")?.getAttribute("contenteditable")?.toLowerCase();
  return declaration === "" || declaration === "true" || declaration === "plaintext-only";
}

const CONTROLS = "select, button, a[href], summary, [role=radiogroup], [role=radio], [role=tablist], [role=tab], [role=separator], [role=slider], [role=spinbutton], [role=checkbox], [role=switch], [role=menuitem]";

/** Derive context without changing focus, state or the event. Native controls keep their keys. */
export function shortcutContext(event: KeyboardEvent, focused: Element | null) {
  const target = event.target instanceof Element ? event.target : null;
  const altGraph = event.getModifierState("AltGraph");
  const blocked = event.defaultPrevented || isComposingKey(event) || event.altKey || altGraph;
  const modified = event.ctrlKey || event.metaKey;
  return {
    blocked,
    editing: editing(focused) || editing(target),
    control: !!focused?.closest(CONTROLS) || !!target?.closest(CONTROLS),
    /** Exact Cmd/Ctrl combinations; AltGr and modified variants are never shortcuts. */
    command: !blocked && modified && !(event.ctrlKey && event.metaKey) && !event.shiftKey,
    /** Shift is used by explicit character shortcuts (e.g. ? and Shift+A). */
    character: !blocked && !modified,
    navigation: !blocked && !modified && !event.shiftKey,
  };
}
