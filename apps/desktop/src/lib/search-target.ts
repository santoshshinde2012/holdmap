// Which on-screen results the title-bar search filters, so its aria-controls always resolves.
import type { View } from "../components/shell/ViewToolbar.svelte";
import type { AgentSection } from "./app-prefs.svelte";

export function searchControls(view: View, section: AgentSection): string {
  if (view === "home") return "home-overview";
  if (view === "graph") return "service-graph";
  if (view === "list") return "port-list";
  if (section === "capabilities") return "agent-capabilities";
  return section === "flow" ? "agents-flow" : "agents-overview";
}
