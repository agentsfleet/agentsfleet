import type { ReactNode } from "react";
import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import {
  Card,
  cn,
  EYEBROW_CLASS,
  PageHeader,
  PageTitle,
  Section,
} from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import { ApiError } from "@/lib/api/errors";
import { getFleet, AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { listFleetEvents } from "@/lib/api/events";
import { listApprovals } from "@/lib/api/approvals";
import { listMemories } from "@/lib/api/memory";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import { EventsList } from "@/components/domain/EventsList";
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
import {
  FleetSubnavigation,
  FLEET_VIEW,
  resolveFleetView,
  type FleetView,
} from "./components/FleetSubnavigation";
import { DANGER_ZONE_LABEL, SOURCE_FIELD } from "./components/console-copy";
import type { FleetDetail } from "@/lib/types";

export const dynamic = "force-dynamic";

type PageContext = {
  workspaceId: string;
  fleet: FleetDetail;
  etag: string;
  token: string;
};

export default async function FleetDetailPage({
  params,
  searchParams,
}: {
  params: Promise<{ workspaceId: string; id: string }>;
  searchParams?: Promise<{ view?: string }>;
}) {
  const { workspaceId, id } = await params;
  const query = searchParams ? await searchParams : { view: undefined };
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const [fleetResult, billing] = await Promise.all([
    loadFleet(workspaceId, id, token),
    getTenantBillingCached(token).catch(() => null),
  ]);
  if (!fleetResult) notFound();

  const { fleet, etag } = fleetResult;
  const view = resolveFleetView(query.view);
  const content = await loadFleetView(view, { workspaceId, fleet, etag, token });
  // The chat is a conversation surface, not a document: it claims the frame so
  // its composer stays on screen and only the message list scrolls. Every
  // other view is ordinary page content and scrolls with the page.
  const claimsViewport = view === FLEET_VIEW.chat;

  return (
    <div className={cn("flex flex-col", claimsViewport && "min-h-0 flex-1")}>
      <FleetViewedTracker fleetId={fleet.id} status={fleet.status} />
      <FleetBreadcrumb workspaceId={workspaceId} fleetName={fleet.name} />
      <PageHeader>
        <div className="flex items-center gap-md">
          <PageTitle>{fleet.name}</PageTitle>
          <FleetStatus status={fleet.status} />
          {billing?.is_exhausted ? <ExhaustionBadge exhaustedAt={billing.exhausted_at} /> : null}
        </div>
      </PageHeader>

      <FleetInstallGate
        workspaceId={workspaceId}
        fleetId={fleet.id}
        fleetName={fleet.name}
        status={fleet.status}
        className={cn("flex flex-col", claimsViewport && "min-h-0 flex-1")}
      >
        <div
          className={cn(
            "flex min-w-0 flex-col gap-xl lg:flex-row",
            claimsViewport && "min-h-0 flex-1",
          )}
        >
          <FleetSubnavigation
            workspaceId={workspaceId}
            fleetId={fleet.id}
            activeView={view}
          />
          <div className={cn("flex min-w-0 flex-1 flex-col", claimsViewport && "min-h-0")}>
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
    case FLEET_VIEW.settings:
      return <SettingsView context={context} />;
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
    <div className="flex min-h-0 flex-1 flex-col gap-lg">
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
        fleetName={fleet.name}
        initial={events.items}
      />
    </div>
  );
}

async function loadEventsView({ workspaceId, fleet, token }: PageContext) {
  const initial = await listFleetEvents(workspaceId, fleet.id, token, { limit: 50 })
    .catch(() => ({ items: [], next_cursor: null }));
  return <EventsList workspaceId={workspaceId} fleetId={fleet.id} initial={initial} />;
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
  return <SourceEditor context={context} field={field} />;
}

function SourceEditor({ context, field }: { context: PageContext; field: "skill" | "trigger" }) {
  const { workspaceId, fleet, etag } = context;
  return (
    <SkillEditor
      workspaceId={workspaceId}
      fleetId={fleet.id}
      field={field}
      sourceMarkdown={fleet.source_markdown}
      triggerMarkdown={fleet.trigger_markdown}
      etag={etag}
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
    <div className="flex flex-col gap-lg">
      <SourceEditor context={context} field={SOURCE_FIELD.trigger} />
      <TriggerPanel triggers={triggers} lastDeliveryByKey={lastDeliveryByKey} />
    </div>
  );
}

function SettingsView({ context }: { context: PageContext }) {
  const { workspaceId, fleet } = context;
  return (
    <div className="flex max-w-3xl flex-col gap-xl">
      <Card className="flex items-center justify-between gap-lg p-lg">
        <div>
          <h2 className="font-mono text-sm font-medium">Runtime</h2>
          <p className="mt-xs text-sm text-muted-foreground">
            Stop, resume, or permanently kill this fleet.
          </p>
        </div>
        <KillSwitch
          workspaceId={workspaceId}
          fleet={{
            id: fleet.id,
            name: fleet.name,
            status: fleet.status,
            created_at: fleet.created_at,
            updated_at: fleet.updated_at,
            triggers: fleet.triggers ?? undefined,
          }}
        />
      </Card>
      <Section aria-label={DANGER_ZONE_LABEL} className="gap-0">
        <h2 className={cn(EYEBROW_CLASS, "mb-sm text-muted-foreground")}>
          {DANGER_ZONE_LABEL}
        </h2>
        <FleetConfig workspaceId={workspaceId} fleetId={fleet.id} fleetName={fleet.name} />
      </Section>
    </div>
  );
}

function FleetBreadcrumb({ workspaceId, fleetName }: { workspaceId: string; fleetName: string }) {
  return (
    <p className="mb-sm font-mono text-sm text-muted-foreground">
      <Link href={workspacePath(workspaceId, "fleets")} className="hover:text-foreground">
        Fleets
      </Link>
      <span aria-hidden="true"> / </span>
      <span className="text-foreground">{fleetName}</span>
    </p>
  );
}

function FleetStatus({ status }: { status: string }) {
  return (
    <span
      className={cn(EYEBROW_CLASS, "inline-flex items-center gap-sm text-muted-foreground")}
      data-state={status}
    >
      {status === AGENTSFLEET_STATUS.ACTIVE ? (
        <span className="size-2 rounded-full bg-pulse" aria-hidden="true" />
      ) : null}
      {status}
    </span>
  );
}
