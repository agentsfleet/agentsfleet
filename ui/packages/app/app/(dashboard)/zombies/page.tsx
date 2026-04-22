import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import {
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
} from "@usezombie/design-system";
import { listWorkspaces } from "@/lib/api";
import { listZombies } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { PlusIcon } from "lucide-react";

export const dynamic = "force-dynamic";

// Resolves "current workspace" as the first one from the list. The proper
// workspace switcher belongs to the dashboard-pages workstream; keeping
// this lightweight so the install flow has a destination today.
export default async function ZombiesListPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const workspaces = await listWorkspaces(token).then((r) => r.data);
  const workspace = workspaces[0];
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

  const [zombies, billing] = await Promise.all([
    listZombies(workspace.id, token).then((r) => r.items),
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

      {zombies.length === 0 ? (
        <EmptyState
          title="No zombies yet"
          description="Install your first zombie from a skill template."
        />
      ) : (
        <ul className="divide-y divide-border rounded-lg border border-border">
          {zombies.map((z) => (
            <li key={z.id}>
              <Link
                href={`/zombies/${z.id}`}
                className="flex items-center justify-between px-4 py-3 hover:bg-muted/40"
              >
                <div>
                  <div className="font-medium">{z.name}</div>
                  <div className="text-xs text-muted-foreground">
                    {z.skill}
                  </div>
                </div>
                <div className="font-mono text-xs text-muted-foreground">
                  {z.id}
                </div>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
