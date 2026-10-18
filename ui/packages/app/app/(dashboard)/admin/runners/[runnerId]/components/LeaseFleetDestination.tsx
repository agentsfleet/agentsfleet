import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import { LEASE_DESTINATION, type LeaseDestination } from "@/lib/api/runners-types";
import { workspacePath } from "@/lib/workspace-routes";
import { OPEN_FLEET_LABEL } from "./runner-copy";

type Props = {workspaceId: string; fleetId: string; destination: LeaseDestination};

export function LeaseFleetDestination({workspaceId, fleetId, destination}: Props) {
  if (destination !== LEASE_DESTINATION.available) return <p className="text-sm text-muted-foreground">
    {destination === LEASE_DESTINATION.fleetDeleted
      ? "This fleet was deleted. Its lease history is retained." : "This fleet belongs to another account."}
  </p>;
  return <Button asChild variant="outline" className="w-full sm:w-auto">
    <Link href={workspacePath(workspaceId, `fleets/${encodeURIComponent(fleetId)}`)}>{OPEN_FLEET_LABEL} →</Link>
  </Button>;
}
