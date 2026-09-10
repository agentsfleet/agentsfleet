import type { ReactNode } from "react";
import { requireCredential } from "@/lib/auth/credential";
import { notFound, redirect } from "next/navigation";
import { cn } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import { ApiError } from "@/lib/api/errors";
import { getFleet } from "@/lib/api/fleets";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import {
  startViewData,
  type ChatViewData,
  type EventsViewData,
  type MemoryViewData,
  type ViewData,
} from "./components/view-data";
import { EventsList } from "@/components/domain/EventsList";
import {
  CURSOR_PAGE_SIZE_PARAM,
  CURSOR_TRAIL_PARAM,
  PAGE_SIZE_PARAM,
  cursorForTrail,
  cursorTrailFrom,
  pageSizeFrom,
} from "@/lib/pagination/cursor-trail";
import TriggerPanel from "./components/TriggerPanel";
import SkillEditor from "./components/SkillEditor";
import MemoryPanel from "./components/MemoryPanel";
import { ChatView } from "./components/ChatView";
import { FleetHeader } from "./components/FleetHeader";
import { buildRunSummary } from "@/lib/events/run-summary";
import { FleetInstallGate } from "./components/FleetInstallGate";
import { FleetViewedTracker } from "./components/FleetViewedTracker";
import { resolveLastDeliveries } from "./components/last-delivery";
import { deriveFleetIdentity } from "../components/fleetIdentity";
import {
  FleetSubnavigation,
  FLEET_VIEW,
  resolveFleetView,
} from "./components/FleetSubnavigation";
import { SOURCE_FIELD } from "./components/console-copy";
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
  const [fleetResult, billing] = await Promise.all([
    loadFleet(workspaceId, id, token),
    getTenantBillingCached(token).catch(() => null),
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
  // The chat is a conversation surface, not a document: it claims the frame so
  // its composer stays on screen and only the message list scrolls. Every
  // other view is ordinary page content and scrolls with the page.
  const claimsViewport = view === FLEET_VIEW.chat;

  return (
    <div
      className={cn(
        "flex min-h-full flex-1 flex-col",
        claimsViewport && "h-full min-h-0 overflow-hidden",
      )}
    >
      <FleetViewedTracker fleetId={fleet.id} status={fleet.status} />
      <div className="flex min-w-0 flex-col gap-3xl lg:flex-row">
        <div
          aria-hidden="true"
          data-testid="fleet-header-alignment-spacer"
          className="hidden lg:block lg:w-56 lg:shrink-0"
        />
        <div className="min-w-0 flex-1">
          <FleetHeader
            workspaceId={workspaceId}
            fleet={fleet}
            exhaustedAt={
              billing?.is_exhausted ? billing.exhausted_at : undefined
            }
          />
        </div>
      </div>

      <FleetInstallGate
        workspaceId={workspaceId}
        fleetId={fleet.id}
        fleetName={fleet.name}
        status={fleet.status}
        className={cn(
          "flex min-h-0 flex-1 flex-col",
          claimsViewport && "h-full overflow-hidden",
        )}
      >
        <div
          className={cn(
            "flex min-w-0 flex-1 flex-col gap-3xl lg:flex-row lg:items-stretch",
            claimsViewport && "h-full min-h-0 flex-1 overflow-hidden",
          )}
        >
          <FleetSubnavigation
            workspaceId={workspaceId}
            fleetId={fleet.id}
            activeView={view}
          />
          <div
            className={cn(
              "flex min-w-0 flex-1 flex-col",
              claimsViewport && "h-full min-h-0 overflow-hidden",
            )}
          >
            {content}
          </div>
        </div>
      </FleetInstallGate>
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
    case FLEET_VIEW.skill:
      return <SourceView context={context} field={SOURCE_FIELD.skill} />;
    case FLEET_VIEW.trigger:
      return loadTriggerView(context);
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
  const threadResult = await data.thread;
  const turns = threadResult?.items ?? [];
  const approvalsHref = `${workspacePath(workspaceId, "approvals")}?fleetId=${encodeURIComponent(fleet.id)}`;
  return (
    <ChatView
      workspaceId={workspaceId}
      fleetId={fleet.id}
      fleetName={`Agent ${deriveFleetIdentity(fleet.id).callsign}`}
      initial={turns}
      initialSummary={buildRunSummary(fleet.status, threadResult, fleet.pending_approvals)}
      approvalsHref={approvalsHref}
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
    <EventsList
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
  const memories = await data.memories;
  return (
    <MemoryPanel
      workspaceId={workspaceId}
      fleetId={fleet.id}
      entries={memories?.items ?? null}
    />
  );
}

function SourceView({
  context,
  field,
}: {
  context: PageContext;
  field: typeof SOURCE_FIELD.skill;
}) {
  return <SourceEditor context={context} field={field} fillAvailableSpace />;
}

function SourceEditor({
  context,
  field,
  fillAvailableSpace = false,
}: {
  context: PageContext;
  field: "skill" | "trigger";
  fillAvailableSpace?: boolean;
}) {
  const { workspaceId, fleet, etag } = context;
  return (
    <SkillEditor
      workspaceId={workspaceId}
      fleetId={fleet.id}
      field={field}
      sourceMarkdown={fleet.source_markdown}
      triggerMarkdown={fleet.trigger_markdown}
      etag={etag}
      fillAvailableSpace={fillAvailableSpace}
    />
  );
}

async function loadTriggerView(context: PageContext) {
  const { workspaceId, fleet, token } = context;
  const triggers = fleet.triggers ?? [];
  const lastDeliveryByKey = await resolveLastDeliveries(
    workspaceId,
    fleet.id,
    token,
    triggers,
  );
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-lg">
      <SourceEditor context={context} field={SOURCE_FIELD.trigger} />
      <TriggerPanel triggers={triggers} lastDeliveryByKey={lastDeliveryByKey} />
    </div>
  );
}
