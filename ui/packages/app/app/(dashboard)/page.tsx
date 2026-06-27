import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import {
  DashboardPanel,
  DashboardPanelContent,
  DashboardPanelDescription,
  DashboardPanelHeader,
  DashboardPanelTitle,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
  StatusCard,
  StatusPill,
  Skeleton,
} from "@agentsfleet/design-system";
import { listFleets, AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { listFleetTemplatesCached } from "@/lib/api/fleet-bundles";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { NANOS_PER_USD } from "@/lib/types";
import type { FleetTemplate } from "@/lib/types";
import { withWorkspaceScope, orFallback } from "@/lib/workspace";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { InstallEntry } from "./fleets/new/InstallEntry";
import { InstallFlowGuide } from "./fleets/new/InstallFlowGuide";

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
    <Section aria-label="Start your fleet" className="mb-6">
      <div className="grid grid-cols-1 gap-lg lg:grid-cols-3">
        <DashboardPanel padding="compact" className="lg:col-span-2">
          <DashboardPanelHeader>
            <div className="space-y-2">
              <SectionLabel>Next step</SectionLabel>
              <DashboardPanelTitle>Start your fleet</DashboardPanelTitle>
              <DashboardPanelDescription className="max-w-prose">
                Pick a template. Watch install states.
              </DashboardPanelDescription>
            </div>
            {credits != null && credits > 0 ? (
              <StatusPill variant="pulse">${credits} free credit ready</StatusPill>
            ) : null}
          </DashboardPanelHeader>

          <DashboardPanelContent className="mt-md">
            <InstallEntry templates={templates} maxTemplates={3} compact showSourceActions={false} />
          </DashboardPanelContent>
        </DashboardPanel>

        <InstallFlowGuide />
      </div>
    </Section>
  );
}

export default async function DashboardPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  return (
    <div>
      <PageHeader description="Pick a template. Starter credit is ready.">
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
    </div>
  );
}
