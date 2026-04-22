import { auth } from "@clerk/nextjs/server";
import {
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
} from "@usezombie/design-system";
import { listWorkspaces } from "@/lib/api";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import WorkspaceCard from "@/components/domain/WorkspaceCard";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";
import AnalyticsPageEvent from "@/components/analytics/AnalyticsPageEvent";
import TrackedAnchor from "@/components/analytics/TrackedAnchor";
import { PlusIcon } from "lucide-react";

export const dynamic = "force-dynamic";

export default async function WorkspacesPage() {
  const { getToken } = await auth();
  const token = await getToken();

  let workspaces = null;
  let error: string | null = null;

  try {
    if (token) {
      const res = await listWorkspaces(token);
      workspaces = res.data;
    }
  } catch (e) {
    error = e instanceof Error ? e.message : "Failed to load workspaces";
  }

  const billing = token
    ? await getTenantBilling(token).catch(() => null)
    : null;

  return (
    <div>
      <ExhaustionBanner billing={billing} />
      <AnalyticsPageEvent
        event="workspace_list_viewed"
        properties={{
          source: "workspaces_page",
          surface: "workspace_list",
          workspace_count: workspaces?.length ?? 0,
          has_error: Boolean(error),
        }}
      />
      {error ? (
        <AnalyticsPageEvent
          event="workspace_list_failed"
          properties={{
            source: "workspaces_page",
            surface: "workspace_list",
            error_message: error,
          }}
        />
      ) : null}

      <PageHeader>
        <PageTitle>Workspaces</PageTitle>
        <TrackedAnchor
          href="https://docs.usezombie.com/quickstart#add-workspace"
          target="_blank"
          rel="noopener noreferrer"
          className={buttonClassName("default", "sm")}
          event="workspace_add_docs_clicked"
          properties={{
            source: "workspaces_page",
            surface: "workspace_list",
            target: "quickstart_add_workspace",
          }}
        >
          <PlusIcon size={14} />
          Add workspace
        </TrackedAnchor>
      </PageHeader>

      {error ? (
        <div
          role="alert"
          className="mb-6 rounded-md border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive"
        >
          {error}
        </div>
      ) : null}

      {workspaces && workspaces.length === 0 ? (
        <EmptyState
          title="No workspaces yet"
          description="Add your first workspace to start queuing specs."
          action={
            <pre className="mt-2 inline-block rounded-md border border-border bg-card px-4 py-2 font-mono text-xs text-info">
              zombiectl workspace add https://github.com/your-org/your-repo
            </pre>
          }
        />
      ) : null}

      {workspaces && workspaces.length > 0 ? (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4">
          {workspaces.map((ws) => (
            <WorkspaceCard key={ws.id} workspace={ws} />
          ))}
        </div>
      ) : null}
    </div>
  );
}
