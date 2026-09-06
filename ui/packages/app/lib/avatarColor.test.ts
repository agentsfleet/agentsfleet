import { describe, it, expect } from "vitest";
import { avatarColor, AVATAR_COLOR_FALLBACK_SEED } from "./avatarColor";

describe("avatarColor", () => {
  it("is deterministic — the same seed always returns the identical string", () => {
    expect(avatarColor("user_123")).toBe(avatarColor("user_123"));
  });

  it("produces visibly different colors for different seeds", () => {
    expect(avatarColor("user_123")).not.toBe(avatarColor("user_456"));
  });

  it("returns one flat color without a gradient", () => {
    const result = avatarColor("user_123");
    expect(result).toMatch(/^hsl\(\d+ var\(--avatar-saturation\) var\(--avatar-lightness\)\)$/);
    expect(result).not.toContain("gradient");
  });

  it("falls back to a stable non-empty seed for an empty string", () => {
    expect(avatarColor("")).toBe(avatarColor(AVATAR_COLOR_FALLBACK_SEED));
  });

  it("never uses the reserved --pulse currency color", () => {
    // The pulse hue (#5eead4) sits at hue ~174 on the HSL wheel; this test
    // just guards that the function emits hsl(...) values, never the raw
    // --pulse/--pulse-dim custom-property references.
    expect(avatarColor("user_123")).not.toContain("--pulse");
  });
});
