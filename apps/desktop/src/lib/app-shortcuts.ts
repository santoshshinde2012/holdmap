// App-wide keyboard shortcuts as a pure decision: given the key, where focus is and what the
// app shows, say what should happen. The app performs the intent; this file never touches state.
import type { View } from "../components/shell/ViewToolbar.svelte";
import type { shortcutContext } from "./keyboard";

export type KeyContext = ReturnType<typeof shortcutContext>;

export interface KeyState {
  view: View;
  singleKeys: boolean;
  /** A dialog or overlay owns the keyboard. */
  modalOpen: boolean;
  snapshotBusy: boolean;
  /** Agents is showing Flow, which has its own arrow keys. */
  flowView: boolean;
  focus: { inSearch: boolean; recordButton: boolean; snapshotButton: boolean; graphService: boolean };
  graph: { detailsOrSelection: boolean; query: boolean };
  agents: { query: boolean };
  list: { selected: boolean; filtersActive: boolean; hasRows: boolean };
  /** The listener in context (List, or Graph with its details open). */
  selection: { present: boolean; stoppable: boolean; openable: boolean };
  /** The selected graph service belongs to a cluster that can be stopped. */
  graphCluster: boolean;
}

export type KeyIntent =
  | { do: "cancel-snapshot" | "palette" | "settings" | "search" | "refresh" | "record" | "toggle-record" | "snapshot" }
  | { do: "graph-close-details" | "graph-clear-query" }
  | { do: "graph-open-selected"; blur: boolean }
  | { do: "graph-step"; delta: number }
  | { do: "list-collapse"; collapse: boolean }
  | { do: "list-edge"; end: boolean }
  | { do: "move"; delta: number }
  | { do: "blur-search" | "agents-clear-query" | "agents-deselect" | "clear-filters" | "list-deselect" }
  | { do: "agents-pick-from-search" | "list-pick-from-search" | "open-drawer" }
  | { do: "stop"; force: boolean }
  | { do: "open" | "copy-url" | "toggle-all-sockets" | "cycle-proto" | "toggle-dev" | "toggle-mine" | "toggle-exposed" | "cycle-theme" | "toggle-side" | "toggle-graph" | "toggle-home-layout" | "toggle-agents" | "history" | "pin" | "pin-label" | "stop-cluster" | "help" };

/** What a key press means, and whether the browser's default is suppressed. Null: let it through.
 * An intent the app finds nothing to do for (B with nothing selected) also lets the key through. */
export type KeyDecision = { intent: KeyIntent | null; prevent: boolean } | null;

const yes = (intent: KeyIntent): KeyDecision => ({ intent, prevent: true });

export function decideKey(key: string, shift: boolean, c: KeyContext, st: KeyState): KeyDecision {
  if (c.blocked) return null;
  if (st.snapshotBusy && c.navigation && key === "Escape") return yes({ do: "cancel-snapshot" });
  if (st.modalOpen) return null; // overlays handle their own keys
  const typing = c.editing;
  const plain = c.character && st.singleKeys;
  const k = key.toLowerCase();
  if (c.command && k === "k") return yes({ do: "palette" });
  if (c.command && key === ",") return yes({ do: "settings" });
  if ((c.command && k === "f") || (!typing && !c.control && plain && key === "/")) return yes({ do: "search" });
  if (c.command && k === "r") return yes({ do: "refresh" });
  if (!typing && plain && key === "V" && st.focus.recordButton) return yes({ do: "record" });
  if (!typing && plain && key === "S" && st.focus.snapshotButton) return yes({ do: "snapshot" });
  if (st.view === "graph" && !typing && c.navigation && key === "Escape") {
    if (st.graph.detailsOrSelection) return yes({ do: "graph-close-details" });
    if (st.graph.query) return yes({ do: "graph-clear-query" });
    return null;
  }
  const arrow = key === "ArrowDown" || key === "ArrowRight" || key === "ArrowUp" || key === "ArrowLeft";
  if (st.view === "graph" && c.navigation && st.focus.graphService && arrow) return yes({ do: "graph-step", delta: key === "ArrowDown" || key === "ArrowRight" ? 1 : -1 });
  if (c.control) return null; // native/select/button controls retain their own keyboard behaviour
  if (typing && !st.focus.inSearch) return null;
  const list = st.view === "list" && !typing && c.navigation;
  if (list && (key === "ArrowLeft" || key === "ArrowRight") && st.list.selected) return yes({ do: "list-collapse", collapse: key === "ArrowLeft" });
  if (list && (key === "Home" || key === "End") && st.list.hasRows) return yes({ do: "list-edge", end: key === "End" });
  if (list && (key === "PageDown" || key === "PageUp")) return yes({ do: "move", delta: key === "PageDown" ? 10 : -10 });
  if (!typing && !st.flowView) {
    if ((c.navigation && (key === "ArrowDown" || key === "ArrowRight" && st.view === "graph")) || (plain && key === "j")) return yes({ do: "move", delta: 1 });
    if ((c.navigation && (key === "ArrowUp" || key === "ArrowLeft" && st.view === "graph")) || (plain && key === "k")) return yes({ do: "move", delta: -1 });
  }
  if (c.navigation && key === "Escape") {
    if (typing) return yes({ do: "blur-search" });
    if (st.view === "agents") return yes({ do: st.agents.query ? "agents-clear-query" : "agents-deselect" });
    if (st.view === "graph") return yes({ do: "graph-clear-query" });
    return yes({ do: st.list.filtersActive ? "clear-filters" : "list-deselect" });
  }
  if (c.navigation && key === "Enter") {
    // In Agents, buttons and map nodes handle Enter outside search.
    if (st.view === "agents") return { intent: typing ? { do: "agents-pick-from-search" } : null, prevent: false };
    if (st.view === "graph") return yes({ do: "graph-open-selected", blur: typing });
    return { intent: typing ? { do: "list-pick-from-search" } : { do: "open-drawer" }, prevent: false };
  }
  if (typing || !c.character) return null;
  // Plain keys below; the list (or the page) has focus.
  const sel = st.selection;
  if (key === "Backspace" || key === "Delete") return sel.stoppable ? yes({ do: "stop", force: shift }) : null;
  if (!st.singleKeys) return null;
  const listOnly = (intent: KeyIntent) => (st.view === "list" ? yes(intent) : null);
  switch (key) {
    case "o": return sel.openable ? yes({ do: "open" }) : null;
    case "c": return sel.present ? yes({ do: "copy-url" }) : null;
    case "r": return yes({ do: "refresh" });
    case "a": return listOnly({ do: "toggle-all-sockets" });
    case "t": return listOnly({ do: "cycle-proto" });
    case "d": return listOnly({ do: "toggle-dev" });
    case "m": return listOnly({ do: "toggle-mine" });
    case "e": return listOnly({ do: "toggle-exposed" });
    case "L": return yes({ do: "cycle-theme" });
    case "b": return yes({ do: "toggle-side" });
    case "g": return yes({ do: "toggle-graph" });
    case "v": return st.view === "home" ? yes({ do: "toggle-home-layout" }) : null;
    case "A": return yes({ do: "toggle-agents" });
    case "h": return yes({ do: "history" });
    case "V": return yes({ do: "toggle-record" });
    case "S": return yes({ do: "snapshot" });
    case "p": return sel.present ? yes({ do: "pin" }) : null;
    case "P": return sel.present ? yes({ do: "pin-label" }) : null;
    case "s": return st.graphCluster ? yes({ do: "stop-cluster" }) : null;
    case "?": return yes({ do: "help" });
    default: return null;
  }
}
