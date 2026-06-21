import { auth } from "@clerk/nextjs/server";
import { notFound, redirect } from "next/navigation";
import { Badge, PageHeader, PageTitle, Section, SectionLabel, WakePulse } from "@agentsfleet/design-system";
import { getFleet, AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { listFleetEvents } from "@/lib/api/events";
import { listApprovals } from "@/lib/api/approvals";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import FleetApprovalsPanel from "@/components/domain/FleetApprovalsPanel";
import FleetThreadDynamic from "@/components/domain/FleetThreadDynamic";
import TriggerPanel from "./components/TriggerPanel";
import FleetConfig from "./components/FleetConfig";
import KillSwitch from "./components/KillSwitch";
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

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) notFound();

  const [fleet, billing, eventsPage, pendingApprovals] = await Promise.all([
    getFleet(workspace.id, id, token),
    getTenantBilling(token).catch(() => null),
    listFleetEvents(workspace.id, id, token, { limit: 20 }).catch(() => ({ items: [], next_cursor: null })),
    listApprovals(workspace.id, token, { fleetId: id, limit: 50 }).catch(() => ({ items: [], next_cursor: null })),
  ]);
  if (!fleet) notFound();

  // Per-trigger "last delivery" lookup. One lightweight server-side call
  // per declared trigger, in parallel; failures degrade to `null` (the
  // TriggerPanel renders "never"). Webhook actors are namespaced as
  // `webhook:<source>:*`; cron as `cron:*`.
  const triggerList = fleet.triggers ?? [];
  const lastDeliveryByKey = await resolveLastDeliveries(
    workspace.id,
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
            {fleet.status}
          </span>
          {billing?.is_exhausted ? (
            <ExhaustionBadge exhaustedAt={billing.exhausted_at} />
          ) : null}
          {hasPending ? (
            <Badge variant="destructive">{pendingCountLabel} pending approval{pendingApprovals.items.length === 1 ? "" : "s"}</Badge>
          ) : null}
        </div>
        <KillSwitch workspaceId={workspace.id} fleet={fleet} />
      </PageHeader>

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
            workspaceId={workspace.id}
            fleetId={fleet.id}
            fleetName={fleet.name}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Pending approvals">
          <SectionLabel>Pending approvals</SectionLabel>
          <FleetApprovalsPanel workspaceId={workspace.id} fleetId={fleet.id} token={token} />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Live activity">
          <SectionLabel>Live activity</SectionLabel>
          <FleetThreadDynamic
            workspaceId={workspace.id}
            fleetId={fleet.id}
            initial={eventsPage.items}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Recent Activity">
          <SectionLabel>Recent Activity</SectionLabel>
          <EventsList
            scope={{ kind: "fleet", workspaceId: workspace.id, fleetId: fleet.id }}
            initial={eventsPage}
          />
        </section>
      </Section>
    </div>
  );
}
