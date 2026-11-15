import { EYEBROW_CLASS, cn } from "@agentsfleet/design-system";
import { agentDisplayName } from "@/lib/fleets/agent-label";

export function AgentLabel({ fleetId, className }: { fleetId: string | null; className?: string }) {
  const name = agentDisplayName(fleetId);
  return <span className={cn(EYEBROW_CLASS, className)} data-agent-name={name}>{name}</span>;
}
