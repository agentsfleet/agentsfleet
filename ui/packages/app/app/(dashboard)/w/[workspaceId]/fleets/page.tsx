import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { PageHeader, PageLayout, PageTitle, Section, Skeleton } from "@agentsfleet/design-system";
import { listFleets } from "@/lib/api/fleets";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { getOnboarding, statusToInputs } from "@/lib/api/onboarding";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import FleetWall from "./components/FleetWall";
import GettingStarted from "./components/GettingStarted";

export const dynamic = "force-dynamic";

const FLEETS_DESCRIPTION =
  "A fleet is an always-on AI agent that wakes on events, gathers evidence, and acts with your approval.";

// The Wall — the workspace's only entry point (single-route refactor). With
// zero fleets it renders the Getting Started checklist as its empty state; with
// fleets it renders the tile grid. The page header adapts to which one shows,
// so it can't paint "Fleets" over a first-run checklist — that means the header
// waits on the data (no shell-first here), a deliberate trade for a truthful
// title on a route that is now two surfaces in one.
export default async function FleetsPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  return (
    <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
      <FleetsData workspaceId={workspaceId} />
    </Suspense>
  );
}

// Async data region: the fleet page + billing in one pass, keyed by the URL
// workspace. When empty, additionally gathers the onboarding signals so the
// checklist renders from live state. Exported so it renders/tests in isolation.
export async function FleetsData({ workspaceId }: { workspaceId: string }) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const [page, billing] = await Promise.all([
    listFleets(workspaceId, token, { limit: 20 }),
    getTenantBillingCached(token).catch(() => null),
  ]);

  if (page.items.length === 0) {
    // One call for the whole checklist — the onboarding endpoint derives every
    // signal server-side in a single query (Indy's consolidation).
    const inputs = statusToInputs(await getOnboarding(workspaceId, token));
    return (
      <>
        <ExhaustionBanner billing={billing} />
        <GettingStarted workspaceId={workspaceId} inputs={inputs} />
      </>
    );
  }

  return (
    <PageLayout>
      <PageHeader description={FLEETS_DESCRIPTION}>
        <PageTitle>Fleets</PageTitle>
      </PageHeader>
      <ExhaustionBanner billing={billing} />
      <Section asChild>
        <section aria-label="Fleets">
          <FleetWall
            workspaceId={workspaceId}
            initialFleets={page.items}
            initialCursor={page.cursor}
          />
        </section>
      </Section>
    </PageLayout>
  );
}
