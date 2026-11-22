import { expect, it } from "vitest";
import { DELETED_AGENT_LABEL, agentDisplayName } from "./agent-label";

const FLEET_ID = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee";

it("derives a stable agent callsign from the retained fleet identifier", () => {
  expect(agentDisplayName(FLEET_ID)).toBe("AGENT LUMEN-8453");
});

it("keeps an unavailable identity explicit", () => {
  expect(agentDisplayName(null)).toBe(DELETED_AGENT_LABEL);
});
