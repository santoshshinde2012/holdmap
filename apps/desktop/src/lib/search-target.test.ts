import { describe, expect, it } from "vitest";
import { searchControls } from "./search-target";

describe("searchControls", () => {
  it("points at each view's results", () => {
    expect(searchControls("home", "overview")).toBe("home-overview");
    expect(searchControls("list", "flow")).toBe("port-list");
    expect(searchControls("graph", "capabilities")).toBe("service-graph");
    expect(searchControls("agents", "overview")).toBe("agents-overview");
    expect(searchControls("agents", "capabilities")).toBe("agent-capabilities");
  });
  it("follows Flow when it replaces the agent overview", () => {
    expect(searchControls("agents", "flow")).toBe("agents-flow");
  });
});
