import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { chargeAgentLabel } from "@/app/(dashboard)/settings/billing/lib/charges";
import { AGENT_PREFIX, DELETED_AGENT_LABEL, agentDisplayName } from "./agent-label";
import { deriveFleetIdentity } from "./identity";

const FLEET_ID = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee";
const FLEETS_COMPONENTS_PATH = "fleets/components/";
const COMPOSERS = [
  "app/(dashboard)/settings/billing/lib/charges.ts",
  "components/domain/AgentLabel.tsx",
] as const;

function charge(fleetId: string | null) {
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
    // Dimension 5.2.
    expect(chargeAgentLabel(charge(FLEET_ID))).toBe(agentDisplayName(FLEET_ID));
    expect(agentDisplayName(FLEET_ID)).toBe(
      `${AGENT_PREFIX} ${deriveFleetIdentity(FLEET_ID).callsign}`,
    );
    expect(chargeAgentLabel(charge(null))).toBe(DELETED_AGENT_LABEL);
  });

  it("neither composer reaches into a fleets component directory for it", () => {
    for (const file of COMPOSERS) {
      const source = readFileSync(resolve(process.cwd(), file), "utf8");
      expect(source, file).not.toContain(FLEETS_COMPONENTS_PATH);
      expect(source, file).toContain("@/lib/fleets/agent-label");
    }
  });
});
