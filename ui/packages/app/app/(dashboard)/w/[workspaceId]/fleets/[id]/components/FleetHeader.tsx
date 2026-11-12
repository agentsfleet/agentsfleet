import { StackIcon } from "@phosphor-icons/react/ssr";
import { Badge, PageTitle } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { FleetDetail } from "@/lib/types";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import { FleetActions } from "./FleetActions";
import { MemoryMenu } from "./MemoryMenu";
import { EditSetupButton, EditSetupMenuItem } from "@/components/domain/chat/FleetSetupWorkspace";
import { FleetApprovalStatus } from "@/components/domain/FleetApprovalStatus";

export function FleetHeader({
  workspaceId,
  fleet,
  exhaustedAt,
  canGrant = false,
}: {
  workspaceId: string;
  fleet: FleetDetail;
  exhaustedAt?: string | null;
  canGrant?: boolean;
}) {
  return (
    <header className="fleet-chat-heading flex min-h-16 shrink-0 items-center justify-between gap-lg border-b border-border-subtle px-lg summary-medium:px-xl summary-wide:px-2xl">
      <div className="flex min-w-0 items-center gap-lg">
        <StackIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
        <PageTitle size="chat" className="truncate font-title">{fleet.name}</PageTitle>
        <FleetActions workspaceId={workspaceId} fleet={fleet}>
          {fleet.status !== AGENTSFLEET_STATUS.KILLED ? <>
            <EditSetupMenuItem workspace={workspaceId} fleet={fleet.id} revision={fleet.setup_revision} />
            <MemoryMenu workspaceId={workspaceId} fleetId={fleet.id} access={fleet.memory_access} canGrant={canGrant} />
          </> : null}
        </FleetActions>
        {fleet.status !== AGENTSFLEET_STATUS.KILLED ? <FleetApprovalStatus workspaceId={workspaceId} fleetId={fleet.id} initial={fleet.pending_approvals} /> : null}
        {fleet.status === AGENTSFLEET_STATUS.INSTALLING || fleet.status === AGENTSFLEET_STATUS.ACTIVE ? null : fleet.status === AGENTSFLEET_STATUS.KILLED ? (
          <Badge aria-label="Fleet status: archived">Archived</Badge>
        ) : <Badge aria-label={`Fleet status: ${fleet.status}`}>{fleet.status}</Badge>}
      </div>
      <div className="flex shrink-0 items-center gap-md">
        {exhaustedAt !== undefined ? <ExhaustionBadge exhaustedAt={exhaustedAt} /> : null}
        {fleet.status !== AGENTSFLEET_STATUS.KILLED ? <EditSetupButton workspace={workspaceId} fleet={fleet.id} revision={fleet.setup_revision} appearance="header" /> : null}
      </div>
    </header>
  );
}
