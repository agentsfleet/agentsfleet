import { TooltipProvider } from "@agentsfleet/design-system";
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
        // Operator scopes gate the platform navigation. Empty set
        // for an anonymous/no-token session.
        readSessionScopes(),
      ])
    : [{ items: [], total: 0 }, new Set<string>()];

  // Shell controls derive the active workspace from `/w/<id>/…`; no
  // `activeWorkspaceId` prop or cookie owns navigation state. ShellFrame wraps
  // both workspace-scoped and tenant/platform pages.
  // The dashboard's ONE tooltip provider. `Tooltip` is Radix's Root, which
  // reads provider context unconditionally and THROWS when there is none — so
  // an island rendering a relative `Time` (tooltip on by default) took its
  // whole page down. Mounting it here makes that unrepresentable instead of a
  // rule each new island has to remember.
  //
  // Here and not the root layout: above every route group it also loaded the
  // tooltip runtime into the auth bundles, which render no tooltips at all.
  // Every tooltip in the app is under this segment. A client component in a
  // Server Component layout — `children` still stream through as slots, and
  // `(dashboard)/error.tsx` renders inside this layout, so the error surface
  // is covered too.
  return (
    <TooltipProvider>
      <ShellFrame workspaces={listResult.items} operatorScopes={[...scopes]}>
        {children}
      </ShellFrame>
    </TooltipProvider>
  );
}
