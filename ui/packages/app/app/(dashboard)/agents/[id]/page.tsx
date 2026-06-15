import { auth } from "@clerk/nextjs/server";
import { notFound, redirect } from "next/navigation";
import { Badge, PageHeader, PageTitle, Section, SectionLabel, WakePulse } from "@agentsfleet/design-system";
import { getAgent, AGENTSFLEET_STATUS } from "@/lib/api/agents";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { listAgentEvents } from "@/lib/api/events";
import { listApprovals } from "@/lib/api/approvals";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import AgentApprovalsPanel from "@/components/domain/AgentApprovalsPanel";
import AgentThreadDynamic from "@/components/domain/AgentThreadDynamic";
import TriggerPanel from "./components/TriggerPanel";
import AgentConfig from "./components/AgentConfig";
import KillSwitch from "./components/KillSwitch";
import { resolveLastDeliveries } from "./components/last-delivery";

export const dynamic = "force-dynamic";

export default async function AgentDetailPage({
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

  const [agent, billing, eventsPage, pendingApprovals] = await Promise.all([
    getAgent(workspace.id, id, token),
    getTenantBilling(token).catch(() => null),
    listAgentEvents(workspace.id, id, token, { limit: 20 }).catch(() => ({ items: [], next_cursor: null })),
    listApprovals(workspace.id, token, { agentId: id, limit: 50 }).catch(() => ({ items: [], next_cursor: null })),
  ]);
  if (!agent) notFound();

  // Per-trigger "last delivery" lookup. One lightweight server-side call
  // per declared trigger, in parallel; failures degrade to `null` (the
  // TriggerPanel renders "never"). Webhook actors are namespaced as
  // `webhook:<source>:*`; cron as `cron:*`.
  const triggerList = agent.triggers ?? [];
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
          <PageTitle>{agent.name}</PageTitle>
          <span className="inline-flex items-center gap-2 font-mono text-label uppercase tracking-label text-muted-foreground" data-state={agent.status}>
            {agent.status === AGENTSFLEET_STATUS.ACTIVE ? (
              <WakePulse
                live
                className="inline-block w-2 h-2 rounded-full bg-pulse"
                aria-hidden="true"
              />
            ) : null}
            {agent.status}
          </span>
          {billing?.is_exhausted ? (
            <ExhaustionBadge exhaustedAt={billing.exhausted_at} />
          ) : null}
          {hasPending ? (
            <Badge variant="destructive">{pendingCountLabel} pending approval{pendingApprovals.items.length === 1 ? "" : "s"}</Badge>
          ) : null}
        </div>
        <KillSwitch workspaceId={workspace.id} agent={agent} />
      </PageHeader>

      <Section asChild>
        <section aria-label="Trigger">
          <SectionLabel>Trigger</SectionLabel>
          <TriggerPanel
            agentId={agent.id}
            triggers={triggerList}
            lastDeliveryByKey={lastDeliveryByKey}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Configuration">
          <SectionLabel>Configuration</SectionLabel>
          <AgentConfig
            workspaceId={workspace.id}
            agentId={agent.id}
            agentName={agent.name}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Pending approvals">
          <SectionLabel>Pending approvals</SectionLabel>
          <AgentApprovalsPanel workspaceId={workspace.id} agentId={agent.id} token={token} />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Live activity">
          <SectionLabel>Live activity</SectionLabel>
          <AgentThreadDynamic
            workspaceId={workspace.id}
            agentId={agent.id}
            initial={eventsPage.items}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Recent Activity">
          <SectionLabel>Recent Activity</SectionLabel>
          <EventsList
            scope={{ kind: "agent", workspaceId: workspace.id, agentId: agent.id }}
            initial={eventsPage}
          />
        </section>
      </Section>
    </div>
  );
}
