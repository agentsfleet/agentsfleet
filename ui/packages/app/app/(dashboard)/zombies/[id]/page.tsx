import { getServerToken } from "@/lib/auth/server";
import { notFound } from "next/navigation";
import { Badge, PageHeader, PageTitle, Section, SectionLabel } from "@usezombie/design-system";
import { getZombie } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { listZombieEvents } from "@/lib/api/events";
import { listApprovals } from "@/lib/api/approvals";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";
import { LiveEventsPanel } from "@/components/domain/LiveEventsPanel";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
import ZombieApprovalsPanel from "@/components/domain/ZombieApprovalsPanel";
import TriggerPanel from "./components/TriggerPanel";
import FirewallRulesEditor from "./components/FirewallRulesEditor";
import ZombieConfig from "./components/ZombieConfig";
import KillSwitch from "./components/KillSwitch";

export const dynamic = "force-dynamic";

export default async function ZombieDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const token = await getServerToken();
  if (!token) notFound();

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) notFound();

  const [zombie, billing, eventsPage, pendingApprovals] = await Promise.all([
    getZombie(workspace.id, id, token),
    getTenantBilling(token).catch(() => null),
    listZombieEvents(workspace.id, id, token, { limit: 20 }).catch(() => ({ items: [], next_cursor: null })),
    listApprovals(workspace.id, token, { zombieId: id, limit: 50 }).catch(() => ({ items: [], next_cursor: null })),
  ]);
  if (!zombie) notFound();
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
          <PageTitle>{zombie.name}</PageTitle>
          <span className="text-xs uppercase tracking-wide text-muted-foreground">
            {zombie.status}
          </span>
          {billing?.is_exhausted ? (
            <ExhaustionBadge exhaustedAt={billing.exhausted_at} />
          ) : null}
          {hasPending ? (
            <Badge variant="destructive">{pendingCountLabel} pending approval{pendingApprovals.items.length === 1 ? "" : "s"}</Badge>
          ) : null}
        </div>
        <KillSwitch workspaceId={workspace.id} zombie={zombie} />
      </PageHeader>

      <Section asChild>
        <section aria-label="Trigger">
          <SectionLabel>Trigger</SectionLabel>
          <TriggerPanel zombieId={zombie.id} />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Firewall rules">
          <SectionLabel>Firewall rules</SectionLabel>
          <FirewallRulesEditor />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Configuration">
          <SectionLabel>Configuration</SectionLabel>
          <ZombieConfig
            workspaceId={workspace.id}
            zombieId={zombie.id}
            zombieName={zombie.name}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Pending approvals">
          <SectionLabel>Pending approvals</SectionLabel>
          <ZombieApprovalsPanel workspaceId={workspace.id} zombieId={zombie.id} token={token} />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Live activity">
          <SectionLabel>Live activity</SectionLabel>
          <LiveEventsPanel workspaceId={workspace.id} zombieId={zombie.id} />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Recent Activity">
          <SectionLabel>Recent Activity</SectionLabel>
          <EventsList
            scope={{ kind: "zombie", workspaceId: workspace.id, zombieId: zombie.id }}
            initial={eventsPage}
          />
        </section>
      </Section>
    </div>
  );
}
