import { deriveFleetIdentity } from "@/lib/fleets/identity";

/**
 * The one place an agent's display label is composed.
 *
 * Billing sorts by it, the approvals and events tables sort and render by it,
 * the fleet page's thread signs the fleet's messages with it. Each of those
 * once spelled `Agent ${callsign}` for itself, and the billing module reached
 * into a fleets component directory for a domain fact three areas consume —
 * which is why this lives under `lib/` beside the derivation it composes from.
 */
export const AGENT_PREFIX = "Agent";

/** Shown where the fleet behind a historical row no longer exists. */
export const DELETED_AGENT_LABEL = "Deleted agent";

/** The agent's name as text, for sort keys, aria-labels, titles and senders. */
export function agentDisplayName(fleetId: string | null): string {
  if (fleetId === null) return DELETED_AGENT_LABEL;
  return `${AGENT_PREFIX} ${deriveFleetIdentity(fleetId).callsign}`;
}
