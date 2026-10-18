// The "needs you" rules exist twice: in Rust (holdmap-core attention, used by the CLI, MCP and
// the desktop app) and here (the browser demo and instant first paint). Both are checked against
// one shared case: crates/holdmap-core/tests/fixtures/attention_case.json.
// Regenerate with HOLDMAP_BLESS=1 npx vitest run src/lib/attention-parity.test.ts
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { DemoAnalyzers } from "./analyzers-demo";
import { attention } from "./home";
import { mockAgents, mockDevServers } from "./mock";
import type { Insights } from "./analyzers";
import type { AgentsReport, PortEntry } from "./types";

const FIXTURE = resolve(__dirname, "../../../../crates/holdmap-core/tests/fixtures/attention_case.json");

/** The mock machine and demo analyzer findings, plus one root agent, one elevated agent and risky settings, so every rule fires. */
function makeCase(): { entries: PortEntry[]; report: AgentsReport; insights: Insights } {
  const entries = mockDevServers();
  const report = structuredClone(mockAgents(undefined, true));
  const [first, second] = report.agents;
  first.capabilities!.settings = [
    ...(first.capabilities!.settings ?? []),
    { key: "permissions.defaultMode", value: "bypassPermissions", scope: "user", source_path: "/Users/dev/.claude/settings.json" },
  ];
  const rootAgent = structuredClone(second);
  rootAgent.id = "agent:1"; rootAgent.name = "Root Agent"; rootAgent.pid = 1; rootAgent.ports = [];
  rootAgent.access = { ...rootAgent.access, root: true, user: "root", uid: 0, mine: false };
  const wide = structuredClone(second);
  wide.id = "agent:2"; wide.name = "Wide Agent"; wide.pid = 2; wide.ports = [];
  wide.access = { ...wide.access, facts: [{ topic: "approvals", level: "elevated", summary: "Auto-approves commands.", evidence: "observed" }] };
  report.agents.push(rootAgent, wide);
  return { entries, report, insights: new DemoAnalyzers().insights() };
}

describe("needs-you parity with Rust", () => {
  it("matches the shared case", () => {
    if (process.env.HOLDMAP_BLESS === "1" || !existsSync(FIXTURE)) {
      const c = makeCase();
      writeFileSync(FIXTURE, JSON.stringify({ ...c, expected: attention(c.entries, c.report, c.insights) }, null, 1) + "\n");
    }
    const { entries, report, insights, expected } = JSON.parse(readFileSync(FIXTURE, "utf8"));
    const items = attention(entries, report, insights);
    expect(items).toEqual(expected);
    expect(new Set(items.map((i) => i.rule))).toEqual(new Set(["exposed", "root", "elevated", "settings", "unpinned", "exposed-port", "analyzer"]));
  });
});
