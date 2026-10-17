// One place for evidence wording: a short label plus the caveat shown in an EvidenceChip tooltip,
// so views state a limit once instead of repeating disclaimers in every panel.
import type { Evidence } from "./types";

export type EvidenceTone = "seen" | "inferred" | "configured" | "unknown";
export interface EvidenceChipText { tone: EvidenceTone; label: string; detail: string }

const BY_EVIDENCE: Record<Evidence, EvidenceChipText> = {
  observed: { tone: "seen", label: "Observed", detail: "Read from the operating system during the last scan." },
  inferred: { tone: "inferred", label: "Inferred", detail: "Derived from process names, folders or parents; not directly reported by the OS." },
  unknown: { tone: "unknown", label: "Unknown", detail: "Not reported by the operating system or the agent." },
};

export const evidenceChip = (evidence: Evidence): EvidenceChipText => BY_EVIDENCE[evidence];

export type CapabilityKind = "skill" | "mcp" | "running";

/** The single caveat for an inventory item: what finding it does and does not establish. */
export function capabilityChip(kind: CapabilityKind): EvidenceChipText {
  if (kind === "skill") return { tone: "configured", label: "Skill file found", detail: "File presence does not prove the skill was loaded or activated. Only its declared name and description are read; instructions are not collected or executed." };
  if (kind === "mcp") return { tone: "configured", label: "MCP definition found", detail: "Configured does not mean connected, loaded or invoked. Commands, arguments, URLs, headers and secret values are not shown." };
  return { tone: "inferred", label: "Runtime observation", detail: "A running process classified as an MCP candidate. It is not matched to a configuration entry, and it is not proof of a connected server or an invocation." };
}
