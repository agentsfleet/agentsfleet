import { EYEBROW_CLASS, cn } from "@agentsfleet/design-system";

import { deriveFleetIdentity } from "@/app/(dashboard)/w/[workspaceId]/fleets/components/fleetIdentity";

/**
 * One fleet, spelled the same way on every surface that names one.
 *
 * The callsign itself is mixed case (`Orly-6056`) because `data-agent-name`,
 * aria-labels and sort keys all want it that way; the UPPERCASE is typography,
 * carried by `EYEBROW_CLASS`. That split is why the Fleets tile read
 * `AGENT ORLY-6056` while Billing read `Agent Orly-6056` — each render site
 * decided its own casing, and one of them forgot. Rendering through this
 * component is what makes the decision once.
 */
export const AGENT_PREFIX = "Agent";

/** Shown where the fleet behind a historical row no longer exists. */
export const DELETED_AGENT_LABEL = "Deleted agent";

/** The agent's name as text, for sort keys, aria-labels and titles. */
export function agentDisplayName(fleetId: string | null): string {
  if (fleetId === null) return DELETED_AGENT_LABEL;
  return `${AGENT_PREFIX} ${deriveFleetIdentity(fleetId).callsign}`;
}

export function AgentLabel({
  fleetId,
  className,
}: {
  fleetId: string | null;
  className?: string;
}) {
  const name = agentDisplayName(fleetId);
  return (
    <span className={cn(EYEBROW_CLASS, className)} data-agent-name={name}>
      {name}
    </span>
  );
}
