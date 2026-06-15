import { describe, expect, it } from "vitest";
import { SUPPORT_EMAIL } from "./contact";

describe("SUPPORT_EMAIL pinned (regression — mirror src/config/contact_test.zig)", () => {
  it("resolves to agentsfleet@agentmail.to", () => {
    // pin test: literal is the contract
    expect(SUPPORT_EMAIL).toBe("agentsfleet@agentmail.to");
  });
});
