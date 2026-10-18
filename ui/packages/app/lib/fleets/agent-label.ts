import { deriveFleetIdentity } from "@/lib/fleets/identity";

export const AGENT_PREFIX = "AGENT";
export const DELETED_AGENT_LABEL = "DELETED AGENT";

/** The stable fleet identity for navigation, activity, billing and sort keys. */
export function agentDisplayName(fleetId: string | null): string {
  return fleetId === null
    ? DELETED_AGENT_LABEL
    : `${AGENT_PREFIX} ${deriveFleetIdentity(fleetId).callsign.toUpperCase()}`;
}
