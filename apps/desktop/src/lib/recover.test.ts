import { describe, expect, it } from "vitest";
import { isBenignError } from "./recover";

describe("isBenignError", () => {
  it("ignores ResizeObserver loop notices", () => {
    expect(isBenignError("ResizeObserver loop completed with undelivered notifications.")).toBe(true);
    expect(isBenignError("ResizeObserver loop limit exceeded")).toBe(true);
  });
  it("keeps real errors", () => {
    expect(isBenignError("TypeError: x is undefined")).toBe(false);
    expect(isBenignError("")).toBe(false);
  });
});
