import { Suspense } from "react";
import { getServerToken } from "@/lib/auth/server";
import { redirect } from "next/navigation";
import { PageHeader, PageTitle, Section, SectionLabel, StatusCard, Skeleton } from "@usezombie/design-system";
import { listZombies } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { listWorkspaceEvents } from "@/lib/api/events";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";

export const dynamic = "force-dynamic";

export async function StatusTiles() {
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

export async function RecentActivity() {
  const token = await getServerToken();
  if (!token) return null;

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) return null;

  const page = await listWorkspaceEvents(workspace.id, token, { limit: 20 }).catch(
    () => ({ items: [], next_cursor: null }),
  );

  return (
    <Section asChild>
      <section aria-label="Recent Activity">
        <SectionLabel>Recent Activity</SectionLabel>
        <EventsList scope={{ kind: "workspace", workspaceId: workspace.id }} initial={page} />
      </section>
    </Section>
  );
}

export default async function DashboardPage() {
  const token = await getServerToken();
  if (!token) redirect("/sign-in");

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
