import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { workspacePath } from "@/lib/workspace-routes";
import NoWorkspaceEmptyState from "@/components/layout/NoWorkspaceEmptyState";

export const dynamic = "force-dynamic";

// Dashboard entry (`/`). Resolves the first owned workspace and redirects once
// to its explicit URL (`/w/<id>/`); a tenant that owns no workspace lands on the
// create-workspace empty state instead of a broken page. This is the ONLY
// place the "default workspace" is chosen — every deeper page reads the
// workspace from its route param.
export default async function DashboardIndexPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const { items } = await listTenantWorkspacesCached(token).catch(() => ({ items: [] }));
  const first = items[0];
  if (first) redirect(workspacePath(first.id));

  return <NoWorkspaceEmptyState />;
}
