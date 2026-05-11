import { describe, expect, it } from "vitest";
import { SUPPORT_EMAIL } from "./contact";

describe("SUPPORT_EMAIL pinned (regression — mirror src/config/contact_test.zig)", () => {
  it("resolves to usezombie@agentmail.to", () => {
    // pin test: literal is the contract
    expect(SUPPORT_EMAIL).toBe("usezombie@agentmail.to");
  });
});
