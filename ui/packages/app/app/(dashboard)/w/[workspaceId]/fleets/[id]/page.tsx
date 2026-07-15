import { auth } from "@clerk/nextjs/server";
import { notFound, redirect } from "next/navigation";
import { Badge, cn, EYEBROW_CLASS, PageHeader, PageTitle, Section, SectionLabel, WakePulse } from "@agentsfleet/design-system";
import { ApiError } from "@/lib/api/errors";
import { getFleet, AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { listFleetEvents } from "@/lib/api/events";
import { listApprovals } from "@/lib/api/approvals";
import { listMemories } from "@/lib/api/memory";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import FleetApprovalsPanel from "@/components/domain/FleetApprovalsPanel";
import FleetThreadDynamic from "@/components/domain/FleetThreadDynamic";
import TriggerPanel from "./components/TriggerPanel";
import FleetConfig from "./components/FleetConfig";
import KillSwitch from "./components/KillSwitch";
import SkillEditor from "./components/SkillEditor";
import MemoryPanel from "./components/MemoryPanel";
import RunsLedger from "./components/RunsLedger";
import RunMetricsStrip from "./components/RunMetricsStrip";
import { FleetInstallGate } from "./components/FleetInstallGate";
import { FleetViewedTracker } from "./components/FleetViewedTracker";
import { resolveLastDeliveries } from "./components/last-delivery";
import {
  APPROVALS_SECTION_LABEL,
  COLUMN_DOES_LABEL,
  COLUMN_IS_LABEL,
  COLUMN_KNOWS_LABEL,
  DANGER_ZONE_LABEL,
  ROLLUP_WINDOW_LIMIT,
  ROLLUP_WINDOW_SINCE,
  TRIGGERS_SECTION_LABEL,
} from "./components/console-copy";

export const dynamic = "force-dynamic";

// The three-column console (M131 §3): what the fleet IS (source, triggers,
// danger zone), what it DOES (metrics strip + steer thread), what it KNOWS and
// COSTS (memory, approvals, runs ledger). The middle column is widest — it
// carries the live thread. Below the content breakpoint the columns stack and
// the body never scrolls horizontally (each column is `min-w-0`).
const CONSOLE_GRID = "grid grid-cols-1 gap-xl lg:grid-cols-[minmax(0,1fr)_minmax(0,1.5fr)_minmax(0,1fr)]";

export default async function FleetDetailPage({
  params,
}: {
  params: Promise<{ workspaceId: string; id: string }>;
}) {
  const { workspaceId, id } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Every read fires in parallel. getFleet hits the real GET …/fleets/{id}
  // (§1) and throws on 404, so it takes its own catch to a null sentinel and we
  // notFound() after the batch. The 7-day window (§6) catches to `null` so the
  // ledger degrades to the lifetime figure rather than failing the page. The
  // ETag getFleet returns is held for the source editor's If-Match save (§4).
  const [fleetResult, billing, eventsPage, windowPage, pendingApprovals, memories] = await Promise.all([
    getFleet(workspaceId, id, token).catch((error: unknown) => {
      if (error instanceof ApiError && error.status === 404) return null;
      throw error;
    }),
    getTenantBillingCached(token).catch(() => null),
    listFleetEvents(workspaceId, id, token, { limit: 20 }).catch(() => ({ items: [], next_cursor: null })),
    listFleetEvents(workspaceId, id, token, { since: ROLLUP_WINDOW_SINCE, limit: ROLLUP_WINDOW_LIMIT }).catch(() => null),
    listApprovals(workspaceId, token, { fleetId: id, limit: 50 }).catch(() => ({ items: [], next_cursor: null })),
    listMemories(workspaceId, id, token, { limit: 100 }).catch(() => ({ items: [], total: 0, request_id: "" })),
  ]);
  if (!fleetResult) notFound();
  const { fleet, etag } = fleetResult;

  const triggerList = fleet.triggers ?? [];
  const lastDeliveryByKey = await resolveLastDeliveries(workspaceId, id, token, triggerList);
  const hasPending = pendingApprovals.items.length > 0;
  const pendingCountLabel = pendingApprovals.next_cursor
    ? `${pendingApprovals.items.length}+`
    : String(pendingApprovals.items.length);

  return (
    <div>
      <FleetViewedTracker fleetId={fleet.id} status={fleet.status} />
      <PageHeader>
        <div className="flex items-center gap-3">
          <PageTitle>{fleet.name}</PageTitle>
          <span className={cn(EYEBROW_CLASS, "inline-flex items-center gap-2 text-muted-foreground")} data-state={fleet.status}>
            {fleet.status === AGENTSFLEET_STATUS.ACTIVE ? (
              <WakePulse live className="inline-block w-2 h-2 rounded-full bg-pulse" aria-hidden="true" />
            ) : null}
            {fleet.status === AGENTSFLEET_STATUS.INSTALLING ? (
              <WakePulse live className="inline-block w-2 h-2 rounded-full bg-info" aria-hidden="true" />
            ) : null}
            {fleet.status}
          </span>
          {billing?.is_exhausted ? <ExhaustionBadge exhaustedAt={billing.exhausted_at} /> : null}
          {hasPending ? (
            <Badge variant="destructive">
              {pendingCountLabel} pending approval{pendingApprovals.items.length === 1 ? "" : "s"}
            </Badge>
          ) : null}
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
      </PageHeader>

      <FleetInstallGate workspaceId={workspaceId} fleetId={fleet.id} fleetName={fleet.name} status={fleet.status}>
        <div className={CONSOLE_GRID}>
          <Section asChild>
          <section aria-label={COLUMN_IS_LABEL} className="min-w-0">
            <SectionLabel>{COLUMN_IS_LABEL}</SectionLabel>
            <SkillEditor
              workspaceId={workspaceId}
              fleetId={fleet.id}
              sourceMarkdown={fleet.source_markdown}
              triggerMarkdown={fleet.trigger_markdown}
              etag={etag}
            />
            <div className="flex flex-col gap-xs">
              <h3 className={cn(EYEBROW_CLASS, "text-muted-foreground")}>{TRIGGERS_SECTION_LABEL}</h3>
              <TriggerPanel
                workspaceId={workspaceId}
                fleetId={fleet.id}
                triggers={triggerList}
                lastDeliveryByKey={lastDeliveryByKey}
              />
            </div>
            <div className="flex flex-col gap-xs">
              <h3 className={cn(EYEBROW_CLASS, "text-muted-foreground")}>{DANGER_ZONE_LABEL}</h3>
              <FleetConfig workspaceId={workspaceId} fleetId={fleet.id} fleetName={fleet.name} />
            </div>
          </section>
          </Section>

          <Section asChild>
          <section aria-label={COLUMN_DOES_LABEL} className="min-w-0">
            <SectionLabel>{COLUMN_DOES_LABEL}</SectionLabel>
            <RunMetricsStrip latest={eventsPage.items[0] ?? null} />
            <FleetThreadDynamic workspaceId={workspaceId} fleetId={fleet.id} initial={eventsPage.items} />
          </section>
          </Section>

          <Section asChild>
          <section aria-label={COLUMN_KNOWS_LABEL} className="min-w-0">
            <SectionLabel>{COLUMN_KNOWS_LABEL}</SectionLabel>
            <MemoryPanel workspaceId={workspaceId} fleetId={fleet.id} entries={memories.items} />
            <div className="flex flex-col gap-xs">
              <h3 className={cn(EYEBROW_CLASS, "text-muted-foreground")}>{APPROVALS_SECTION_LABEL}</h3>
              <FleetApprovalsPanel workspaceId={workspaceId} fleetId={fleet.id} token={token} />
            </div>
            <RunsLedger windowEvents={windowPage === null ? null : windowPage.items} lifetimeBudgetNanos={fleet.budget_used_nanos} />
          </section>
          </Section>
        </div>
      </FleetInstallGate>
    </div>
  );
}
