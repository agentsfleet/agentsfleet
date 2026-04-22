import { cache } from "react";
import { cookies } from "next/headers";
import { getServerSessionMetadata } from "./auth/server";
import { listTenantWorkspaces, type TenantWorkspace } from "./api/workspaces";

/**
 * Per-request deduped wrapper around `listTenantWorkspaces`. Wrapping with
 * React's `cache()` makes every caller in a single RSC render share one
 * round-trip. Without this, a dashboard load can fire 4+ redundant
 * GET /v1/tenants/me/workspaces calls (layout prefetch + each Suspense
 * boundary that calls resolveActiveWorkspace). Both the layout and this
 * module use the cached wrapper.
 */
export const listTenantWorkspacesCached = cache(listTenantWorkspaces);

export const ACTIVE_WORKSPACE_COOKIE = "active_workspace_id";

/**
 * Resolves which workspace the operator is currently acting on.
 *
 * Lookup order:
 *   1. The `active_workspace_id` cookie, if it names a workspace the tenant
 *      owns. Written by the in-shell workspace switcher.
 *   2. The `workspace_id` claim on the Clerk session JWT, if present and
 *      still a valid member of the tenant's workspace list. Alpha Clerk
 *      metadata set this to the "primary" workspace per user.
 *   3. The first workspace in the tenant's list (creation-order ascending)
 *      — deterministic fallback when neither hint resolves.
 *   4. `null` when the tenant has no workspaces at all.
 *
 * Backend contract: `GET /v1/tenants/me/workspaces` reads `tenant_id` from
 * the JWT principal, returns every workspace the tenant owns. We never
 * pass the tenant id explicitly; that's server-side only.
 */
export async function resolveActiveWorkspace(
  token: string,
): Promise<TenantWorkspace | null> {
  const { items } = await listTenantWorkspacesCached(token).catch(() => ({ items: [] }));
  if (items.length === 0) return null;

  const cookieStore = await cookies();
  const preferredId = cookieStore.get(ACTIVE_WORKSPACE_COOKIE)?.value;
  if (preferredId) {
    const match = items.find((ws) => ws.id === preferredId);
    if (match) return match;
  }

  const claimId = await readWorkspaceClaim();
  if (claimId) {
    const match = items.find((ws) => ws.id === claimId);
    if (match) return match;
  }

  return items[0] ?? null;
}

// Reads the `workspace_id` claim from the session metadata.
// Returns null if the auth provider isn't available, the session is
// anonymous, or the claim is missing — every caller already handles null.
async function readWorkspaceClaim(): Promise<string | null> {
  try {
    const metadata = await getServerSessionMetadata();
    const value = metadata && typeof metadata.workspace_id === "string"
      ? metadata.workspace_id
      : null;
    return value;
  } catch {
    return null;
  }
}
