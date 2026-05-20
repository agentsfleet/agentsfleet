"use server";

import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";
import { ACTIVE_WORKSPACE_COOKIE } from "@/lib/workspace";
import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  createTenantWorkspace,
  type CreateWorkspaceResponse,
} from "@/lib/api/workspaces";

const ACTIVE_WORKSPACE_MAX_AGE_S = 60 * 60 * 24 * 365;

// Writes the active-workspace cookie server-side, then revalidates the
// dashboard so the next render picks up the new selection. The cookie is
// a UX preference (not auth) — no HttpOnly, no Secure flag required
// beyond what the transport enforces.
async function writeActiveWorkspace(workspaceId: string): Promise<void> {
  const store = await cookies();
  store.set({
    name: ACTIVE_WORKSPACE_COOKIE,
    value: workspaceId,
    path: "/",
    sameSite: "lax",
    maxAge: ACTIVE_WORKSPACE_MAX_AGE_S,
  });
  revalidatePath("/", "layout");
}

export async function setActiveWorkspace(workspaceId: string): Promise<void> {
  await writeActiveWorkspace(workspaceId);
}

// Creates a workspace, then switches the active-workspace cookie to it on
// success so the dashboard lands on the new workspace. On failure the cookie
// is left untouched (no half-switch).
export async function createWorkspaceAction(
  body: { name?: string },
): Promise<ActionResult<CreateWorkspaceResponse>> {
  const result = await withToken((t) => createTenantWorkspace(t, body));
  if (result.ok) await writeActiveWorkspace(result.data.workspace_id);
  return result;
}
