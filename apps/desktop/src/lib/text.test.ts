import { describe, expect, it } from "vitest";
import { codeSpans } from "./text";

describe("text helpers", () => {
  it("splits backtick code spans", () => {
    expect(codeSpans("run `sudo holdmap stop 631`.")).toEqual([{ code: false, text: "run " }, { code: true, text: "sudo holdmap stop 631" }, { code: false, text: "." }]);
    expect(codeSpans("plain")).toEqual([{ code: false, text: "plain" }]);
  });
});
