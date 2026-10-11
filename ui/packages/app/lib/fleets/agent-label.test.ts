import { describe, expect, it } from "vitest";

import { chargeAgentLabel } from "@/app/(dashboard)/settings/billing/lib/charges";
import { AGENT_PREFIX, DELETED_AGENT_LABEL, agentDisplayName } from "./agent-label";
import { deriveFleetIdentity } from "./identity";

const FLEET_ID = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee";

function charge(fleetId: string) {
  return {
    id: "chg_1",
    fleet_id: fleetId,
    charge_type: "run",
    debit_nanos: 0,
    recorded_at: 0,
  } as unknown as Parameters<typeof chargeAgentLabel>[0];
}

describe("the agent label has one composer", () => {
  it("billing and the domain label compose one string", () => {
    expect(chargeAgentLabel(charge(FLEET_ID))).toBe(agentDisplayName(FLEET_ID));
    expect(agentDisplayName(FLEET_ID)).toBe(
      `${AGENT_PREFIX} ${deriveFleetIdentity(FLEET_ID).callsign.toUpperCase()}`,
    );
  });

  // These arms used to be proven through `chargeAgentLabel`, with a charge
  // carrying no `fleet_id`. Slot 916 made that column NOT NULL, so a charge
  // can no longer reach them and the proofs moved here — to the signature that
  // still can. `AgentLabel` takes `fleetId: string | null` as its own prop, so
  // a lease, an approval or an event row still arrives without one.
  it("shows the operator's own name when there is no identifier to derive from", () => {
    expect(agentDisplayName(null, "deploy-bot")).toBe("deploy-bot");
  });

  it("never renders a blank label, whatever shape the missing name takes", () => {
    for (const fleetName of [null, undefined, "", "   "]) {
      expect(agentDisplayName(null, fleetName)).toBe(DELETED_AGENT_LABEL);
    }
  });
});
