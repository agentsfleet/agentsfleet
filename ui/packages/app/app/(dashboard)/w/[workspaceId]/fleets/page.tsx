import { Suspense } from "react";
import { credential, requireCredential } from "@/lib/auth/credential";
import { PageLayout, Section } from "@agentsfleet/design-system";
import { listFleets } from "@/lib/api/fleets";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import FleetWall from "./components/FleetWall";
import { NewFleetEntrance } from "./components/NewFleetEntrance";
import FleetsLoading from "./loading";

export const dynamic = "force-dynamic";


// Every workspace opens the fleet dashboard, with chat creation above its wall.
export default async function FleetsPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  await requireCredential();

  return (
    <Suspense fallback={<FleetsLoading />}>
      <FleetsData workspaceId={workspaceId} />
    </Suspense>
  );
}

// Async data region: the fleet page + billing in one pass, keyed by the URL
// workspace. Exported so it renders/tests in isolation.
export async function FleetsData({ workspaceId }: { workspaceId: string }) {
  const token = await credential();
  if (!token) return null;

  const [page, billing] = await Promise.all([
    listFleets(workspaceId, token, { limit: 20 }),
    getTenantBillingCached(token).catch(() => null),
  ]);

  return (
    <PageLayout>
      <NewFleetEntrance workspaceId={workspaceId} />
      <ExhaustionBanner billing={billing} />
      <Section asChild>
        <section aria-label="Fleets">
          <FleetWall
            workspaceId={workspaceId}
            initialFleets={page.items}
            initialCursor={page.next_cursor}
          />
        </section>
      </Section>
    </PageLayout>
  );
}
