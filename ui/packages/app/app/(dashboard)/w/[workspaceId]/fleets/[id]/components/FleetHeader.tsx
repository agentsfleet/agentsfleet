import Link from "next/link";
import { StackIcon, NotePencilIcon } from "@phosphor-icons/react/ssr";
import { Badge, Button, PageTitle } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import type { FleetDetail } from "@/lib/types";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import { FleetActions } from "./FleetActions";
import { MemoryMenu } from "./MemoryMenu";
import { EditSetupMenuItem } from "@/components/domain/chat/FleetSetupWorkspace";

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
    <header className="fleet-chat-heading flex min-h-16 shrink-0 items-center justify-between gap-lg border-b border-border-subtle px-lg sm:px-2xl">
      <div className="flex min-w-0 items-center gap-lg">
        <StackIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
        <PageTitle size="chat" className="truncate">{fleet.name}</PageTitle>
        <FleetActions workspaceId={workspaceId} fleet={fleet}>
          {fleet.status !== AGENTSFLEET_STATUS.KILLED ? <>
            <EditSetupMenuItem workspace={workspaceId} fleet={fleet.id} revision={fleet.setup_revision} />
            <MemoryMenu workspaceId={workspaceId} fleetId={fleet.id} access={fleet.memory_access} canGrant={canGrant} />
          </> : null}
        </FleetActions>
        {fleet.status === AGENTSFLEET_STATUS.INSTALLING || fleet.status === AGENTSFLEET_STATUS.ACTIVE ? null : fleet.status === AGENTSFLEET_STATUS.KILLED ? (
          <Badge aria-label="Fleet status: archived">Archived</Badge>
        ) : <Badge aria-label={`Fleet status: ${fleet.status}`}>{fleet.status}</Badge>}
      </div>
      <div className="flex shrink-0 items-center gap-md">
        {exhaustedAt !== undefined ? <ExhaustionBadge exhaustedAt={exhaustedAt} /> : null}
        <Button asChild variant="ghost" size="icon" className="rounded-sm" aria-label="Start a new fleet" title="New fleet">
          <Link href={`/w/${workspaceId}/fleets/new`}><NotePencilIcon size={16} aria-hidden="true" /></Link>
        </Button>
      </div>
    </header>
  );
}
