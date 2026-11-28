import { claims, requireCredential } from "@/lib/auth/credential";
import { expandScopes, SCOPE } from "@/lib/auth/scopes";
import { notFound, redirect } from "next/navigation";
import { workspacePath } from "@/lib/workspace-routes";
import { ApiError } from "@/lib/api/errors";
import { getFleet } from "@/lib/api/fleets";
import { GITHUB_GRANT, GRANT_STATUS } from "@/lib/api/fleet-detail";
import { runtimeModelLabel, type WorkspaceModel } from "@/lib/api/workspace-models";
import { fleetSources } from "@/lib/fleet-sources";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { FleetWorkspace } from "@/components/domain/fleet/FleetWorkspace";
import { FleetListSections } from "@/components/domain/fleet/FleetListSections";
import { FleetNetworkSection } from "@/components/domain/network/FleetNetworkSection";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { startChatData, type ChatViewData } from "./components/view-data";
import { ChatView } from "./components/ChatView";
import { FleetHeader } from "./components/FleetHeader";
import { buildRunSummary } from "@/lib/turns/run-summary";
import { FleetViewedTracker } from "./components/FleetViewedTracker";
import { namedMembers } from "@/lib/turns/sender-names";
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
  const [fleetResult, billing, workspaces, sessionClaims] = await Promise.all([
    loadFleet(workspaceId, id, token),
    getTenantBillingCached(token).catch(() => null),
    listTenantWorkspacesCached(token).catch(() => null),
    claims(),
  ]);
  if (!fleetResult) notFound();

  const account = workspaces?.items.find(item => item.id === workspaceId)?.account.tenant_id ?? null;
  const viewer = typeof sessionClaims?.sub === "string" ? sessionClaims.sub : null;
  // afd_auth::scope::parse_claim separates a session token's scopes on literal spaces.
  const scopes = expandScopes(typeof sessionClaims?.scopes === "string" ? sessionClaims.scopes.split(" ") : []);
  const {schedules, triggers, grants, ...fields} = fleetResult.fleet;
  // The fleet detail includes these lists, but each list has its own read permission.
  const lists = {
    schedules: viewer && scopes.has(SCOPE.SCHEDULE_READ) ? schedules : undefined,
    triggers: viewer && scopes.has(SCOPE.FLEET_READ) ? triggers : undefined,
    grants: viewer && scopes.has(SCOPE.GRANT_READ) ? grants : undefined,
  };
  const fleet = {...fields, schedules: lists.schedules ?? [], triggers: lists.triggers ?? [], grants: lists.grants ?? []};
  const {content, latestRun, models} = await loadChatView(workspaceId, fleet, viewData, viewer);
  return <div data-fleet-workspace className="flex h-full min-h-0 flex-1 flex-col overflow-clip">
    <FleetViewedTracker fleetId={fleet.id} status={fleet.status} />
    <FleetWorkspace account={account} workspace={workspaceId} fleet={{...fields, ...lists}} models={models} viewer={viewer}>
      <FleetHeader workspaceId={workspaceId} fleet={fleet} canGrant={scopes.has(SCOPE.FLEET_WRITE)} latestRun={latestRun}
        exhaustedAt={billing?.is_exhausted ? billing.exhausted_at : undefined} />
      <div className="page-entrance flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-clip">{content}</div>
    </FleetWorkspace>
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
  // The repositories a run may reach are the ones a person approved for GitHub.
  const github = fleet.grants.find(grant => grant.name === GITHUB_GRANT && grant.status === GRANT_STATUS.approved);
  const catalogue: readonly WorkspaceModel[] = models ?? [];
  return {
    latestRun: initialSummary.latest,
    models: catalogue,
    content: <ChatView
      workspaceId={workspaceId}
      fleetId={fleet.id}
      schedules={fleet.schedules}
      sources={fleetSources(github?.scope)}
      senderLabel={fleet.name}
      lists={<><FleetListSections /><FleetNetworkSection /></>}
      modelLabel={runtimeModelLabel(models, fleet.model_ref)}
      initial={turns}
      initialSummary={initialSummary}
      approvalsHref={approvalsHref}
      viewer={viewer}
      senderNames={namedMembers(members ?? [])}
    />,
  };
}
