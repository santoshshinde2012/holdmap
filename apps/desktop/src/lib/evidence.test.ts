import { describe, expect, it } from "vitest";
import { capabilityChip, evidenceChip } from "./evidence";

describe("evidence chips", () => {
  it("labels each evidence level with a caveat", () => {
    expect(evidenceChip("observed")).toMatchObject({ tone: "seen", label: "Observed" });
    expect(evidenceChip("inferred").tone).toBe("inferred");
    expect(evidenceChip("unknown").label).toBe("Unknown");
  });

  it("keeps capability caveats in one place", () => {
    expect(capabilityChip("skill").detail).toContain("does not prove");
    expect(capabilityChip("mcp").detail).toContain("Configured does not mean connected");
    expect(capabilityChip("running").label).toBe("Runtime observation");
  });
});
