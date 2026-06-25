import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import {
  Card,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
  StatusCard,
  Skeleton,
} from "@agentsfleet/design-system";
import { listFleets, AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { listFleetTemplatesCached } from "@/lib/api/fleet-bundles";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { NANOS_PER_USD } from "@/lib/types";
import type { FleetTemplate } from "@/lib/types";
import { listWorkspaceEvents } from "@/lib/api/events";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { InstallEntry } from "./fleets/new/InstallEntry";

export const dynamic = "force-dynamic";

export async function StatusTiles() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  // Request the server max (100) so the Active/Paused/Stopped tiles don't
  // silently under-report for workspaces above the 20-default page size.
  // A dedicated fleet-status summary endpoint will replace this client-side
  // rollup once it ships; until then 100 matches what the /fleets list uses.
  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [fleets, billing] = await Promise.all([
      listFleets(workspaceId, token, { limit: 100 }).then((r) => r.items).catch(orFallback([])),
      getTenantBillingCached(token).catch(() => null),
    ]);
    return { fleets, billing };
  });
  if (!result) return null;
  const { fleets, billing } = result;

  const active = fleets.filter((z) => z.status === AGENTSFLEET_STATUS.ACTIVE).length;
  const paused = fleets.filter((z) => z.status === AGENTSFLEET_STATUS.PAUSED).length;
  const stopped = fleets.filter((z) => z.status === AGENTSFLEET_STATUS.STOPPED).length;

  if (fleets.length === 0) {
    const templates = await listFleetTemplatesCached(token)
      .then((response) => response.items)
      .catch(() => []);
    return (
      <>
        <ExhaustionBanner billing={billing} />
        <FirstInstallCard balanceNanos={billing?.balance_nanos ?? null} templates={templates} />
      </>
    );
  }

  return (
    <>
      <ExhaustionBanner billing={billing} />
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 mb-6">
        <StatusCard label="Live" count={active} variant="success" sublabel={active > 0 ? "wake on event" : undefined} />
        <StatusCard label="Paused" count={paused} variant="warning" />
        <StatusCard label="Stopped" count={stopped} variant="default" />
        <StatusCard
          label="Balance"
          count={billing ? `$${(billing.balance_nanos / NANOS_PER_USD).toFixed(2)}` : "—"}
          variant={billing?.is_exhausted ? "danger" : "default"}
        />
      </div>
    </>
  );
}

// First-run card: composes the shared InstallEntry (one source for the install
// affordances), wrapped in the dashboard's "Start your fleet" framing. Each
// affordance deep-links into the install page, which proceeds inline to the
// live states — no review page.
function FirstInstallCard({
  balanceNanos,
  templates,
}: {
  balanceNanos: number | null;
  templates: FleetTemplate[];
}) {
  const credits = balanceNanos != null ? Math.floor(balanceNanos / NANOS_PER_USD) : null;
  return (
    <Section aria-label="Start your fleet" className="mb-8">
      <Card className="space-y-6 p-6 sm:p-8">
        <div className="space-y-3">
          <SectionLabel>Next step</SectionLabel>
          <h2 className="font-mono text-heading text-foreground">Start your fleet</h2>
          <p className="max-w-prose text-sm text-muted-foreground">
            Pick a template and it installs inline — you see each state. Each one
            installs from a single <code className="font-mono">SKILL.md</code>.
          </p>
          {credits != null && credits > 0 ? (
            <p className="text-sm text-muted-foreground">
              ${credits} free credit is ready for that first run.
            </p>
          ) : null}
        </div>

        <InstallEntry templates={templates} quickstart />
      </Card>
    </Section>
  );
}

export async function RecentActivity() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  // Dashboard shows a short preview; the full, paginated stream lives at
  // /events (the sidebar "Events" item). Keeps the two from duplicating.
  const result = await withWorkspaceScope(token, async (workspaceId) => ({
    workspaceId,
    page: await listWorkspaceEvents(workspaceId, token, { limit: 5 }).catch(
      orFallback({ items: [], next_cursor: null }),
    ),
  }));
  if (!result) return null;
  const { workspaceId, page } = result;

  return (
    <Section asChild>
      <section aria-label="Recent Activity">
        <SectionLabel>Recent Activity</SectionLabel>
        <EventsList
          scope={{ kind: "workspace", workspaceId }}
          initial={page}
          viewAllHref="/events"
        />
      </section>
    </Section>
  );
}

export default async function DashboardPage() {
  const { getToken } = await auth();
  const token = await getToken();
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
