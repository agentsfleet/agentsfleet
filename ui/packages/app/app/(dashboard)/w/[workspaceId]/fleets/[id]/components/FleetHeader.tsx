import { Badge, PageTitle } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { FleetDetail } from "@/lib/types";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import FleetConfig from "./FleetConfig";
import KillSwitch from "./KillSwitch";
import { EditSetupButton } from "@/components/domain/chat/FleetSetupWorkspace";

const LIFECYCLE_ACTION_STATUSES = new Set<string>([
  AGENTSFLEET_STATUS.ACTIVE,
  AGENTSFLEET_STATUS.PAUSED,
  AGENTSFLEET_STATUS.STOPPED,
]);

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
    <div className="fleet-chat-heading flex shrink-0 items-center justify-between gap-md px-xl pb-sm pt-5 sm:px-chat-inset">
      <div className="min-w-0">
        <PageTitle size="chat" className="truncate">{fleet.name}</PageTitle>
        <p className="mt-xs text-eyebrow text-muted-foreground">Saved v{fleet.setup_revision} · <span className="capitalize">{fleet.status}</span></p>
      </div>
      <div
        aria-label="Fleet lifecycle actions"
        className="flex shrink-0 items-center justify-end gap-sm"
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
            fleetName={fleet.name}
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
