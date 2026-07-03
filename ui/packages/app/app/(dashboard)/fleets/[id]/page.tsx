import { auth } from "@clerk/nextjs/server";
import { notFound, redirect } from "next/navigation";
import { Badge, PageHeader, PageTitle, Section, SectionLabel, WakePulse } from "@agentsfleet/design-system";
import { getFleet, AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { listFleetEvents } from "@/lib/api/events";
import { listApprovals } from "@/lib/api/approvals";
import { resolveActiveWorkspaceId } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import FleetApprovalsPanel from "@/components/domain/FleetApprovalsPanel";
import FleetThreadDynamic from "@/components/domain/FleetThreadDynamic";
import TriggerPanel from "./components/TriggerPanel";
import FleetConfig from "./components/FleetConfig";
import KillSwitch from "./components/KillSwitch";
import { FleetInstallGate } from "./components/FleetInstallGate";
import { FleetViewedTracker } from "./components/FleetViewedTracker";
import { resolveLastDeliveries } from "./components/last-delivery";

export const dynamic = "force-dynamic";

export default async function FleetDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const active = await resolveActiveWorkspaceId(token);
  if (!active) notFound();
  const workspaceId = active.id;

  const [fleet, billing, eventsPage, pendingApprovals] = await Promise.all([
    getFleet(workspaceId, id, token),
    getTenantBillingCached(token).catch(() => null),
    listFleetEvents(workspaceId, id, token, { limit: 20 }).catch(() => ({ items: [], next_cursor: null })),
    listApprovals(workspaceId, token, { fleetId: id, limit: 50 }).catch(() => ({ items: [], next_cursor: null })),
  ]);
  if (!fleet) notFound();

  // Per-trigger "last delivery" lookup. One lightweight server-side call
  // per declared trigger, in parallel; failures degrade to `null` (the
  // TriggerPanel renders "never"). Webhook actors are namespaced as
  // `webhook:<source>:*`; cron as `cron:*`.
  const triggerList = fleet.triggers ?? [];
  const lastDeliveryByKey = await resolveLastDeliveries(
    workspaceId,
    id,
    token,
    triggerList,
  );
  // Exact count up to the page size; "50+" past that. The Approvals panel
  // below paginates the full list — the badge is just a glance signal.
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
          <span className="inline-flex items-center gap-2 font-mono text-label uppercase tracking-label text-muted-foreground" data-state={fleet.status}>
            {fleet.status === AGENTSFLEET_STATUS.ACTIVE ? (
              <WakePulse
                live
                className="inline-block w-2 h-2 rounded-full bg-pulse"
                aria-hidden="true"
              />
            ) : null}
            {fleet.status === AGENTSFLEET_STATUS.INSTALLING ? (
              <WakePulse
                live
                className="inline-block w-2 h-2 rounded-full bg-info"
                aria-hidden="true"
              />
            ) : null}
            {fleet.status}
          </span>
          {billing?.is_exhausted ? (
            <ExhaustionBadge exhaustedAt={billing.exhausted_at} />
          ) : null}
          {hasPending ? (
            <Badge variant="destructive">{pendingCountLabel} pending approval{pendingApprovals.items.length === 1 ? "" : "s"}</Badge>
          ) : null}
        </div>
        <KillSwitch workspaceId={workspaceId} fleet={fleet} />
      </PageHeader>

      <FleetInstallGate
        workspaceId={workspaceId}
        fleetId={fleet.id}
        fleetName={fleet.name}
        status={fleet.status}
      >
      <Section asChild>
        <section aria-label="Trigger">
          <SectionLabel>Trigger</SectionLabel>
          <TriggerPanel
            fleetId={fleet.id}
            triggers={triggerList}
            lastDeliveryByKey={lastDeliveryByKey}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Configuration">
          <SectionLabel>Configuration</SectionLabel>
          <FleetConfig
            workspaceId={workspaceId}
            fleetId={fleet.id}
            fleetName={fleet.name}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Pending approvals">
          <SectionLabel>Pending approvals</SectionLabel>
          <FleetApprovalsPanel workspaceId={workspaceId} fleetId={fleet.id} token={token} />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Live activity">
          <SectionLabel>Live activity</SectionLabel>
          <FleetThreadDynamic
            workspaceId={workspaceId}
            fleetId={fleet.id}
            initial={eventsPage.items}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Recent Activity">
          <SectionLabel>Recent Activity</SectionLabel>
          <EventsList
            scope={{ kind: "fleet", workspaceId: workspaceId, fleetId: fleet.id }}
            initial={eventsPage}
          />
        </section>
      </Section>
      </FleetInstallGate>
    </div>
  );
}
