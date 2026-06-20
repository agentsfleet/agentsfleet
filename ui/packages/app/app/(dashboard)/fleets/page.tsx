import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  Button,
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
} from "@agentsfleet/design-system";
import { listFleets } from "@/lib/api/fleets";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { resolveActiveWorkspace } from "@/lib/workspace";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { PlusIcon } from "lucide-react";
import FleetsList from "./components/FleetsList";

export const dynamic = "force-dynamic";

const QUICKSTART_URL = "https://docs.agentsfleet.net/quickstart";

export default async function FleetsListPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) {
    return (
      <div>
        <PageHeader>
          <PageTitle>Fleets</PageTitle>
        </PageHeader>
        <EmptyState
          title="No workspace yet"
          description="Create a workspace before installing teammates."
        />
      </div>
    );
  }

  const [page, billing] = await Promise.all([
    listFleets(workspace.id, token, { limit: 20 }),
    getTenantBilling(token).catch(() => null),
  ]);

  return (
    <div>
      <ExhaustionBanner billing={billing} />
      <PageHeader>
        <PageTitle>Fleets</PageTitle>
        <Link
          href="/fleets/new"
          className={buttonClassName("default", "sm")}
        >
          <PlusIcon size={14} /> Install teammate
        </Link>
      </PageHeader>

      {page.items.length === 0 ? (
        <EmptyState
          title="Start your fleet"
          description="Install your first teammate to automate recurring work, then trigger it once to see events."
          action={
            <div className="flex flex-wrap justify-center gap-2">
              <Button asChild size="sm">
                <Link href="/fleets/new">Install teammate</Link>
              </Button>
              <Button asChild variant="ghost" size="sm">
                <a href={QUICKSTART_URL} target="_blank" rel="noopener noreferrer">
                  Quick start
                </a>
              </Button>
            </div>
          }
        />
      ) : (
        <FleetsList
          workspaceId={workspace.id}
          initialFleets={page.items}
          initialCursor={page.cursor}
        />
      )}
    </div>
  );
}
