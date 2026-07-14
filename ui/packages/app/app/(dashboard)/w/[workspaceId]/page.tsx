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
import { countFleets } from "@/lib/fleet-rollup";
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

  // Total over the status registry, not three hand-picked filters. The tiles used
  // to count active/paused/stopped and nothing else, so an `installing` or
  // `killed` fleet appeared in NO tile — present in the workspace, absent from the
  // summary that claims to describe it.
  const counts = countFleets(fleets);
  const active = counts.byStatus[AGENTSFLEET_STATUS.ACTIVE];
  const paused = counts.byStatus[AGENTSFLEET_STATUS.PAUSED];
  const stopped = counts.byStatus[AGENTSFLEET_STATUS.STOPPED];
  const installing = counts.byStatus[AGENTSFLEET_STATUS.INSTALLING];
  const killed = counts.byStatus[AGENTSFLEET_STATUS.KILLED];

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
        {/* Installing and Killed appear only when they have fleets to report. A
            zero tile hides nothing; a MISSING tile with a non-zero count is what
            the dashboard was doing, and that is the bug. */}
        {installing > 0 ? (
          <StatusCard label="Installing" count={installing} variant="default" sublabel="provisioning" />
        ) : null}
        <StatusCard label="Paused" count={paused} variant="warning" />
        <StatusCard label="Stopped" count={stopped} variant="default" />
        {killed > 0 ? <StatusCard label="Killed" count={killed} variant="danger" /> : null}
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
