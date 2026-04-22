import { Suspense } from "react";
import { getServerToken } from "@/lib/auth/server";
import { notFound } from "next/navigation";
import { EmptyState, PageHeader, PageTitle, StatusCard, Skeleton } from "@usezombie/design-system";
import { listZombies } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { listWorkspaceActivity } from "@/lib/api/activity";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { ActivityFeed } from "@/components/domain/ActivityFeed";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";

export const dynamic = "force-dynamic";

async function StatusTiles() {
  const token = await getServerToken();
  if (!token) return null;

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) return null;

  // Request the server max (100) so the Active/Paused/Stopped tiles don't
  // silently under-report for workspaces above the 20-default page size.
  // A dedicated summary endpoint will replace this client-side rollup once it
  // ships; until then 100 matches what the /zombies list page uses.
  const [zombies, billing] = await Promise.all([
    listZombies(workspace.id, token, { limit: 100 }).then((r) => r.items).catch(() => []),
    getTenantBilling(token).catch(() => null),
  ]);

  const active = zombies.filter((z) => z.status === "active").length;
  const paused = zombies.filter((z) => z.status === "paused").length;
  const stopped = zombies.filter((z) => z.status === "stopped").length;

  return (
    <>
      <ExhaustionBanner billing={billing} />
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 mb-6">
        <StatusCard label="Active" count={active} variant="success" />
        <StatusCard label="Paused" count={paused} variant="warning" />
        <StatusCard label="Stopped" count={stopped} variant="default" />
        <StatusCard
          label="Balance"
          count={billing ? `${billing.balance_cents / 100} credits` : "—"}
          variant={billing?.is_exhausted ? "danger" : "default"}
        />
      </div>
    </>
  );
}

async function RecentActivity() {
  const token = await getServerToken();
  if (!token) return null;

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) return null;

  const page = await listWorkspaceActivity(workspace.id, token).catch(() => null);
  if (!page) return null;

  return (
    <ActivityFeed
      events={page.events}
      title="Recent Activity"
      empty={<EmptyState title="No activity yet" description="Events appear here as your zombies run." />}
    />
  );
}

export default async function DashboardPage() {
  const token = await getServerToken();
  if (!token) notFound();

  return (
    <div>
      <PageHeader>
        <PageTitle>Dashboard</PageTitle>
      </PageHeader>

      <Suspense
        fallback={
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 mb-6">
            {[0, 1, 2, 3].map((i) => <Skeleton key={i} className="h-20 rounded-lg" />)}
          </div>
        }
      >
        <StatusTiles />
      </Suspense>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        <RecentActivity />
      </Suspense>
    </div>
  );
}
