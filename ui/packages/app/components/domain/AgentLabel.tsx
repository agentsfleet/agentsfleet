import { EYEBROW_CLASS, cn } from "@agentsfleet/design-system";

import { agentDisplayName } from "@/lib/fleets/agent-label";

/**
 * One fleet, spelled the same way on every surface that names one.
 *
 * The callsign itself is mixed case (`Orly-6056`) because `data-agent-name`,
 * aria-labels and sort keys all want it that way; the UPPERCASE is typography,
 * carried by `EYEBROW_CLASS`. That split is why the Fleets tile read
 * `AGENT ORLY-6056` while Billing read `Agent Orly-6056` — each render site
 * decided its own casing, and one of them forgot. Rendering through this
 * component is what makes the decision once; the text itself is composed in
 * `lib/fleets/agent-label.ts`, where billing and the sort keys read it too.
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
