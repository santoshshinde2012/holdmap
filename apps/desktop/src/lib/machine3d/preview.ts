// The picture inside a selected part's details: that part's own district, nothing else.
// A port, process or server stays with the agent that owns it, so the model still shows
// where it stands. A recorded call has no district and is drawn on its own.
import type { MachineScene } from "./scene";

export interface ComponentView {
  scene: MachineScene;
  /** The agent this picture belongs to. Null for a recorded call, which isn't one agent's. */
  focus: string | null;
}

/** The scene a details view draws for one selected part. Wires that leave the district are dropped. */
export function componentScene(scene: MachineScene, id: string): ComponentView | null {
  const part = scene.parts.find((p) => p.id === id);
  if (!part) return null;
  if (part.kind === "call") {
    return { focus: null, scene: { ...scene, agents: [], parts: [part], wires: [], omittedAgents: 0, calls: 1, signature: `preview:${part.id}` } };
  }
  const owner = part.kind === "agent" || part.kind === "dormant" ? part.id : part.owner;
  const agent = scene.agents.find((a) => a.id === owner);
  if (!agent) return null;
  const parts = scene.parts.filter((p) => p.id === owner || p.owner === owner);
  const ids = new Set(parts.map((p) => p.id));
  const wires = scene.wires.filter((w) => ids.has(w.from) && ids.has(w.to));
  return {
    focus: owner,
    scene: { ...scene, agents: [agent], parts, wires, omittedAgents: 0, calls: 0, signature: `preview:${owner}:${scene.signature}` },
  };
}
