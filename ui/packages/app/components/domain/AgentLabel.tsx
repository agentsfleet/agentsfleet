import { EYEBROW_CLASS, cn } from "@agentsfleet/design-system";

import { agentDisplayName } from "@/lib/fleets/agent-label";

/**
 * One fleet, spelled the same way on every surface that names one.
 *
 * The shared display name is uppercase on every surface, including labels,
 * sort keys and the data attribute. EYEBROW_CLASS supplies the visual rhythm.
 * The text itself is composed in `lib/fleets/agent-label.ts`.
 */
export function AgentLabel({
  fleetId,
  fleetName,
  className,
}: {
  fleetId: string | null;
  /**
   * The operator's own name for the fleet, where the caller has one stored.
   *
   * Optional, and its absence is today's behaviour exactly — the approvals and
   * events tables hold no name and pass nothing. Only a billing charge carries
   * a captured name, so only billing passes this.
   */
  fleetName?: string | null;
  className?: string;
}) {
  const name = agentDisplayName(fleetId, fleetName);
  return (
    <span className={cn(EYEBROW_CLASS, className)} data-agent-name={name}>
      {name}
    </span>
  );
}
