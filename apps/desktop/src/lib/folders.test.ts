import { describe, expect, it } from "vitest";
import { currentFolderOwner, fileManagerLabel, resolveFolderTarget } from "./folders";
import { mockAgents } from "./mock";

const sharedPath = "/Users/dev/code/shop-web";

describe("folder target resolution", () => {
  it("uses the first actual owner in report order for a shared folder", () => {
    const report = mockAgents();
    expect(resolveFolderTarget(report, sharedPath)).toEqual({ agentId: report.agents[0].id, path: sharedPath });
    report.agents.reverse();
    expect(resolveFolderTarget(report, sharedPath)).toEqual({ agentId: "agent:52000", path: sharedPath });
  });

  it("prefers the selected owner of the exact shared path", () => {
    const report = mockAgents();
    expect(resolveFolderTarget(report, sharedPath, report.agents[1].id)).toEqual({ agentId: report.agents[1].id, path: sharedPath });
  });

  it.each([null, "missing-agent", "agent:48000"])("falls back to the first owner when preference %s cannot own the path", (preferredAgentId) => {
    const report = mockAgents();
    expect(resolveFolderTarget(report, sharedPath, preferredAgentId)).toEqual({ agentId: report.agents[0].id, path: sharedPath });
  });

  it("accepts a reported recent folder even when its evidence is inferred", () => {
    const report = mockAgents();
    report.agents = [report.agents[0]];
    expect(report.agents[0].folders.find((folder) => folder.path === sharedPath)?.evidence).toBe("inferred");
    expect(resolveFolderTarget(report, sharedPath)).toEqual({ agentId: report.agents[0].id, path: sharedPath });
  });

  it("does not infer folder ownership from process cwd, tool cwd or project roots", () => {
    const report = mockAgents();
    const agent = report.agents[1];
    agent.processes[0].cwd = "/process-only";
    agent.tools[0].cwd = "/tool-only";
    agent.folders[0].project = { ...agent.folders[0].project!, root: "/project-only" };
    for (const path of ["/process-only", "/tool-only", "/project-only", "/"]) {
      expect(resolveFolderTarget(report, path, agent.id)).toBeNull();
    }
  });

  it.each(["", "/Users/dev/code", `${sharedPath}/child`, `${sharedPath}/`, "/Users/dev/code/shop-Web", "/Users/dev/code/shop-web-other"])("rejects the unreported exact path %s", (path) => {
    expect(resolveFolderTarget(mockAgents(), path)).toBeNull();
  });

  it("has no target without a report or any reported owner", () => {
    expect(resolveFolderTarget(null, sharedPath)).toBeNull();
    expect(resolveFolderTarget({ ...mockAgents(), agents: [] }, sharedPath)).toBeNull();
  });
});

describe("current folder owner", () => {
  it("returns the selected owner from the current report", () => {
    const report = mockAgents();
    const target = resolveFolderTarget(report, sharedPath, report.agents[1].id)!;
    const refreshed = mockAgents();
    refreshed.agents[1].name = "Updated Cursor";
    expect(currentFolderOwner(refreshed, target)).toBe(refreshed.agents[1]);
  });

  it("does not retarget when the selected agent disappears and another owner remains", () => {
    const report = mockAgents();
    const target = resolveFolderTarget(report, sharedPath, report.agents[1].id)!;
    report.agents = [report.agents[0]];
    expect(resolveFolderTarget(report, sharedPath)).not.toBeNull();
    expect(currentFolderOwner(report, target)).toBeNull();
  });

  it("does not retarget when the selected agent no longer reports the folder", () => {
    const report = mockAgents();
    const target = resolveFolderTarget(report, sharedPath, report.agents[1].id)!;
    report.agents[1].folders = [];
    expect(resolveFolderTarget(report, sharedPath)).not.toBeNull();
    expect(currentFolderOwner(report, target)).toBeNull();
  });

  it("rejects missing reports, missing targets, unknown owners and mismatched paths", () => {
    const report = mockAgents();
    const target = resolveFolderTarget(report, sharedPath)!;
    expect(currentFolderOwner(null, target)).toBeNull();
    expect(currentFolderOwner(report, null)).toBeNull();
    expect(currentFolderOwner(report, { ...target, agentId: "missing-agent" })).toBeNull();
    expect(currentFolderOwner(report, { ...target, path: `${sharedPath}/child` })).toBeNull();
  });
});

describe("file manager labels", () => {
  it.each(["macos", "darwin", "Darwin"])("names Finder on %s", (platform) => {
    expect(fileManagerLabel(platform)).toBe("Open in Finder");
  });

  it.each(["win", "win32", "windows", "Windows"])("names Explorer on %s", (platform) => {
    expect(fileManagerLabel(platform)).toBe("Open in Explorer");
  });

  it.each([undefined, "linux", "unknown"])("uses a generic file manager label on %s", (platform) => {
    expect(fileManagerLabel(platform)).toBe("Open in file manager");
  });
});
