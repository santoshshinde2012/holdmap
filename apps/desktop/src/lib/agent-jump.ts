// Where a 3D part leads. The picture is the start; a second click opens the screen that part belongs to.
import type { PartKind } from "./machine3d/scene";

export type AgentJump = "overview" | "capabilities" | "flow" | "calls" | "insights" | "findings" | "results" | "risks" | "skills";

/** The doors on a selected agent. Recorded calls are a separate door, only on a call. */
export const AGENT_DOORS: { jump: AgentJump; label: string; icon: string; title: string }[] = [
  { jump: "overview", label: "Overview", icon: "bot", title: "This agent's resources, access and processes" },
  { jump: "capabilities", label: "MCP & skills", icon: "plug", title: "MCP servers and skills this agent can load" },
  { jump: "flow", label: "Flow", icon: "branch", title: "Connections this agent is part of" },
  { jump: "results", label: "Results", icon: "list", title: "Analyzer results" },
  { jump: "findings", label: "Security", icon: "shield", title: "Open findings" },
];

/** The one next step for this part: a verb, where it lands, and a short reason. */
export function partDestination(kind: PartKind, variant = ""): { jump: AgentJump; label: string; hint: string; icon: string } {
  if (kind === "call") return { jump: "calls", label: "Recorded calls", hint: "Not tied to one agent", icon: "activity" };
  const jump = partJump(kind, variant);
  const door = AGENT_DOORS.find((d) => d.jump === jump)!;
  const tuned: Partial<Record<PartKind, { label: string; hint: string }>> = {
    agent: { label: "Open overview", hint: "Resources, access and processes" },
    project: { label: "Open overview", hint: "This agent's working folders" },
    port: { label: "Open overview", hint: "The agent that holds this port" },
    mcp: { label: "Open this server", hint: "Where this MCP server is defined" },
    skill: { label: "Open this skill", hint: "The skill file and what it declares" },
    instructions: { label: "Open MCP & skills", hint: "Rules and instruction files" },
    dormant: { label: "Open MCP & skills", hint: "Configuration found on disk" },
    remote: { label: "Open in Flow", hint: "Connections to these hosts" },
    tool: variant === "mcp_server"
      ? { label: "Open this process", hint: "The running process, separate from its config" }
      : { label: "Open in Flow", hint: "Connections this process is part of" },
    more: variant === "skills"
      ? { label: "Open MCP & skills", hint: "Every skill, including ones not drawn" }
      : variant === "MCP servers"
        ? { label: "Open MCP & skills", hint: "Every server, including ones not drawn" }
        : { label: "Open overview", hint: "Everything this agent has" },
  };
  const copy = tuned[kind] ?? { label: door.label, hint: door.title };
  return { jump, label: copy.label, hint: copy.hint, icon: door.icon };
}

/** The screen a second click on this part opens. `variant` distinguishes a process that is an MCP server. */
export function partJump(kind: PartKind, variant = ""): AgentJump {
  if (kind === "tool" && variant === "mcp_server") return "capabilities";
  if (kind === "more" && (variant === "MCP servers" || variant === "skills")) return "capabilities";
  switch (kind) {
    case "mcp":
    case "skill":
    case "instructions":
    case "dormant":
      return "capabilities";
    case "remote":
    case "tool":
      return "flow";
    case "call":
      return "calls";
    default:
      return "overview";
  }
}
