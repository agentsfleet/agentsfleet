import { expect, it } from "vitest";
import { AGENT_PREFIX, DELETED_AGENT_LABEL, agentDisplayName } from "./agent-label";
import { deriveFleetIdentity } from "./identity";

const FLEET_ID = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee";

it("derives a stable agent callsign from the retained fleet identifier", () => {
  expect(agentDisplayName(FLEET_ID)).toBe(
    `${AGENT_PREFIX} ${deriveFleetIdentity(FLEET_ID).callsign.toUpperCase()}`,
  );
});

it("keeps an unavailable identity explicit", () => {
  expect(agentDisplayName(null)).toBe(DELETED_AGENT_LABEL);
});
