"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  createTenantWorkspace,
  type CreateWorkspaceResponse,
} from "@/lib/api/workspaces";

// Creates a workspace and returns the new workspace id + name. Selection is now
// a client navigation (`router.push('/w/<newId>')`, see CreateWorkspaceDialog)
// — this action no longer writes an active-workspace cookie or revalidates; the
// URL is authoritative.
export async function createWorkspaceAction(
  body: { name?: string },
): Promise<ActionResult<CreateWorkspaceResponse>> {
  return withToken((t) => createTenantWorkspace(t, body));
}
