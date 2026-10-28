"use client";

import { useRouter } from "next/navigation";
import nextDynamic from "next/dynamic";
import { Alert, Button, Skeleton } from "@agentsfleet/design-system";
import type { FleetDetail } from "@/lib/types";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";
import { MemoryMenu } from "./MemoryMenu";

const WorkspaceChat = nextDynamic(() => import("@/components/domain/chat/WorkspaceChat").then(module => module.WorkspaceChat), {
  ssr: false,
  loading: () => <output className="min-h-0 w-full flex-1" aria-label="Loading setup chat"><Skeleton className="h-full w-full" /></output>,
});

export function FleetPendingChat({account, workspace, fleet, canGrant}: {
  account: string | null; workspace: string; fleet: FleetDetail; canGrant: boolean;
}) {
  const router = useRouter();
  if (!account) return <Alert variant="warning">The setup conversation could not be loaded.
    <Button variant="link" size="sm" onClick={() => router.refresh()}>Try again</Button>
  </Alert>;
  return <WorkspaceChat account={account} workspace={workspace} fleetPage
    source={{kind: SOURCE_KIND.fleet, fleet_id: fleet.id, setup_revision: fleet.setup_revision}}
    menuItems={<MemoryMenu workspaceId={workspace} fleetId={fleet.id} access={fleet.memory_access} canGrant={canGrant} />} />;
}
