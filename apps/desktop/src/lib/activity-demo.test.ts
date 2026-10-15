import { describe, expect, it } from "vitest";
import { demoActivity } from "./activity-demo";
import { activityRuns } from "./agent-flow/build";

describe("reported capability usage examples", () => {
  it("keeps the skill activation separate from tools and inventory/process identities", () => {
    const snapshot = demoActivity();
    const skill = snapshot.spans.find((span) => span.kind === "skill")!;
    expect(skill).toMatchObject({ name: "Skill", status: "unknown", start_at_ms: null, duration_ms: null, pid: null });
    const run = activityRuns(snapshot.spans).find((run) => run.spans.includes(skill))!;
    expect(run).toMatchObject({ skills: 1, calls: 0 });
    expect(snapshot.spans.find((span) => span.name === "mcp__filesystem__read_file")?.mcp_server).toBe("filesystem");
    expect(snapshot.limits.join(" ")).toContain("simulated");
    expect(snapshot.limits.join(" ")).toContain("do not match configured servers to processes");
  });
});
