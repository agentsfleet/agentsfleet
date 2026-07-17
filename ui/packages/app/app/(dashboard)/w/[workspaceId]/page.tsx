import { redirect } from "next/navigation";
import { DEFAULT_WORKSPACE_SUBPATH, workspacePath } from "@/lib/workspace-routes";

// The workspace root is a permanent redirect to the Wall — the single-route
// refactor removed the dashboard. The Wall (`/w/{ws}/fleets`) is the only entry
// point; with zero fleets it renders the Getting Started checklist as its empty
// state, so there is no landing decision to make here. This route survives only
// so bookmarks and stale links to `/w/{ws}/` keep working; it renders nothing.
export default async function WorkspaceRootPage({
  params,
}: {
  params: Promise<{ workspaceId: string }>;
}) {
  const { workspaceId } = await params;
  redirect(workspacePath(workspaceId, DEFAULT_WORKSPACE_SUBPATH));
}
