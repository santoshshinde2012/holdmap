import { describe, expect, it } from "vitest";
import { noun, plural } from "./plural";

describe("plural", () => {
  it("uses the singular only for exactly one", () => {
    expect(plural(1, "service")).toBe("1 service");
    expect(plural(0, "service")).toBe("0 services");
    expect(plural(2, "group")).toBe("2 groups");
    expect(plural(1, "connected neighbour")).toBe("1 connected neighbour");
    expect(noun(3, "process", "processes")).toBe("processes");
  });
});
