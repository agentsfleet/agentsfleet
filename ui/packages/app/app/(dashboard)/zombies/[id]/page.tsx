import { getServerToken } from "@/lib/auth/server";
import { notFound } from "next/navigation";
import { PageHeader, PageTitle, SectionLabel } from "@usezombie/design-system";
import { getZombie } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { listZombieActivity } from "@/lib/api/activity";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { ActivityFeed } from "@/components/domain/ActivityFeed";
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

  const [zombie, billing, activityPage] = await Promise.all([
    getZombie(workspace.id, id, token),
    getTenantBilling(token).catch(() => null),
    listZombieActivity(workspace.id, id, token).catch(() => null),
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

      <section className="mb-8">
        <SectionLabel>Trigger</SectionLabel>
        <TriggerPanel zombieId={zombie.id} />
      </section>

      <section className="mb-8">
        <SectionLabel>Firewall rules</SectionLabel>
        <FirewallRulesEditor />
      </section>

      <section className="mb-8">
        <SectionLabel>Configuration</SectionLabel>
        <ZombieConfig
          workspaceId={workspace.id}
          zombieId={zombie.id}
          zombieName={zombie.name}
        />
      </section>

      {activityPage && activityPage.events.length > 0 && (
        <section className="mb-8">
          <SectionLabel>Recent Activity</SectionLabel>
          <ActivityFeed events={activityPage.events} />
        </section>
      )}
    </div>
  );
}
