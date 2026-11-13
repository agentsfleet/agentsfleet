import { StackIcon } from "@phosphor-icons/react/ssr";
import { Badge, PageTitle } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { FleetDetail } from "@/lib/types";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import { FleetActions } from "./FleetActions";
import { MemoryMenu } from "./MemoryMenu";
import { EditSetupMenuItem } from "@/components/domain/chat/FleetSetupWorkspace";
import { FleetHeaderStatus } from "@/components/domain/FleetHeaderStatus";
import { NewFleetShortcut } from "@/components/layout/ResourceHeader";
import { FleetRunCost } from "@/components/domain/FleetRunCost";
import type { RunFigures } from "@/lib/events/run-summary";

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
        <div className="flex min-w-0 items-center gap-md summary-medium:gap-lg">
        <StackIcon data-app-header-icon="" size={16} className="hidden shrink-0 text-muted-foreground sm:block" aria-hidden="true" />
        <PageTitle data-app-header-title="" size="chat" className="truncate font-semibold">{fleet.name}</PageTitle></div>
        <FleetActions workspaceId={workspaceId} fleet={fleet}>
          {fleet.status !== AGENTSFLEET_STATUS.KILLED ? <>
            <EditSetupMenuItem workspace={workspaceId} fleet={fleet.id} revision={fleet.setup_revision} />
            <MemoryMenu workspaceId={workspaceId} fleetId={fleet.id} access={fleet.memory_access} canGrant={canGrant} />
          </> : null}
        </FleetActions>
        {fleet.status !== AGENTSFLEET_STATUS.KILLED ? <FleetHeaderStatus workspaceId={workspaceId} fleetId={fleet.id} initial={fleet.pending_approvals} schedules={fleet.schedules} latestRun={latestRun} /> : null}
        {fleet.status === AGENTSFLEET_STATUS.INSTALLING || fleet.status === AGENTSFLEET_STATUS.ACTIVE ? null : fleet.status === AGENTSFLEET_STATUS.KILLED ? (
          <span aria-label="Fleet status: archived" className="inline-flex items-center gap-md text-eyebrow text-muted-foreground"><span aria-hidden="true" className="size-1.5 rounded-full bg-current" />Archived</span>
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
