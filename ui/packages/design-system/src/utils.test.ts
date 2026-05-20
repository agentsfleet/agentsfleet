import { describe, expect, it } from "vitest";
import { cn } from "./utils";

describe("cn", () => {
  it("joins plain string inputs with single spaces", () => {
    expect(cn("a", "b", "c")).toBe("a b c");
  });

  it("drops falsy inputs (false, null, undefined, empty string, 0)", () => {
    expect(cn("a", false, null, undefined, "", 0, "b")).toBe("a b");
  });

  it("keeps a conditionally-included class and drops the falsy branch", () => {
    const active = true;
    const disabled = false;
    expect(cn("base", active && "is-active", disabled && "is-disabled")).toBe(
      "base is-active",
    );
  });

  it("flattens a nested array of class values", () => {
    expect(cn("a", ["b", "c"], "d")).toBe("a b c d");
  });

  it("flattens deeply nested arrays and skips their falsy members", () => {
    expect(cn(["a", ["b", false, ["c", null]]], "d")).toBe("a b c d");
  });

  it("stringifies a numeric class value", () => {
    expect(cn("z", 42)).toBe("z 42");
  });

  it("returns an empty string when every input is falsy", () => {
    expect(cn(false, null, undefined, "", 0)).toBe("");
  });

  it("preserves duplicate/conflicting tokens verbatim (no tailwind-merge dedupe)", () => {
    // cn is a plain join — it does NOT resolve merge conflicts the way
    // tailwind-merge would; both px utilities survive in source order.
    expect(cn("px-2", "px-4")).toBe("px-2 px-4");
  });
});
