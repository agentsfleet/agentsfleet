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
export const AGENT_PREFIX = "AGENT";

/** Shown where the fleet behind a historical row no longer exists. */
export const DELETED_AGENT_LABEL = "DELETED AGENT";

/** Between the derived callsign and the operator's own name for the fleet. */
export const LABEL_SEPARATOR = " · ";

/**
 * The agent's name as text, for sort keys, aria-labels, titles and senders.
 *
 * Two independent facts, and the label shows whichever it has. The **callsign**
 * is DERIVED from `fleetId` — a pure hash, never stored, so there is nothing to
 * drift. The **name** is the operator's own, which no function can derive, so
 * it is the one thing a charge row carries a copy of.
 *
 * Callers with no name to give pass nothing and get exactly today's label. The
 * approvals and events tables are that case; only billing stores a name.
 *
 * The four states, and none of them is blank:
 *
 * | `fleetId` | `fleetName` | Label |
 * |---|---|---|
 * | set | set | `AGENT NOVA · deploy-bot` |
 * | set | absent | `AGENT NOVA` — a live fleet, or a charge predating the name |
 * | absent | set | `deploy-bot` |
 * | absent | absent | `DELETED AGENT` |
 *
 * The last row is unrecoverable rather than unhandled: a fleet purged before
 * slot 915 had its identifier nulled by a foreign key, and nothing can bring it
 * back. It renders the deleted label rather than an empty cell, because an
 * empty cell reads as a rendering bug against a charge that is perfectly real.
 */
export function agentDisplayName(
  fleetId: string | null,
  fleetName?: string | null,
): string {
  // Trimmed before testing: a name that is present but blank would otherwise
  // render as an empty label, which is the one outcome this function exists to
  // rule out.
  const name = fleetName?.trim();
  const callsign =
    fleetId === null
      ? null
      : `${AGENT_PREFIX} ${deriveFleetIdentity(fleetId).callsign.toUpperCase()}`;

  if (callsign !== null && name) return `${callsign}${LABEL_SEPARATOR}${name}`;
  if (callsign !== null) return callsign;
  if (name) return name;
  return DELETED_AGENT_LABEL;
}
