import { cookies } from "next/headers";
import { listWorkspaces } from "./api";
import type { Workspace } from "./types";

const ACTIVE_WORKSPACE_COOKIE = "active_workspace_id";

/**
 * Resolves which workspace the operator is currently acting on.
 *
 * Lookup order:
 *   1. The `active_workspace_id` cookie, if it names a workspace the tenant
 *      owns. This is the handle the dashboard-pages workspace switcher
 *      writes when it ships — one-line swap here when that lands.
 *   2. The first workspace in the tenant's list. Deterministic (the API
 *      returns creation-order ascending) so the fallback is stable across
 *      requests, not arbitrary.
 *   3. `null` when the tenant has no workspaces at all — callers render an
 *      empty-workspace affordance.
 *
 * Kept as a standalone resolver rather than inlined so every zombie-route
 * page (and anything else that goes workspace-scoped later) picks the same
 * workspace the same way. When the switcher ships, only this file changes.
 */
export async function resolveActiveWorkspace(
  token: string,
): Promise<Workspace | null> {
  const workspaces = await listWorkspaces(token).then((r) => r.data);
  if (workspaces.length === 0) return null;

  const cookieStore = await cookies();
  const preferredId = cookieStore.get(ACTIVE_WORKSPACE_COOKIE)?.value;
  if (preferredId) {
    const match = workspaces.find((ws) => ws.id === preferredId);
    if (match) return match;
  }

  return workspaces[0] ?? null;
}
