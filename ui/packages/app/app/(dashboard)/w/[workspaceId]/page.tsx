import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import {
  PageHeader,
  PageTitle,
  Section,
  StatusCard,
  StatusPill,
  Skeleton,
} from "@agentsfleet/design-system";
import { listFleets, AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { listWorkspaceFleetLibraryCached } from "@/lib/api/fleet-library";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { NANOS_PER_USD } from "@/lib/types";
import type { FleetLibraryGalleryEntry } from "@/lib/types";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { InstallEntry } from "./fleets/new/InstallEntry";
import { hasLibraryWriteScope } from "./fleets/scope";

export const dynamic = "force-dynamic";

export async function StatusTiles({ workspaceId }: { workspaceId: string }) {
  const { getToken, sessionClaims } = await auth();
  const token = await getToken();
  if (!token) return null;

  // Request the server max (100) so the Active/Paused/Stopped tiles don't
  // silently under-report for workspaces above the 20-default page size.
  // A dedicated fleet-status summary endpoint will replace this client-side
  // rollup once it ships; until then 100 matches what the /fleets list uses.
  const [fleets, billing] = await Promise.all([
    listFleets(workspaceId, token, { limit: 100 }).then((r) => r.items).catch(() => []),
    getTenantBillingCached(token).catch(() => null),
  ]);

  const active = fleets.filter((z) => z.status === AGENTSFLEET_STATUS.ACTIVE).length;
  const paused = fleets.filter((z) => z.status === AGENTSFLEET_STATUS.PAUSED).length;
  const stopped = fleets.filter((z) => z.status === AGENTSFLEET_STATUS.STOPPED).length;

  if (fleets.length === 0) {
    const entries = await listWorkspaceFleetLibraryCached(workspaceId, token)
      .then((response) => response.items)
      .catch(() => []);
    return (
      <>
        <ExhaustionBanner billing={billing} />
        <FirstInstall
          workspaceId={workspaceId}
          balanceNanos={billing?.balance_nanos ?? null}
          entries={entries}
          canAddLibraryEntry={hasLibraryWriteScope(sessionClaims)}
        />
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

// First-run surface: the shared InstallEntry rendered plainly under the page
// header — no wrapping panel (its title just repeated the page description)
// and no side guide; the empty state / gallery carries the actions itself.
// Each affordance deep-links into the install page, which proceeds inline to
// the live states — no review page. The free-credit pill is the one place the
// starter credit is announced.
function FirstInstall({
  workspaceId,
  balanceNanos,
  entries,
  canAddLibraryEntry,
}: {
  workspaceId: string;
  balanceNanos: number | null;
  entries: FleetLibraryGalleryEntry[];
  canAddLibraryEntry: boolean;
}) {
  const credits = balanceNanos != null ? Math.floor(balanceNanos / NANOS_PER_USD) : null;
  return (
    <Section aria-label="Start your fleet" className="mb-6 space-y-md">
      {credits != null && credits > 0 ? (
        <div className="flex justify-end">
          <StatusPill variant="pulse">${credits} free credit ready</StatusPill>
        </div>
      ) : null}
      <InstallEntry
        workspaceId={workspaceId}
        entries={entries}
        maxEntries={3}
        compact
        canAddLibraryEntry={canAddLibraryEntry}
      />
    </Section>
  );
}

export default async function DashboardPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  return (
    <div>
      <PageHeader description="Start a fleet from the prebuilt fleet library.">
        <PageTitle>Dashboard</PageTitle>
      </PageHeader>

      <Suspense
        fallback={
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 mb-6">
            {[0, 1, 2, 3].map((i) => <Skeleton key={i} className="h-20 rounded-lg" />)}
          </div>
        }
      >
        <StatusTiles workspaceId={workspaceId} />
      </Suspense>
    </div>
  );
}
