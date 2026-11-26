import { Badge } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { FleetDetail } from "@/lib/types";
import ExhaustionBadge from "@/components/domain/usage/ExhaustionBadge";
import { FleetActions } from "./FleetActions";
import { FleetHeaderStatus } from "@/components/domain/conversation/chrome/FleetHeaderStatus";
import { NewFleetShortcut } from "@/components/layout/ResourceHeader";
import { FleetRunCost } from "@/components/domain/activity/replies/FleetRunCost";
import type { RunFigures } from "@/lib/turns/run-summary";

export function FleetHeader({
  workspaceId,
  fleet,
  exhaustedAt,
  canGrant = false,
  latestRun,
}: {
  workspaceId: string;
  fleet: FleetDetail;
  exhaustedAt?: string | null;
  canGrant?: boolean;
  latestRun?: RunFigures | null;
}) {
  return (
    <header className="fleet-chat-heading flex min-h-16 shrink-0 items-center justify-between gap-lg border-b border-border-subtle px-lg summary-medium:px-xl summary-wide:px-2xl">
      <div className="flex min-w-0 items-center gap-md summary-medium:gap-lg">
        <FleetActions workspaceId={workspaceId} fleet={fleet} canGrant={canGrant} />
        {fleet.status !== AGENTSFLEET_STATUS.ARCHIVED ? <FleetHeaderStatus workspaceId={workspaceId} fleetId={fleet.id} initial={fleet.pending_approvals} schedules={fleet.schedules} latestRun={latestRun} /> : null}
        {fleet.status === AGENTSFLEET_STATUS.ACTIVE ? null : fleet.status === AGENTSFLEET_STATUS.ARCHIVED ? (
          <span aria-label="Fleet status: archived" className="hidden items-center gap-md text-eyebrow text-muted-foreground summary:inline-flex"><span aria-hidden="true" className="size-1.5 rounded-full bg-current" />Archived</span>
        ) : <Badge aria-label={`Fleet status: ${fleet.status}`}>{fleet.status}</Badge>}
      </div>
      <div className="flex shrink-0 items-center gap-5">
        <FleetRunCost workspaceId={workspaceId} fleetId={fleet.id} initial={latestRun} />
        {exhaustedAt !== undefined ? <ExhaustionBadge exhaustedAt={exhaustedAt} /> : null}
        <NewFleetShortcut workspaceId={workspaceId} />
      </div>
    </header>
  );
}
