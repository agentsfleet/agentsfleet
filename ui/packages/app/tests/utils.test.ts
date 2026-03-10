import { describe, expect, it } from "vitest";

import { cn, formatDuration, truncate } from "../lib/utils";

describe("app utils", () => {
  it("merges tailwind classes deterministically", () => {
    expect(cn("px-2", "px-4", "text-sm")).toBe("px-4 text-sm");
  });

  it("formats short and minute durations", () => {
    expect(formatDuration(42)).toBe("42s");
    expect(formatDuration(121)).toBe("2m 1s");
    expect(formatDuration(120)).toBe("2m");
  });

  it("truncates only when string exceeds max length", () => {
    expect(truncate("zombie", 12)).toBe("zombie");
    expect(truncate("zombie-control-plane", 6)).toBe("zombie…");
  });
});
