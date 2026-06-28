import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
  SectionLabel,
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
import type { FleetTemplate } from "@/lib/types";

export const dynamic = "force-dynamic";

export default async function FleetsListPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Shell-first: the header (title + Install link) paints immediately while the
  // workspace resolves and the list/billing load inside FleetsData. Mirrors the
  // home page's StatusTiles streaming.
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
  // Fetch the template catalogue only on the empty path (one-time onboarding), so
  // the populated list never pays for it. The await lives here in the async data
  // region; FleetsEmptyState stays sync so renderToStaticMarkup / React's sync
  // render never hit a nested async boundary.
  const templates =
    page.items.length === 0
      ? await listFleetTemplatesCached(token)
          .then((response) => response.items)
          .catch(() => [])
      : [];

  return (
    <>
      <ExhaustionBanner billing={billing} />
      {page.items.length === 0 ? (
        <FleetsEmptyState templates={templates} />
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

// Empty fleets → a full-width onboarding gallery. Reuses the InstallEntry picker
// so the first-run experience IS the real template gallery — spanning the page
// like Models & Keys — instead of abstract step cards. The catalogue is fetched
// by FleetsData (the async region); this stays a sync component. Besides the page
// header's "Install fleet" button, this gallery is the install affordance here.
function FleetsEmptyState({ templates }: { templates: FleetTemplate[] }) {
  return (
    <div className="space-y-6">
      <div className="space-y-1.5">
        <SectionLabel>No fleets yet</SectionLabel>
        <p className="max-w-prose text-body-sm leading-body-sm text-muted-foreground">
          Pick a template to install your first fleet — connect its tool and it runs on every
          matching event.
        </p>
      </div>
      <InstallEntry templates={templates} quickstart />
    </div>
  );
}
