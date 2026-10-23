import type { ReactNode } from "react";
import { claims, requireCredential } from "@/lib/auth/credential";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { notFound, redirect } from "next/navigation";
import { cn } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import { ApiError } from "@/lib/api/errors";
import { getFleet } from "@/lib/api/fleets";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { FleetSetupWorkspace } from "@/components/domain/chat/FleetSetupWorkspace";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import {
  startViewData,
  runtimeModelLabel,
  type ChatViewData,
  type EventsViewData,
  type MemoryViewData,
  type ViewData,
} from "./components/view-data";
import { FleetActivity } from "./components/FleetActivity";
import {
  CURSOR_PAGE_SIZE_PARAM,
  CURSOR_TRAIL_PARAM,
  PAGE_SIZE_PARAM,
  cursorForTrail,
  cursorTrailFrom,
  pageSizeFrom,
} from "@/lib/pagination/cursor-trail";
import MemoryPanel from "./components/MemoryPanel";
import { ChatView } from "./components/ChatView";
import { FleetHeader } from "./components/FleetHeader";
import { buildRunSummary } from "@/lib/events/run-summary";
import { FleetRecovery } from "./components/FleetRecovery";
import { FleetViewedTracker } from "./components/FleetViewedTracker";
import { namedMembers } from "@/lib/events/sender-names";
import {
  FleetSubnavigation,
  FLEET_VIEW,
  resolveFleetView,
} from "./components/FleetSubnavigation";
import type { FleetDetail } from "@/lib/types";

export const dynamic = "force-dynamic";

type PageContext = {
  workspaceId: string;
  fleet: FleetDetail;
  etag: string;
  token: string;
  /** Cursor of the events page named by the URL; null on the first page. */
  eventsCursor: string | null;
  eventsPageSize: number;
};

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

  const view = resolveFleetView(
    typeof query.view === "string" ? query.view : undefined,
  );
  const eventsPageSize = pageSizeFrom(query[PAGE_SIZE_PARAM]);
  const eventsCursor = cursorForTrail(
    cursorTrailFrom(
      query[CURSOR_TRAIL_PARAM],
      eventsPageSize,
      query[CURSOR_PAGE_SIZE_PARAM],
    ),
  );
  if (!view) redirect(workspacePath(workspaceId, `fleets/${id}`));

  // View data that needs only route params starts HERE, beside the fleet
  // read — the detail read used to serialize every view fetch behind it.
  const viewData = startViewData(view, {
    workspaceId,
    fleetId: id,
    token,
    eventsCursor,
    eventsPageSize,
  });
  const [fleetResult, billing, workspaces, sessionClaims] = await Promise.all([
    loadFleet(workspaceId, id, token),
    getTenantBillingCached(token).catch(() => null),
    listTenantWorkspacesCached(token).catch(() => null),
    claims(),
  ]);
  if (!fleetResult) notFound();

  const { fleet, etag } = fleetResult;
  const content = await loadFleetView(
    {
      workspaceId,
      fleet,
      etag,
      token,
      eventsCursor,
      eventsPageSize,
    },
    viewData,
  );
  // Chat owns scrolling inside its transcript. Other tabs own a scrollable
  // content area beneath the fixed header and navigation.
  const claimsViewport = view === FLEET_VIEW.chat;

  return (
    <div
      data-fleet-workspace
      className={cn(
        "flex h-full min-h-0 flex-1 flex-col overflow-clip",
      )}
    >
      <FleetViewedTracker fleetId={fleet.id} status={fleet.status} />
      <FleetSetupWorkspace account={workspaces?.items.find(item => item.id === workspaceId)?.account.tenant_id ?? null}
        workspace={workspaceId} fleet={fleet.id} revision={fleet.setup_revision}
        viewer={typeof sessionClaims?.sub === "string" ? sessionClaims.sub : null} header={<>
      <FleetHeader
        workspaceId={workspaceId}
        fleet={fleet}
        exhaustedAt={billing?.is_exhausted ? billing.exhausted_at : undefined}
      />

      <FleetRecovery key={`${workspaceId}:${fleet.id}:${fleet.etag}`} workspace={workspaceId} fleet={fleet} />
      </>}>
      <div
        className={cn(
          "flex min-h-0 flex-1 flex-col",
          claimsViewport && "h-full overflow-clip",
        )}
      >
        <div
          className={cn(
            "flex min-h-0 min-w-0 flex-1 flex-col gap-3xl",
            claimsViewport && "h-full min-h-0 flex-1 gap-0 overflow-clip",
          )}
        >
          <FleetSubnavigation
            workspaceId={workspaceId}
            fleetId={fleet.id}
            activeView={view}
          />
          <div
            key={view}
            className={cn(
              "page-entrance flex min-h-0 min-w-0 flex-1 flex-col",
              claimsViewport ? "h-full overflow-clip" : "overflow-y-auto px-xl pb-xl sm:px-chat-inset",
            )}
          >
            {content}
          </div>
        </div>
      </div>
      </FleetSetupWorkspace>
    </div>
  );
}

async function loadFleet(workspaceId: string, id: string, token: string) {
  return getFleet(workspaceId, id, token).catch((error: unknown) => {
    if (error instanceof ApiError && (error.status === 400 || error.status === 404)) return null;
    throw error;
  });
}

async function loadFleetView(
  context: PageContext,
  data: ViewData,
): Promise<ReactNode> {
  switch (data.view) {
    case FLEET_VIEW.events:
      return loadEventsView(context, data);
    case FLEET_VIEW.memory:
      return loadMemoryView(context, data);
    default:
      return loadChatView(context, data);
  }
}

async function loadChatView(
  { workspaceId, fleet }: PageContext,
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
      modelLabel={runtimeModelLabel(models, fleet.definition.config["x-agentsfleet"].model)}
      initial={turns}
      initialSummary={buildRunSummary(fleet.status, threadResult, fleet.pending_approvals)}
      approvalsHref={approvalsHref}
      viewer={typeof subject === "string" ? subject : null}
      senderNames={namedMembers(members ?? [])}
    />
  );
}

async function loadEventsView(
  { fleet, eventsPageSize }: PageContext,
  data: EventsViewData,
) {
  const result = await data.eventsInitial;
  if (!result.ok) throw result.error;
  const initial = result.page;
  return (
    <FleetActivity
      fleetId={fleet.id}
      initial={initial}
      pageSize={eventsPageSize}
    />
  );
}

async function loadMemoryView(
  { workspaceId, fleet }: PageContext,
  data: MemoryViewData,
) {
  const [memories, canGrant] = await Promise.all([data.memories, hasScope(SCOPE.FLEET_WRITE)]);
  return (
    <MemoryPanel
      workspaceId={workspaceId}
      fleetId={fleet.id}
      entries={memories?.items ?? null}
      access={fleet.memory_access}
      canGrant={canGrant}
    />
  );
}

