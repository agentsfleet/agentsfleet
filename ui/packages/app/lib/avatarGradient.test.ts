import { describe, it, expect } from "vitest";
import { avatarGradient, AVATAR_GRADIENT_FALLBACK_SEED } from "./avatarGradient";

describe("avatarGradient", () => {
  it("is deterministic — the same seed always returns the identical string", () => {
    expect(avatarGradient("user_123")).toBe(avatarGradient("user_123"));
  });

  it("produces visibly different patterns for different seeds", () => {
    expect(avatarGradient("user_123")).not.toBe(avatarGradient("user_456"));
  });

  it("returns a repeating-conic-gradient with two hsl stops", () => {
    const result = avatarGradient("user_123");
    expect(result).toMatch(/^repeating-conic-gradient\(from \d+deg, hsl\(\d+, 65%, 45%\) 0deg 45deg, hsl\(\d+, 70%, 38%\) 45deg 90deg\)$/);
  });

  it("falls back to a stable non-empty seed for an empty string", () => {
    expect(avatarGradient("")).toBe(avatarGradient(AVATAR_GRADIENT_FALLBACK_SEED));
  });

  it("never uses the reserved --pulse currency color", () => {
    // The pulse hue (#5eead4) sits at hue ~174 on the HSL wheel; this test
    // just guards that the function emits hsl(...) values, never the raw
    // --pulse/--pulse-dim custom-property references.
    expect(avatarGradient("user_123")).not.toContain("--pulse");
  });
});
