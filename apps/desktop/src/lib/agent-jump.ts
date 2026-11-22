// Where opening an agent lands. Home cards and the Agents map both call this; the screen
// that follows is Overview unless the caller names another door.
export type AgentJump = "overview" | "capabilities" | "flow" | "calls" | "insights" | "findings" | "results" | "risks" | "skills";
