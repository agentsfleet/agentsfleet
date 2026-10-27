import { Badge, PageTitle } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { FleetDetail } from "@/lib/types";
import { agentDisplayName } from "@/lib/fleets/agent-label";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import FleetConfig from "./FleetConfig";
import KillSwitch from "./KillSwitch";
import { EditSetupButton } from "@/components/domain/chat/FleetSetupWorkspace";
import { MemoryMenu } from "./MemoryMenu";

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
    <div className="fleet-chat-heading flex shrink-0 items-center justify-between gap-md px-xl pb-sm pt-5 sm:px-chat-inset">
      <div className="min-w-0">
        <PageTitle size="chat" className="truncate">{fleet.name}</PageTitle>
        <p className="mt-xs truncate text-eyebrow text-muted-foreground">{agentDisplayName(fleet.id)}</p>
      </div>
      <div
        aria-label="Fleet lifecycle actions"
        className="flex shrink-0 items-center justify-end gap-sm"
      >
        {exhaustedAt !== undefined ? (
          <ExhaustionBadge exhaustedAt={exhaustedAt} />
        ) : null}
        <EditSetupButton workspace={workspaceId} fleet={fleet.id} revision={fleet.setup_revision} />
        {fleet.status === AGENTSFLEET_STATUS.INSTALLING || fleet.status === AGENTSFLEET_STATUS.ACTIVE ? null : fleet.status === AGENTSFLEET_STATUS.KILLED ? (
          <FleetConfig
            workspaceId={workspaceId}
            fleetId={fleet.id}
            fleetName={fleet.name}
            etag={fleet.etag}
          />
        ) : (
          <Badge aria-label={`Fleet status: ${fleet.status}`}>
            {fleet.status}
          </Badge>
        )}
        <KillSwitch workspaceId={workspaceId} fleet={fleet}>
          <MemoryMenu workspaceId={workspaceId} fleetId={fleet.id} access={fleet.memory_access} canGrant={canGrant} />
        </KillSwitch>
      </div>
    </div>
  );
}
