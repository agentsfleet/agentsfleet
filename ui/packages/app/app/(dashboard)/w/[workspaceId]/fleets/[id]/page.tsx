import type { ReactNode } from "react";
import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import { Badge, cn } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import { ApiError } from "@/lib/api/errors";
import { getFleet, AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { listFleetEvents } from "@/lib/api/events";
import { listApprovals } from "@/lib/api/approvals";
import { listMemories } from "@/lib/api/memory";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import { EventsList } from "@/components/domain/EventsList";
import {
  CURSOR_TRAIL_PARAM,
  EVENTS_PAGE_SIZE,
  cursorForTrail,
  cursorTrailFrom,
} from "@/lib/pagination/cursor-trail";
import FleetThreadDynamic from "@/components/domain/FleetThreadDynamic";
import TriggerPanel from "./components/TriggerPanel";
import FleetConfig from "./components/FleetConfig";
import KillSwitch from "./components/KillSwitch";
import SkillEditor from "./components/SkillEditor";
import MemoryPanel from "./components/MemoryPanel";
import RunMetricsStrip from "./components/RunMetricsStrip";
import { FleetInstallGate } from "./components/FleetInstallGate";
import { FleetViewedTracker } from "./components/FleetViewedTracker";
import { resolveLastDeliveries } from "./components/last-delivery";
import { deriveFleetIdentity } from "../components/fleetIdentity";
import {
  FleetSubnavigation,
  FLEET_VIEW,
  resolveFleetView,
  type FleetView,
} from "./components/FleetSubnavigation";
import {
  BREADCRUMB_LABEL,
  FLEETS_CRUMB_LABEL,
  SOURCE_FIELD,
} from "./components/console-copy";
import type { FleetDetail } from "@/lib/types";

export const dynamic = "force-dynamic";

type PageContext = {
  workspaceId: string;
  fleet: FleetDetail;
  etag: string;
  token: string;
  /** Cursor of the events page named by the URL; null on the first page. */
  eventsCursor: string | null;
};

const LIFECYCLE_ACTION_STATUSES = new Set<string>([
  AGENTSFLEET_STATUS.ACTIVE,
  AGENTSFLEET_STATUS.PAUSED,
  AGENTSFLEET_STATUS.STOPPED,
]);

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
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const view = resolveFleetView(typeof query.view === "string" ? query.view : undefined);
  const eventsCursor = cursorForTrail(cursorTrailFrom(query[CURSOR_TRAIL_PARAM]));
  if (!view) redirect(workspacePath(workspaceId, `fleets/${id}`));

  const [fleetResult, billing] = await Promise.all([
    loadFleet(workspaceId, id, token),
    getTenantBillingCached(token).catch(() => null),
  ]);
  if (!fleetResult) notFound();

  const { fleet, etag } = fleetResult;
  const content = await loadFleetView(view, { workspaceId, fleet, etag, token, eventsCursor });
  // The chat is a conversation surface, not a document: it claims the frame so
  // its composer stays on screen and only the message list scrolls. Every
  // other view is ordinary page content and scrolls with the page.
  const claimsViewport = view === FLEET_VIEW.chat;

  return (
    <div className={cn("flex min-h-full flex-1 flex-col", claimsViewport && "h-full min-h-0 overflow-hidden")}>
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
            exhaustedAt={billing?.is_exhausted ? billing.exhausted_at : undefined}
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
          <div className={cn("flex min-w-0 flex-1 flex-col", claimsViewport && "h-full min-h-0 overflow-hidden")}>
            {content}
          </div>
        </div>
      </FleetInstallGate>
    </div>
  );
}

async function loadFleet(workspaceId: string, id: string, token: string) {
  return getFleet(workspaceId, id, token).catch((error: unknown) => {
    if (error instanceof ApiError && error.status === 404) return null;
    throw error;
  });
}

async function loadFleetView(view: FleetView, context: PageContext): Promise<ReactNode> {
  switch (view) {
    case FLEET_VIEW.events:
      return loadEventsView(context);
    case FLEET_VIEW.memory:
      return loadMemoryView(context);
    case FLEET_VIEW.skill:
      return <SourceView context={context} field={SOURCE_FIELD.skill} />;
    case FLEET_VIEW.trigger:
      return loadTriggerView(context);
    default:
      return loadChatView(context);
  }
}

async function loadChatView({ workspaceId, fleet, token }: PageContext) {
  const [eventsResult, approvalsResult] = await Promise.all([
    listFleetEvents(workspaceId, fleet.id, token, { limit: 20 })
      .catch(() => null),
    listApprovals(workspaceId, token, { fleetId: fleet.id, limit: 50 })
      .catch(() => null),
  ]);
  const events = eventsResult ?? { items: [], next_cursor: null };
  const approvals = approvalsResult ?? { items: [], next_cursor: null };
  const approvalsHref = `${workspacePath(workspaceId, "approvals")}?fleetId=${encodeURIComponent(fleet.id)}`;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-md overflow-hidden">
      <div className="shrink-0">
        <RunMetricsStrip
          status={fleet.status}
          latest={events.items[0] ?? null}
          pendingApprovals={approvals.items.length}
          pendingApprovalsHasMore={approvals.next_cursor !== null}
          approvalsHref={approvalsHref}
          summaryAvailable={eventsResult !== null}
          approvalsAvailable={approvalsResult !== null}
        />
      </div>
      <FleetThreadDynamic
        workspaceId={workspaceId}
        fleetId={fleet.id}
        fleetName={`Agent ${deriveFleetIdentity(fleet.id).callsign}`}
        initial={events.items}
      />
    </div>
  );
}

async function loadEventsView({ workspaceId, fleet, token, eventsCursor }: PageContext) {
  // Fetched on the server for the cursor the URL names, so a reload or a
  // shared link opens the page the operator was actually looking at.
  const initial = await listFleetEvents(workspaceId, fleet.id, token, {
    limit: EVENTS_PAGE_SIZE,
    ...(eventsCursor ? { cursor: eventsCursor } : {}),
  }).catch(() => ({ items: [], next_cursor: null }));
  return <EventsList fleetId={fleet.id} initial={initial} />;
}

async function loadMemoryView({ workspaceId, fleet, token }: PageContext) {
  const memories = await listMemories(workspaceId, fleet.id, token, { limit: 100 })
    .catch(() => null);
  return (
    <MemoryPanel
      workspaceId={workspaceId}
      fleetId={fleet.id}
      entries={memories?.items ?? null}
    />
  );
}

function SourceView({ context, field }: { context: PageContext; field: typeof SOURCE_FIELD.skill }) {
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

function FleetBreadcrumb({ workspaceId, fleetName }: { workspaceId: string; fleetName: string }) {
  return (
    <nav
      aria-label={BREADCRUMB_LABEL}
      className="mb-sm shrink-0 font-mono text-sm text-muted-foreground"
    >
      <Link href={workspacePath(workspaceId, "fleets")} className="hover:text-foreground">
        {FLEETS_CRUMB_LABEL}
      </Link>
      <span aria-hidden="true"> / </span>
      <span className="text-foreground">{fleetName}</span>
    </nav>
  );
}

function FleetHeader({
  workspaceId,
  fleet,
  exhaustedAt,
}: {
  workspaceId: string;
  fleet: FleetDetail;
  exhaustedAt?: number | null;
}) {
  const actionFleet = {
    id: fleet.id,
    name: fleet.name,
    status: fleet.status,
    created_at: fleet.created_at,
    updated_at: fleet.updated_at,
    triggers: fleet.triggers ?? undefined,
  };
  return (
    <div className="mb-lg flex flex-col gap-md sm:flex-row sm:items-center sm:justify-between">
      <h1 className="sr-only">{fleet.name}</h1>
      <FleetBreadcrumb workspaceId={workspaceId} fleetName={fleet.name} />
      <div aria-label="Fleet lifecycle actions" className="flex flex-wrap items-center justify-end gap-sm">
        {exhaustedAt !== undefined ? <ExhaustionBadge exhaustedAt={exhaustedAt} /> : null}
        {fleet.status === AGENTSFLEET_STATUS.INSTALLING ? (
          <Badge variant="cyan" aria-label="Fleet status: installing">Installing</Badge>
        ) : fleet.status === AGENTSFLEET_STATUS.KILLED ? (
          <FleetConfig workspaceId={workspaceId} fleetId={fleet.id} fleetName={fleet.name} />
        ) : LIFECYCLE_ACTION_STATUSES.has(fleet.status) ? (
          <KillSwitch workspaceId={workspaceId} fleet={actionFleet} />
        ) : (
          <Badge aria-label={`Fleet status: ${fleet.status}`}>{fleet.status}</Badge>
        )}
      </div>
    </div>
  );
}
