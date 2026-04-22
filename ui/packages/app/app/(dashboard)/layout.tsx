import Shell from "@/components/layout/Shell";
import { getServerToken } from "@/lib/auth/server";
import { listTenantWorkspaces } from "@/lib/api/workspaces";
import { resolveActiveWorkspace } from "@/lib/workspace";

export default async function DashboardLayout({ children }: { children: React.ReactNode }) {
  const token = await getServerToken();
  const [listResult, active] = token
    ? await Promise.all([
        listTenantWorkspaces(token).catch(() => ({ items: [], total: 0 })),
        resolveActiveWorkspace(token),
      ])
    : [{ items: [], total: 0 }, null];

  return (
    <Shell workspaces={listResult.items} activeWorkspaceId={active?.id ?? null}>
      {children}
    </Shell>
  );
}
