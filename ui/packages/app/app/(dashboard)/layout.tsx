import { ShellFrame } from "@/components/layout/ShellFrame";
import { auth } from "@clerk/nextjs/server";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { readSessionScopes } from "@/lib/auth/platform";

export default async function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const { getToken } = await auth();
  const token = await getToken();
  const [listResult, scopes] = token
    ? await Promise.all([
        // The switcher needs the complete workspace list; this
        // is the one place that walks the complete cursor-paginated list off
        // the page data path. `cache()` deduplicates that walk with the
        // `[workspaceId]` guard and entry redirect.
        listTenantWorkspacesCached(token).catch(() => ({
          items: [],
          total: 0,
        })),
        // Operator scopes gate the platform nav per-surface (Shell). Empty set
        // for an anonymous/no-token session.
        readSessionScopes(),
      ])
    : [{ items: [], total: 0 }, new Set<string>()];

  // Shell derives the active workspace from the route (`/w/<id>/…`) itself —
  // no `activeWorkspaceId` prop, no cookie. It wraps both the workspace-scoped
  // subtree and the tenant/platform pages (settings/api-keys, billing, admin).
  return (
    <ShellFrame workspaces={listResult.items} operatorScopes={[...scopes]}>
      {children}
    </ShellFrame>
  );
}
