import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { firstTenantWorkspace } from "@/lib/api/workspaces";
import { DEFAULT_WORKSPACE_SUBPATH, workspacePath } from "@/lib/workspace-routes";
import NoWorkspaceEmptyState from "@/components/layout/NoWorkspaceEmptyState";

export const dynamic = "force-dynamic";

// Dashboard entry (`/`). Resolves the first owned workspace and redirects once
// to its fleet wall; a tenant that owns no workspace lands on the
// create-workspace empty state instead of a broken page. This is the ONLY
// place the "default workspace" is chosen — every deeper page reads the
// workspace from its route param.
//
// One page of one, deliberately: the redirect fires before the layout tree
// renders, so nothing here can share the layout's cached full walk — a full
// list on this request is pure double-payment on every cold entry.
export default async function DashboardIndexPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // No `.catch` here: a transient list failure must NOT fall through to the
  // create-first empty state — an operator who already owns workspaces would be
  // shown "create a workspace" and could make a duplicate. The error propagates
  // to `(dashboard)/error.tsx` (a retry surface) instead; the empty state renders
  // only on a genuinely empty list (a successful 200 with no items).
  const first = await firstTenantWorkspace(token);
  if (first) redirect(workspacePath(first.id, DEFAULT_WORKSPACE_SUBPATH));

  return <NoWorkspaceEmptyState />;
}
