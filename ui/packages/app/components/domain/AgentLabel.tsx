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
