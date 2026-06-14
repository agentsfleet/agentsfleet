import { test } from "bun:test";
import assert from "node:assert/strict";
import { SUPPORT_EMAIL } from "../src/lib/contact.ts";

// Pin test — bumping SUPPORT_EMAIL must be a coordinated change across
// every runtime (Zig + website TS + app TS + agentsfleet TS + Mintlify
// snippet). The literal IS the contract here.
test("SUPPORT_EMAIL pinned to usezombie@agentmail.to", () => {
  // pin test: literal is the contract
  assert.equal(SUPPORT_EMAIL, "usezombie@agentmail.to");
});
