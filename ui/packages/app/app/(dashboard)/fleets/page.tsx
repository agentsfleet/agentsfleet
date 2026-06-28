import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  Button,
  buttonClassName,
  DashboardPanel,
  DashboardPanelContent,
  DashboardPanelDescription,
  DashboardPanelHeader,
  DashboardPanelTitle,
  EmptyState,
  PageHeader,
  PageTitle,
  SectionLabel,
  Skeleton,
} from "@agentsfleet/design-system";
import { listFleets } from "@/lib/api/fleets";
import { getTenantBillingCached } from "@/lib/api/tenant_billing";
import { withWorkspaceScope } from "@/lib/workspace";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { PlusIcon } from "lucide-react";
import FleetsList from "./components/FleetsList";

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

  return (
    <>
      <ExhaustionBanner billing={billing} />
      {page.items.length === 0 ? (
        <DashboardPanel padding="compact" className="max-w-3xl">
          <DashboardPanelHeader>
            <div className="space-y-2">
              <SectionLabel>No fleets yet</SectionLabel>
              <DashboardPanelTitle>Install your first fleet</DashboardPanelTitle>
              <DashboardPanelDescription className="max-w-prose">
                Pick a template, connect the tool, and watch it wake.
              </DashboardPanelDescription>
            </div>
            <Button asChild>
              <Link href="/fleets/new">
                <PlusIcon size={16} /> Install fleet
              </Link>
            </Button>
          </DashboardPanelHeader>
          <DashboardPanelContent className="grid gap-md sm:grid-cols-3">
            {["Choose template", "Connect the tool", "Watch it wake"].map((item, index) => (
              <div key={item} className="rounded-md border border-border bg-secondary p-md">
                <div className="font-mono text-eyebrow text-pulse">0{index + 1}</div>
                <div className="mt-2 font-medium text-foreground">{item}</div>
              </div>
            ))}
          </DashboardPanelContent>
        </DashboardPanel>
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
