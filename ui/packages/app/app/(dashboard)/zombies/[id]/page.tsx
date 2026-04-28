import { getServerToken } from "@/lib/auth/server";
import { notFound } from "next/navigation";
import { PageHeader, PageTitle, Section, SectionLabel } from "@usezombie/design-system";
import { getZombie } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { listZombieEvents } from "@/lib/api/events";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";
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

  const [zombie, billing, eventsPage] = await Promise.all([
    getZombie(workspace.id, id, token),
    getTenantBilling(token).catch(() => null),
    listZombieEvents(workspace.id, id, token, { limit: 20 }).catch(() => ({ items: [], next_cursor: null })),
  ]);
  if (!zombie) notFound();

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
