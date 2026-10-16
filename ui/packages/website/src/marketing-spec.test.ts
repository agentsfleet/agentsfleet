import { describe, it, expect } from "vitest";
import { HERO_HEADLINE, HERO_LEDE_PARTS } from "./lib/marketing-copy";

/*
 * Guards the approved positioning copy.
 *
 * Test names follow RULE TST-NAM (no milestone IDs in test names).
 */

// The literal pin of the headline and the wake-on-event lede phrase: the
// other suites import those constants, so a revert of either would pass them.
describe("marketing hero — approved wording pinned once", () => {
  it("pins the approved headline and the wake-on-event lede phrase", () => {
    expect(HERO_HEADLINE).toBe("What should your fleet handle?");
    // pin test: literal is the contract
    expect(HERO_LEDE_PARTS.trigger).toBe("wakes on a production event");
  });
});
