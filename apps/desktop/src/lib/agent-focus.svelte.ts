// The capability a 3D part asked to open. The MCP & skills workspace reads it once it is on screen
// (the workspace loads on demand) and then clears it, so a later visit does not reopen the sidebar.
export const agentFocus = $state<{ capabilityId: string | null; kind: "mcp" | "skill" | "running" | null }>({
  capabilityId: null,
  kind: null,
});
