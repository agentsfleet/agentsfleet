import { test } from "bun:test";
import assert from "node:assert/strict";
import { SUPPORT_EMAIL } from "../src/lib/contact.ts";

// Pin test — bumping SUPPORT_EMAIL must be a coordinated change across
// every copy (website TS + app TS + agentsfleet TS + Mintlify snippet).
// The literal IS what this pins.
test("SUPPORT_EMAIL pinned to agentsfleet@agentmail.to", () => {
  // pin test: the literal is the pinned value
  assert.equal(SUPPORT_EMAIL, "agentsfleet@agentmail.to");
});
