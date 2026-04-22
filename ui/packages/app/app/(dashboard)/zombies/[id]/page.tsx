import { auth } from "@clerk/nextjs/server";
import { notFound } from "next/navigation";
import { PageHeader, PageTitle, SectionLabel } from "@usezombie/design-system";
import { getZombie } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { resolveActiveWorkspace } from "@/lib/workspace";
import TriggerPanel from "./components/TriggerPanel";
import FirewallRulesEditor from "./components/FirewallRulesEditor";
import ZombieConfig from "./components/ZombieConfig";
import ExhaustionBadge from "@/components/domain/ExhaustionBadge";

export const dynamic = "force-dynamic";

// Minimal detail stub composing the lifecycle panels (trigger, firewall,
// config). The richer detail page (status header, kill switch, spend panel,
// activity feed) is owned by the dashboard-pages workstream and will
// replace this file while continuing to import the same panel components.
export default async function ZombieDetailStub({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) notFound();

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) notFound();

  const [zombie, billing] = await Promise.all([
    getZombie(workspace.id, id, token),
    getTenantBilling(token).catch(() => null),
  ]);
  if (!zombie) notFound();

  return (
    <div>
      <PageHeader>
        <div className="flex items-center gap-3">
          <PageTitle>{zombie.name}</PageTitle>
          {billing?.is_exhausted ? (
            <ExhaustionBadge exhaustedAt={billing.exhausted_at} />
          ) : null}
        </div>
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
    </div>
  );
}
