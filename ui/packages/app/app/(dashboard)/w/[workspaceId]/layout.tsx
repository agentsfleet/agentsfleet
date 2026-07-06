import { auth } from "@clerk/nextjs/server";
import { notFound, redirect } from "next/navigation";
import { listTenantWorkspacesCached } from "@/lib/workspace";

// Ownership guard for the workspace-scoped subtree. The URL `workspaceId` is a
// UX selector, so a hand-edited/shared/stale id that the tenant doesn't own
// renders `notFound()` here rather than another workspace's data. This is
// defence-in-depth on top of the real security boundary: every backend call
// under this route re-authorizes with `ownsWithinTenant` server-side (the URL
// id is NEVER an authorization input). No data client runs before this check.
export default async function WorkspaceLayout({
  children,
  params,
}: {
  children: React.ReactNode;
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Fail OPEN on a transient list-read failure: this guard is a UX affordance,
  // not the security boundary — every backend call under this route re-authorizes
  // with `ownsWithinTenant` and 403s an un-owned id regardless. Blanking a
  // possibly-owned workspace to a hard 404 on a list-endpoint blip would be worse
  // than letting the backend gate. Only a *confirmed* miss (list succeeded, id
  // absent) renders `notFound()`.
  let owned = true;
  try {
    const { items } = await listTenantWorkspacesCached(token);
    owned = items.some((workspace) => workspace.id === workspaceId);
  } catch {
    owned = true;
  }
  if (!owned) notFound();

  return <>{children}</>;
}
