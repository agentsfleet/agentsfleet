import { describe, expect, it } from "vitest";

import { formatDuration, truncate } from "../lib/utils";

// `cn` moved to @agentsfleet/design-system (single workspace declaration);
// its merge behavior is covered by the design-system utils suite.

describe("app utils", () => {
  it("formats short and minute durations", () => {
    expect(formatDuration(42)).toBe("42s");
    expect(formatDuration(121)).toBe("2m 1s");
    expect(formatDuration(120)).toBe("2m");
  });

  it("truncates only when string exceeds max length", () => {
    expect(truncate("fleet", 12)).toBe("fleet");
    expect(truncate("fleet-control-plane", 5)).toBe("fleet…");
  });
});
