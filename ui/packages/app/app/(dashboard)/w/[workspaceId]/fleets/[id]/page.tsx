import { claims, requireCredential } from "@/lib/auth/credential";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { notFound, redirect } from "next/navigation";
import { workspacePath } from "@/lib/workspace-routes";
import { ApiError } from "@/lib/api/errors";
import { getFleet } from "@/lib/api/fleets";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { FleetSetupWorkspace } from "@/components/domain/chat/FleetSetupWorkspace";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { startChatData, runtimeModelLabel, type ChatViewData } from "./components/view-data";
import { ChatView } from "./components/ChatView";
import { FleetHeader } from "./components/FleetHeader";
import { buildRunSummary } from "@/lib/events/run-summary";
import { FleetRecovery } from "./components/FleetRecovery";
import { FleetViewedTracker } from "./components/FleetViewedTracker";
import { namedMembers } from "@/lib/events/sender-names";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { FleetPendingChat } from "./components/FleetPendingChat";
import type { FleetDetail } from "@/lib/types";

export const dynamic = "force-dynamic";

export default async function FleetDetailPage({
  params,
  searchParams,
}: {
  params: Promise<{ workspaceId: string; id: string }>;
  searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
  const { workspaceId, id } = await params;
  const query: Record<string, string | string[] | undefined> = searchParams
    ? await searchParams
    : {};
  const token = await requireCredential();

  if (query.view && query.view !== "chat") redirect(workspacePath(workspaceId, `fleets/${id}`));
  const viewData = startChatData({workspaceId, fleetId: id, token});
  const [fleetResult, billing, workspaces, sessionClaims, canGrant] = await Promise.all([
    loadFleet(workspaceId, id, token),
    getTenantBillingCached(token).catch(() => null),
    listTenantWorkspacesCached(token).catch(() => null),
    claims(),
    hasScope(SCOPE.FLEET_WRITE),
  ]);
  if (!fleetResult) notFound();

  const {fleet} = fleetResult;
  const account = workspaces?.items.find(item => item.id === workspaceId)?.account.tenant_id ?? null;
  const pendingSetup = fleet.status === AGENTSFLEET_STATUS.INSTALLING;
  const content = pendingSetup ? <FleetPendingChat account={account} workspace={workspaceId} fleet={fleet} canGrant={canGrant} />
    : await loadChatView(workspaceId, fleet, viewData);
  return <div data-fleet-workspace className="flex h-full min-h-0 flex-1 flex-col overflow-clip">
    <FleetViewedTracker fleetId={fleet.id} status={fleet.status} />
    <FleetSetupWorkspace account={account} fleet={fleet.id} revision={fleet.setup_revision}
      model={typeof fleet.definition.config["x-agentsfleet"].model === "string" ? fleet.definition.config["x-agentsfleet"].model : "platform-default"}
      workspace={workspaceId} viewer={typeof sessionClaims?.sub === "string" ? sessionClaims.sub : null}
      header={pendingSetup ? null : <FleetHeader workspaceId={workspaceId} fleet={fleet} canGrant={canGrant}
        exhaustedAt={billing?.is_exhausted ? billing.exhausted_at : undefined} />}>
      <div className="page-entrance flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-clip">{content}</div>
    </FleetSetupWorkspace>
  </div>;
}

async function loadFleet(workspaceId: string, id: string, token: string) {
  return getFleet(workspaceId, id, token).catch((error: unknown) => {
    if (error instanceof ApiError && (error.status === 400 || error.status === 404)) return null;
    throw error;
  });
}

async function loadChatView(
  workspaceId: string, fleet: FleetDetail,
  data: ChatViewData,
) {
  // The transcript is the one surface that genuinely wants the bodies: it
  // renders what was said. The thread read carries them in ONE request — the
  // list-then-one-detail-per-turn fan-out this view used to issue is gone. The
  // strip's first figures come off that same page, and its pending count off
  // the fleet detail the page already holds: the chat opens on two reads, and
  // the live tail moves both from there.
  const [threadResult, members, models] = await Promise.all([data.thread, data.members, data.models]);
  const turns = threadResult?.items ?? [];
  // Who is signed in, from the verified claims this request already holds:
  // the thread keys its unsent messages by it before the client knows.
  const subject = (await claims())?.sub;
  const approvalsHref = `${workspacePath(workspaceId, "approvals")}?fleetId=${encodeURIComponent(fleet.id)}`;
  return (
    <ChatView
      workspaceId={workspaceId}
      fleetId={fleet.id}
      senderLabel={fleet.name}
      setupRevision={fleet.setup_revision}
      setup={<FleetRecovery key={`${workspaceId}:${fleet.id}:${fleet.etag}`} workspace={workspaceId} fleet={fleet} showSaved />}
      modelLabel={runtimeModelLabel(models, fleet.definition.config["x-agentsfleet"].model)}
      initial={turns}
      initialSummary={buildRunSummary(fleet.status, threadResult, fleet.pending_approvals)}
      approvalsHref={approvalsHref}
      viewer={typeof subject === "string" ? subject : null}
      senderNames={namedMembers(members ?? [])}
    />
  );
}

