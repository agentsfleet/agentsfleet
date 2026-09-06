import Link from "next/link";
import { Badge, Nav, PageTitle } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import type { FleetDetail } from "@/lib/types";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import FleetConfig from "./FleetConfig";
import KillSwitch from "./KillSwitch";
import { BREADCRUMB_LABEL, FLEETS_CRUMB_LABEL } from "./console-copy";

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
    <Nav
      aria-label={BREADCRUMB_LABEL}
      className="mb-sm shrink-0 text-sm text-muted-foreground"
    >
      <Link
        href={workspacePath(workspaceId, "fleets")}
        className="hover:text-foreground"
      >
        {FLEETS_CRUMB_LABEL}
      </Link>
      <span aria-hidden="true"> / </span>
      <span className="text-foreground">{fleetName}</span>
    </Nav>
  );
}

export default function FleetHeader({
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
      <PageTitle className="sr-only">{fleet.name}</PageTitle>
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
