import { describe, expect, it } from "vitest";
import { SUPPORT_EMAIL } from "./contact";

// Pin test — bumping SUPPORT_EMAIL must be a coordinated change across
// every surface that carries it (website TS + app TS + CLI TS + Mintlify
// snippet). The literal itself is what this pins.
describe("SUPPORT_EMAIL pinned (regression — same pin as the app and CLI)", () => {
  it("resolves to agentsfleet@agentmail.to", () => {
    // pin test: literal is the contract
    expect(SUPPORT_EMAIL).toBe("agentsfleet@agentmail.to");
  });
});
