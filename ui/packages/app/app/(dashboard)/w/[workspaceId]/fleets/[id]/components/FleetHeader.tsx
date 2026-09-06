import Link from "next/link";
import { Badge } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import FleetConfig from "./FleetConfig";
import KillSwitch from "./KillSwitch";
import { BREADCRUMB_LABEL, FLEETS_CRUMB_LABEL } from "./console-copy";
import type { FleetDetail } from "@/lib/types";

// The console's header row: the way back to the wall on the left, the fleet's
// lifecycle controls on the right. Which control renders is decided here, from
// the server-rendered status — the client leaves below only paint the action.

const LIFECYCLE_ACTION_STATUSES = new Set<string>([
  AGENTSFLEET_STATUS.ACTIVE,
  AGENTSFLEET_STATUS.PAUSED,
  AGENTSFLEET_STATUS.STOPPED,
]);

function FleetBreadcrumb({
  workspaceId,
  fleetName,
}: {
  workspaceId: string;
  fleetName: string;
}) {
  return (
    <nav
      aria-label={BREADCRUMB_LABEL}
      className="mb-sm shrink-0 font-mono text-sm text-muted-foreground"
    >
      <Link
        href={workspacePath(workspaceId, "fleets")}
        className="hover:text-foreground"
      >
        {FLEETS_CRUMB_LABEL}
      </Link>
      <span aria-hidden="true"> / </span>
      <span className="text-foreground">{fleetName}</span>
    </nav>
  );
}

export function FleetHeader({
  workspaceId,
  fleet,
  exhaustedAt,
}: {
  workspaceId: string;
  fleet: FleetDetail;
  exhaustedAt?: number | null;
}) {
  const actionFleet = {
    id: fleet.id,
    name: fleet.name,
    status: fleet.status,
    created_at: fleet.created_at,
    updated_at: fleet.updated_at,
    triggers: fleet.triggers ?? undefined,
  };
  return (
    <div className="mb-lg flex flex-col gap-md sm:flex-row sm:items-center sm:justify-between">
      <h1 className="sr-only">{fleet.name}</h1>
      <FleetBreadcrumb workspaceId={workspaceId} fleetName={fleet.name} />
      <div
        aria-label="Fleet lifecycle actions"
        className="flex flex-wrap items-center justify-end gap-sm"
      >
        {exhaustedAt !== undefined ? (
          <ExhaustionBadge exhaustedAt={exhaustedAt} />
        ) : null}
        {fleet.status === AGENTSFLEET_STATUS.INSTALLING ? (
          <Badge variant="cyan" aria-label="Fleet status: installing">
            Installing
          </Badge>
        ) : fleet.status === AGENTSFLEET_STATUS.KILLED ? (
          <FleetConfig
            workspaceId={workspaceId}
            fleetId={fleet.id}
            fleetName={fleet.name}
          />
        ) : LIFECYCLE_ACTION_STATUSES.has(fleet.status) ? (
          <KillSwitch workspaceId={workspaceId} fleet={actionFleet} />
        ) : (
          <Badge aria-label={`Fleet status: ${fleet.status}`}>
            {fleet.status}
          </Badge>
        )}
      </div>
    </div>
  );
}
