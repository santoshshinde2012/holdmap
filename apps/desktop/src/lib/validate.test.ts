import { describe, expect, it } from "vitest";
import { validateHost, validateLabel, validatePort, validateRange } from "./validate";

describe("validation", () => {
  it("ports", () => {
    expect(validatePort("3000")).toBeNull();
    expect(validatePort(65535)).toBeNull();
    expect(validatePort("")).toMatch(/Enter/);
    expect(validatePort("0")).toMatch(/1 to 65535/);
    expect(validatePort("70000")).toMatch(/1 to 65535/);
    expect(validatePort("30a")).toMatch(/whole numbers/);
  });
  it("ranges and labels", () => {
    expect(validateRange(4, 1, 60)).toBeNull();
    expect(validateRange(null, 1, 60, " s")).toMatch(/1 to 60 s/);
    expect(validateRange(1.5, 1, 60)).toMatch(/whole/);
    expect(validateRange(61, 1, 60)).toMatch(/1–60/);
    expect(validateLabel("x".repeat(40))).toBeNull();
    expect(validateLabel("x".repeat(41))).toMatch(/40/);
  });
  it("hosts mirror the core validator (no ssh option injection)", () => {
    for (const ok of ["devbox", "user@10.0.0.5", "deploy@staging.internal:2222", "[::1]", "me@[fe80::1]:22", "my_host-1.local"]) expect(validateHost(ok), ok).toBeNull();
    for (const bad of ["", "-oProxyCommand=x", "a@-b", "host;rm -rf", "host:0", "host:99999", "host:22x", "[::1", "@host", "a b", "user@", "$(id)"]) expect(validateHost(bad), bad).not.toBeNull();
  });
});
