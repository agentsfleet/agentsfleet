import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
  Skeleton,
} from "@agentsfleet/design-system";
import { listFleets } from "@/lib/api/fleets";
import { listFleetTemplatesCached } from "@/lib/api/fleet-bundles";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { withWorkspaceScope } from "@/lib/workspace";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { PlusIcon } from "lucide-react";
import FleetsList from "./components/FleetsList";
import { InstallEntry } from "./new/InstallEntry";

export const dynamic = "force-dynamic";

export default async function FleetsListPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Shell-first: the header (title + Install link) paints immediately while the
  // workspace resolves and the list/billing load inside FleetsData. Mirrors the
  // home page's StatusTiles/RecentActivity streaming split.
  return (
    <div>
      <PageHeader>
        <PageTitle>Fleets</PageTitle>
        <Link href="/fleets/new" className={buttonClassName("default", "sm")}>
          <PlusIcon size={14} /> Install fleet
        </Link>
      </PageHeader>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        <FleetsData />
      </Suspense>
    </div>
  );
}

// Async data region streamed under the shell. Resolves the active workspace
// from the cookie/JWT hint (no list round-trip on the hot path), then loads the
// fleet page + billing in one pass. Exported so it renders/tests in isolation.
export async function FleetsData() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const result = await withWorkspaceScope(token, async (workspaceId) => {
    const [page, billing] = await Promise.all([
      listFleets(workspaceId, token, { limit: 20 }),
      getTenantBillingCached(token).catch(() => null),
    ]);
    // Only the empty state needs the template gallery — fetch it lazily so a
    // populated list pays nothing for it.
    const templates =
      page.items.length === 0
        ? await listFleetTemplatesCached(token)
            .then((response) => response.items)
            .catch(() => [])
        : [];
    return { workspaceId, page, billing, templates };
  });
  if (!result) {
    return (
      <EmptyState
        title="No workspace yet"
        description="Create a workspace before installing teammates."
      />
    );
  }
  const { workspaceId, page, billing, templates } = result;

  return (
    <>
      <ExhaustionBanner billing={billing} />
      {page.items.length === 0 ? (
        <EmptyState
          title="Start your fleet"
          description="Install your first fleet to automate recurring work, then trigger it once to see events."
          action={
            <div className="w-full max-w-xl text-left">
              <InstallEntry templates={templates} quickstart />
            </div>
          }
        />
      ) : (
        <FleetsList
          workspaceId={workspaceId}
          initialFleets={page.items}
          initialCursor={page.cursor}
        />
      )}
    </>
  );
}
