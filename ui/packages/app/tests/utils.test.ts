import { describe, expect, it } from "vitest";

import { truncate } from "../lib/utils";

// `cn` moved to @agentsfleet/design-system (single workspace declaration);
// its merge behavior is covered by the design-system utils suite.

describe("app utils", () => {
  it("truncates only when string exceeds max length", () => {
    expect(truncate("fleet", 12)).toBe("fleet");
    expect(truncate("fleet-control-plane", 5)).toBe("fleet…");
  });
});
