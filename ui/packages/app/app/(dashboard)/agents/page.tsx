import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
} from "@agentsfleet/design-system";
import { listAgents } from "@/lib/api/agents";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { AGENT_DEFINITION } from "@/lib/copy";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import { PlusIcon } from "lucide-react";
import AgentsList from "./components/AgentsList";

export const dynamic = "force-dynamic";

export default async function AgentsListPage() {
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
    listAgents(workspace.id, token, { limit: 20 }),
    getTenantBilling(token).catch(() => null),
  ]);

  return (
    <div>
      <ExhaustionBanner billing={billing} />
      <PageHeader>
        <PageTitle>Agents</PageTitle>
        <Link
          href="/agents/new"
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
        <AgentsList
          workspaceId={workspace.id}
          initialAgents={page.items}
          initialCursor={page.cursor}
        />
      )}
    </div>
  );
}
