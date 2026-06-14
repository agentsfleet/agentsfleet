import { describe, expect, it } from "vitest";
import { SUPPORT_EMAIL } from "./contact";

// Pin test — bumping SUPPORT_EMAIL must be a coordinated change across
// every runtime (Zig + website TS + app TS + agentsfleet JS + Mintlify
// snippet). The literal IS the contract here.
describe("SUPPORT_EMAIL pinned (regression — mirror src/config/contact_test.zig)", () => {
  it("resolves to usezombie@agentmail.to", () => {
    // pin test: literal is the contract
    expect(SUPPORT_EMAIL).toBe("usezombie@agentmail.to");
  });
});
