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
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { workspacePath } from "@/lib/workspace-routes";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { BotIcon, PlusIcon } from "lucide-react";
import FleetsList from "./components/FleetsList";

export const dynamic = "force-dynamic";

const FLEETS_DESCRIPTION = "Fleets installed in this workspace, and their live state.";
const FLEETS_DOC_URL = "https://docs.agentsfleet.net/fleets/overview";

export default async function FleetsListPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Shell-first: the header paints immediately while the list/billing load
  // inside FleetsData. Mirrors the home page's StatusTiles streaming. The
  // Install affordance lives with the content it acts on: the list toolbar when
  // fleets exist, the empty state's primary action otherwise.
  return (
    <div>
      <PageHeader description={FLEETS_DESCRIPTION}>
        <PageTitle>Fleets</PageTitle>
      </PageHeader>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        <FleetsData workspaceId={workspaceId} />
      </Suspense>
    </div>
  );
}

// Async data region streamed under the shell: the fleet page + billing in one
// pass, keyed by the URL workspace. Exported so it renders/tests in isolation.
export async function FleetsData({ workspaceId }: { workspaceId: string }) {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const [page, billing] = await Promise.all([
    listFleets(workspaceId, token, { limit: 20 }),
    getTenantBillingCached(token).catch(() => null),
  ]);

  return (
    <>
      <ExhaustionBanner billing={billing} />
      {page.items.length === 0 ? (
        <FleetsEmptyState workspaceId={workspaceId} />
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

// Empty fleets → a centered EmptyState: icon, headline, one line of context,
// then [Learn more] + the primary Install affordance. The library gallery
// itself lives on /fleets/new (the Install fleet button routes there), so the
// first-run screen stays calm rather than rendering the full picker inline.
function FleetsEmptyState({ workspaceId }: { workspaceId: string }) {
  return (
    <EmptyState
      icon={<BotIcon size={28} />}
      title="No fleets yet"
      description="Pick from the fleet library to install your first fleet."
      action={
        <div className="flex flex-wrap items-center justify-center gap-md">
          <a
            href={FLEETS_DOC_URL}
            target="_blank"
            rel="noopener noreferrer"
            className={buttonClassName("outline", "sm")}
          >
            Learn more
          </a>
          <Link href={workspacePath(workspaceId, "fleets/new")} className={buttonClassName("default", "sm")}>
            <PlusIcon size={14} /> Install fleet
          </Link>
        </div>
      }
    />
  );
}
