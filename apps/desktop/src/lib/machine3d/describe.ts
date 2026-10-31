// How a 3D part is known, in the shared evidence wording, for the inspector and the hover card.
import { capabilityChip, evidenceChip, type EvidenceChipText } from "../evidence";
import { PROVIDER_LABEL } from "../agent-flow/model";
import type { Part } from "./scene";

export function partChip(part: Part): EvidenceChipText {
  if (part.kind === "mcp") return part.live ? { tone: "seen", label: "Running now", detail: "Configured, and one of this agent's own processes runs it. That still doesn't show which tools were called." } : capabilityChip("mcp");
  if (part.kind === "skill") return capabilityChip("skill");
  if (part.kind === "instructions") return { tone: "configured", label: "Files found", detail: "Only names, descriptions and rule decisions are read. That a file exists doesn't show it was used." };
  if (part.kind === "dormant") return { tone: "configured", label: "Configuration found", detail: "No process of this agent is running; only its default configuration files were read." };
  if (part.kind === "tool" && part.variant === "mcp_server") return capabilityChip("running");
  if (part.kind === "call") return { tone: "inferred", label: "Reported", detail: `Reported by an agent integration (${Object.values(PROVIDER_LABEL).join(", ")}) and recorded locally. It isn't matched to a process or a configured server.` };
  return evidenceChip(part.evidence === "configured" || part.evidence === "reported" ? "unknown" : part.evidence);
}
