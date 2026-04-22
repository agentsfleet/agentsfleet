import { getServerToken } from "@/lib/auth/server";
import Link from "next/link";
import { notFound } from "next/navigation";
import {
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
} from "@usezombie/design-system";
import { listZombies } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { resolveActiveWorkspace } from "@/lib/workspace";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { PlusIcon } from "lucide-react";
import ZombiesList from "./components/ZombiesList";

export const dynamic = "force-dynamic";

export default async function ZombiesListPage() {
  const token = await getServerToken();
  if (!token) notFound();

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) {
    return (
      <div>
        <PageHeader>
          <PageTitle>Zombies</PageTitle>
        </PageHeader>
        <EmptyState
          title="No workspace yet"
          description="Create a workspace before installing zombies."
        />
      </div>
    );
  }

  const [page, billing] = await Promise.all([
    listZombies(workspace.id, token, { limit: 20 }),
    getTenantBilling(token).catch(() => null),
  ]);

  return (
    <div>
      <ExhaustionBanner billing={billing} />
      <PageHeader>
        <PageTitle>Zombies</PageTitle>
        <Link
          href="/zombies/new"
          className={buttonClassName("default", "sm")}
        >
          <PlusIcon size={14} /> Install Zombie
        </Link>
      </PageHeader>

      {page.items.length === 0 ? (
        <EmptyState
          title="No zombies yet"
          description="Install your first zombie from a skill template."
        />
      ) : (
        <ZombiesList
          workspaceId={workspace.id}
          initialZombies={page.items}
          initialCursor={page.cursor}
        />
      )}
    </div>
  );
}
