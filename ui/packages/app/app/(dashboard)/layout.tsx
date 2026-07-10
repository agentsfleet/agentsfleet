import { TooltipProvider } from "@agentsfleet/design-system";
import Shell from "@/components/layout/Shell";
import { auth } from "@clerk/nextjs/server";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { readSessionScopes } from "@/lib/auth/platform";

export default async function DashboardLayout({ children }: { children: React.ReactNode }) {
  const { getToken } = await auth();
  const token = await getToken();
  const [listResult, scopes] = token
    ? await Promise.all([
        // The switcher dropdown needs the full list; this is the one place
        // that legitimately fetches it (off the page data path). `cache()`
        // dedups it with the `[workspaceId]` guard + entry redirect.
        listTenantWorkspacesCached(token).catch(() => ({ items: [], total: 0 })),
        // Operator scopes gate the platform nav per-surface (Shell). Empty set
        // for an anonymous/no-token session.
        readSessionScopes(),
      ])
    : [{ items: [], total: 0 }, new Set<string>()];

  // Single TooltipProvider at the dashboard root keeps every <Tooltip>
  // (DataTable headers, EventsList timestamps, Time primitives, future
  // sites) on a coordinated delay timer. Per-page providers like
  // BillingBalanceCard stay nested — Radix tolerates re-entry.
  //
  // Shell derives the active workspace from the route (`/w/<id>/…`) itself —
  // no `activeWorkspaceId` prop, no cookie. It wraps both the workspace-scoped
  // subtree and the tenant/platform pages (settings/api-keys, billing, admin).
  return (
    <TooltipProvider>
      <Shell workspaces={listResult.items} operatorScopes={[...scopes]}>
        {children}
      </Shell>
    </TooltipProvider>
  );
}
