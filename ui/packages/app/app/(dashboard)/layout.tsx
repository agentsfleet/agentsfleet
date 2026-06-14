import { TooltipProvider } from "@agentsfleet/design-system";
import Shell from "@/components/layout/Shell";
import { auth } from "@clerk/nextjs/server";
import {
  listTenantWorkspacesCached,
  resolveActiveWorkspace,
} from "@/lib/workspace";
import { readPlatformAdminClaim } from "@/lib/auth/platform";

export default async function DashboardLayout({ children }: { children: React.ReactNode }) {
  const { getToken } = await auth();
  const token = await getToken();
  const [listResult, active, isPlatformAdmin] = token
    ? await Promise.all([
        // Cached so `resolveActiveWorkspace` below (and every Suspense
        // boundary on /) share a single GET /v1/tenants/me/workspaces.
        listTenantWorkspacesCached(token).catch(() => ({ items: [], total: 0 })),
        resolveActiveWorkspace(token),
        readPlatformAdminClaim(),
      ])
    : [{ items: [], total: 0 }, null, false];

  // Single TooltipProvider at the dashboard root keeps every <Tooltip>
  // (DataTable headers, EventsList timestamps, Time primitives, future
  // sites) on a coordinated delay timer. Per-page providers like
  // BillingBalanceCard stay nested — Radix tolerates re-entry.
  return (
    <TooltipProvider delayDuration={150}>
      <Shell workspaces={listResult.items} activeWorkspaceId={active?.id ?? null} isPlatformAdmin={isPlatformAdmin}>
        {children}
      </Shell>
    </TooltipProvider>
  );
}
