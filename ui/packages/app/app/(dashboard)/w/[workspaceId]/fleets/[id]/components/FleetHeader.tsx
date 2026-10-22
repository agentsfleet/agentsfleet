import Link from "next/link";
import { Badge, Nav, PageTitle } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { FleetDetail } from "@/lib/types";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import FleetConfig from "./FleetConfig";
import KillSwitch from "./KillSwitch";
import { BREADCRUMB_LABEL, FLEETS_CRUMB_LABEL } from "./console-copy";
import { agentDisplayName } from "@/lib/fleets/agent-label";
import { EditSetupButton } from "@/components/domain/chat/FleetSetupWorkspace";

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
      className="mb-md shrink-0 text-sm text-muted-foreground"
    >
      <Link
        href={workspacePath(workspaceId, "fleets")}
        className="hover:text-foreground"
      >
        {FLEETS_CRUMB_LABEL}
      </Link>
      <span className="sr-only"> / {fleetName}</span>
    </Nav>
  );
}

export function FleetHeader({
  workspaceId,
  fleet,
  exhaustedAt,
}: {
  workspaceId: string;
  fleet: FleetDetail;
  exhaustedAt?: string | null;
}) {
  return (
    <div className="mb-xl flex flex-col gap-lg sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <FleetBreadcrumb workspaceId={workspaceId} fleetName={agentDisplayName(fleet.id)} />
        <PageTitle className="break-words">{agentDisplayName(fleet.id)}</PageTitle>
        <p className="mt-sm text-body-sm text-muted-foreground">{fleet.definition.description}</p>
      </div>
      <div
        aria-label="Fleet lifecycle actions"
        className="flex flex-wrap items-center justify-end gap-sm"
      >
        {exhaustedAt !== undefined ? (
          <ExhaustionBadge exhaustedAt={exhaustedAt} />
        ) : null}
        {fleet.needs_re_review ? <Badge variant="amber">Access needs attention before resuming</Badge> : null}
        <EditSetupButton />
        {fleet.status === AGENTSFLEET_STATUS.INSTALLING ? (
          <Badge variant="cyan" aria-label="Fleet status: installing">
            Installing
          </Badge>
        ) : fleet.status === AGENTSFLEET_STATUS.KILLED ? (
          <FleetConfig
            workspaceId={workspaceId}
            fleetId={fleet.id}
            fleetName={agentDisplayName(fleet.id)}
            etag={fleet.etag}
          />
        ) : LIFECYCLE_ACTION_STATUSES.has(fleet.status) ? (
          <KillSwitch workspaceId={workspaceId} fleet={fleet} />
        ) : (
          <Badge aria-label={`Fleet status: ${fleet.status}`}>
            {fleet.status}
          </Badge>
        )}
      </div>
    </div>
  );
}
