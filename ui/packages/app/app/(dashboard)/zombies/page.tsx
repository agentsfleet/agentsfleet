import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
} from "@usezombie/design-system";
import { listZombies } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { AGENT_DEFINITION } from "@/lib/copy";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { PlusIcon } from "lucide-react";
import ZombiesList from "./components/ZombiesList";

export const dynamic = "force-dynamic";

export default async function ZombiesListPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) {
    return (
      <div>
        <PageHeader>
          <PageTitle>Agents</PageTitle>
        </PageHeader>
        <EmptyState
          title="No workspace yet"
          description="Create a workspace before installing agents."
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
        <PageTitle>Agents</PageTitle>
        <Link
          href="/zombies/new"
          className={buttonClassName("default", "sm")}
        >
          <PlusIcon size={14} /> Install Agent
        </Link>
      </PageHeader>

      {page.items.length === 0 ? (
        <EmptyState
          title="No agents yet"
          description={`${AGENT_DEFINITION} Install your first one from a skill template.`}
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
