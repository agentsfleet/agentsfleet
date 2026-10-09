import { describe, expect, it } from "vitest";
import { SUPPORT_EMAIL } from "./contact";

describe("SUPPORT_EMAIL pinned (regression — mirror cli/test/contact.unit.test.ts)", () => {
  it("resolves to agentsfleet@agentmail.to", () => {
    // pin test: literal is the contract
    expect(SUPPORT_EMAIL).toBe("agentsfleet@agentmail.to");
  });
});
