import { TooltipProvider } from "@agentsfleet/design-system";
import Shell from "@/components/layout/Shell";
import { auth } from "@clerk/nextjs/server";
import {
  listTenantWorkspacesCached,
  resolveActiveWorkspaceId,
} from "@/lib/workspace";
import { readSessionScopes } from "@/lib/auth/platform";

export default async function DashboardLayout({ children }: { children: React.ReactNode }) {
  const { getToken } = await auth();
  const token = await getToken();
  const [listResult, active, scopes] = token
    ? await Promise.all([
        // The switcher dropdown needs the full list; this is the one place
        // that legitimately fetches it (off the page data path). `cache()`
        // dedups it with any in-render caller.
        listTenantWorkspacesCached(token).catch(() => ({ items: [], total: 0 })),
        // Cheap hint resolve (cookie/claim) — no extra round-trip on the hot path.
        resolveActiveWorkspaceId(token),
        // Operator scopes gate the platform nav per-surface (Shell). Empty set
        // for an anonymous/no-token session.
        readSessionScopes(),
      ])
    : [{ items: [], total: 0 }, null, new Set<string>()];

  // Single TooltipProvider at the dashboard root keeps every <Tooltip>
  // (DataTable headers, EventsList timestamps, Time primitives, future
  // sites) on a coordinated delay timer. Per-page providers like
  // BillingBalanceCard stay nested — Radix tolerates re-entry.
  return (
    <TooltipProvider delayDuration={150}>
      <Shell workspaces={listResult.items} activeWorkspaceId={active?.id ?? null} operatorScopes={[...scopes]}>
        {children}
      </Shell>
    </TooltipProvider>
  );
}
