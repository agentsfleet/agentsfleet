import { runtimeModelLabel } from "@/lib/api/draft-list-types";
import { fleetSources } from "@/lib/fleet-sources";
import { claims, requireCredential } from "@/lib/auth/credential";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { notFound, redirect } from "next/navigation";
import { fleetSetupPath, workspacePath } from "@/lib/workspace-routes";
import { ApiError } from "@/lib/api/errors";
import { getFleet } from "@/lib/api/fleets";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { FleetSetupWorkspace } from "@/components/domain/setup/FleetSetupWorkspace";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { startChatData, type ChatViewData } from "./components/view-data";
import { ChatView } from "./components/ChatView";
import { FleetHeader } from "./components/FleetHeader";
import { buildRunSummary } from "@/lib/events/run-summary";
import { FleetRecovery } from "./components/FleetRecovery";
import { FleetViewedTracker } from "./components/FleetViewedTracker";
import { namedMembers } from "@/lib/events/sender-names";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { FleetPendingChat } from "./components/FleetPendingChat";
import type { FleetDetail } from "@/lib/types";
import { setupRequirements } from "@/lib/setup-requirements";
import { SetupSummary } from "@/components/domain/setup/summary/SetupSummary";

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
  const viewer = typeof sessionClaims?.sub === "string" ? sessionClaims.sub : null;
  const {content, latestRun} = pendingSetup
    ? {content: <FleetPendingChat account={account} workspace={workspaceId} fleet={fleet} canGrant={canGrant} />, latestRun: null}
    : await loadChatView(workspaceId, fleet, viewData, viewer);
  return <div data-fleet-workspace className="flex h-full min-h-0 flex-1 flex-col overflow-clip">
    <FleetViewedTracker fleetId={fleet.id} status={fleet.status} />
    <FleetSetupWorkspace account={account} fleet={fleet.id} revision={fleet.setup_revision}
      model={typeof fleet.definition.config["x-agentsfleet"].model === "string" ? fleet.definition.config["x-agentsfleet"].model : "platform-default"}
      workspace={workspaceId} viewer={viewer}
      header={pendingSetup ? null : <FleetHeader workspaceId={workspaceId} fleet={fleet} canGrant={canGrant} latestRun={latestRun}
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
  data: ChatViewData, viewer: string | null,
) {
  // The transcript is the one surface that genuinely wants the bodies: it
  // renders what was said. The thread read carries them in ONE request — the
  // list-then-one-detail-per-turn fan-out this view used to issue is gone. The
  // strip's first figures come off that same page, and its pending count off
  // the fleet detail the page already holds: the chat opens on two reads, and
  // the live tail moves both from there.
  const [threadResult, members, models] = await Promise.all([data.thread, data.members, data.models]);
  const turns = threadResult?.items ?? [];
  const initialSummary = buildRunSummary(fleet.status, threadResult, fleet.pending_approvals);
  const approvalsHref = `${workspacePath(workspaceId, `fleets/${fleet.id}`)}#fleet-approvals`;
  const modelLabel = runtimeModelLabel(models, fleet.definition.config["x-agentsfleet"].model);
  const scheduled = fleet.status !== AGENTSFLEET_STATUS.ARCHIVED && fleet.schedules.some(schedule => schedule.status === "active" || schedule.status === "paused");
  return {
    latestRun: initialSummary.latest,
    content: <ChatView
      workspaceId={workspaceId}
      fleetId={fleet.id}
      schedules={fleet.schedules}
      sources={fleetSources(fleet.definition.config["x-agentsfleet"])}
      senderLabel={fleet.name}
      setupRevision={fleet.setup_revision}
      setup={<FleetRecovery key={`${workspaceId}:${fleet.id}:${fleet.etag}`} workspace={workspaceId} fleet={fleet} showSaved />}
      setupSummary={scheduled ? <SetupSummary requirements={setupRequirements(fleet.definition) ?? undefined} schedules={fleet.schedules}
        modelLabel={modelLabel}
        ready editHref={fleetSetupPath(workspaceId, fleet.id, fleet.setup_revision)} /> : undefined}
      modelLabel={modelLabel}
      initial={turns}
      initialSummary={initialSummary}
      approvalsHref={approvalsHref}
      viewer={viewer}
      senderNames={namedMembers(members ?? [])}
    />,
  };
}

