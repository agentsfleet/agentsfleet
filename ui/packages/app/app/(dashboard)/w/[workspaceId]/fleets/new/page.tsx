import { notFound } from "next/navigation";
import { PageLayout } from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { readWorkspaceModels } from "../[id]/components/view-data";
import { NewFleetChat } from "@/components/domain/fleet/NewFleetChat";

export const dynamic = "force-dynamic";

export default async function NewFleetPage({params}: {params: Promise<{workspaceId: string}>}) {
  const {workspaceId} = await params;
  const token = await requireCredential();
  const [{items}, models] = await Promise.all([
    listTenantWorkspacesCached(token),
    // The picker offers what the workspace can run; with none read, the first message uses the workspace's model.
    readWorkspaceModels(workspaceId, token).catch(() => []),
  ]);
  if (!items.some(item => item.id === workspaceId)) notFound();
  return <PageLayout fullHeight className="flex min-h-0 flex-1 flex-col">
    <NewFleetChat key={workspaceId} workspace={workspaceId} models={models} />
  </PageLayout>;
}
