import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
} from "@agentsfleet/design-system";
import { listFleets } from "@/lib/api/fleets";
import { listFleetTemplatesCached } from "@/lib/api/fleet-bundles";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { resolveActiveWorkspace } from "@/lib/workspace";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { PlusIcon } from "lucide-react";
import FleetsList from "./components/FleetsList";
import { InstallEntry } from "./new/InstallEntry";

export const dynamic = "force-dynamic";

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

  // Only the empty state needs the template gallery — fetch it lazily so a
  // populated list pays nothing for it.
  const templates =
    page.items.length === 0
      ? await listFleetTemplatesCached(token)
          .then((response) => response.items)
          .catch(() => [])
      : [];

  return (
    <div>
      <ExhaustionBanner billing={billing} />
      <PageHeader>
        <PageTitle>Fleets</PageTitle>
        <Link
          href="/fleets/new"
          className={buttonClassName("default", "sm")}
        >
          <PlusIcon size={14} /> Install fleet
        </Link>
      </PageHeader>

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
          workspaceId={workspace.id}
          initialFleets={page.items}
          initialCursor={page.cursor}
        />
      )}
    </div>
  );
}
