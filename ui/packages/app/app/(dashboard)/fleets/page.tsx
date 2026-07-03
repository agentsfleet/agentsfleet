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
import { withWorkspaceScope } from "@/lib/workspace";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { BotIcon, PlusIcon } from "lucide-react";
import FleetsList from "./components/FleetsList";

export const dynamic = "force-dynamic";

const FLEETS_DESCRIPTION = "Fleets installed in this workspace, and their live state.";
const FLEETS_DOC_URL = "https://docs.agentsfleet.net/fleets/overview";

export default async function FleetsListPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Shell-first: the header paints immediately while the workspace resolves and
  // the list/billing load inside FleetsData. Mirrors the home page's StatusTiles
  // streaming. The Install affordance lives with the content it acts on: the
  // list toolbar when fleets exist, the empty state's primary action otherwise —
  // never duplicated in the header corner.
  return (
    <div>
      <PageHeader description={FLEETS_DESCRIPTION}>
        <PageTitle>Fleets</PageTitle>
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
    return { workspaceId, page, billing };
  });
  if (!result) {
    return (
      <EmptyState
        title="No workspace yet"
        description="Create a workspace first."
      />
    );
  }
  const { workspaceId, page, billing } = result;

  return (
    <>
      <ExhaustionBanner billing={billing} />
      {page.items.length === 0 ? (
        <FleetsEmptyState />
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
// then [Learn more] + the primary Install affordance. The template gallery
// itself lives on /fleets/new (the Install fleet button routes there), so the
// first-run screen stays calm rather than rendering the full picker inline.
function FleetsEmptyState() {
  return (
    <EmptyState
      icon={<BotIcon size={28} />}
      title="No fleets yet"
      description="Pick a template to install your first fleet — connect its tool and it runs on every matching event."
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
          <Link href="/fleets/new" className={buttonClassName("default", "sm")}>
            <PlusIcon size={14} /> Install fleet
          </Link>
        </div>
      }
    />
  );
}
