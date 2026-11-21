import { describe, expect, it } from "vitest";
import { partDestination, partJump } from "./agent-jump";

describe("3D part destinations", () => {
  it("sends each part to the screen that part belongs to", () => {
    expect(partJump("agent")).toBe("overview");
    expect(partJump("project")).toBe("overview");
    expect(partJump("port")).toBe("overview");
    expect(partJump("mcp")).toBe("capabilities");
    expect(partJump("skill")).toBe("capabilities");
    expect(partJump("instructions")).toBe("capabilities");
    expect(partJump("dormant")).toBe("capabilities");
    expect(partJump("tool")).toBe("flow");
    expect(partJump("tool", "mcp_server")).toBe("capabilities");
    expect(partJump("tool", "dev_server")).toBe("flow");
    expect(partJump("more", "MCP servers")).toBe("capabilities");
    expect(partJump("more", "skills")).toBe("capabilities");
    expect(partJump("more", "processes")).toBe("overview");
    expect(partJump("remote")).toBe("flow");
    expect(partJump("call")).toBe("calls");
  });

  it("names one next step, and that step is the screen the part opens", () => {
    const server = partDestination("mcp");
    expect(server).toMatchObject({ jump: "capabilities", label: "Open this server" });
    expect(partDestination("skill").label).toBe("Open this skill");
    expect(partDestination("tool", "mcp_server").label).toBe("Open this process");
    expect(partDestination("tool", "dev_server")).toMatchObject({ jump: "flow", label: "Open in Flow" });
    expect(partDestination("remote").jump).toBe("flow");
    expect(partDestination("port").label).toBe("Open overview");
    expect(partDestination("call")).toMatchObject({ jump: "calls", label: "Recorded calls" });
    expect(partDestination("agent").jump).toBe(partJump("agent"));
  });
});
