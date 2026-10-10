import { describe, it, expect } from "vitest";
import { HERO_HEADLINE, HERO_LEDE_PARTS } from "./lib/marketing-copy";

/*
 * Guards the approved positioning copy. The copy policy (retired framing,
 * unvalidated claims, the retired noun) is a lint: scripts/check_website_copy.py.
 *
 * Test names follow RULE TST-NAM (no milestone IDs in test names).
 */

// The literal pin of the headline and the wake-on-event lede phrase: the
// other suites import those constants, so a revert of either would pass them.
describe("marketing hero — approved wording pinned once", () => {
  it("pins the approved headline and the wake-on-event lede phrase", () => {
    expect(HERO_HEADLINE).toBe("AI agents that wake when production breaks.");
    // pin test: literal is the contract
    expect(HERO_LEDE_PARTS.trigger).toBe("wakes on a production event");
  });
});
