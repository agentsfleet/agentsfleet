import { cache } from "react";
import { cookies } from "next/headers";
import { auth } from "@clerk/nextjs/server";
import { ApiError } from "./api/errors";
import { listTenantWorkspaces } from "./api/workspaces";

/**
 * Per-request deduped wrapper around `listTenantWorkspaces`. Wrapping with
 * React's `cache()` makes every caller in a single RSC render share one
 * round-trip. Without this, a dashboard load can fire 4+ redundant
 * GET /v1/tenants/me/workspaces calls (layout + each Suspense boundary).
 */
export const listTenantWorkspacesCached = cache(listTenantWorkspaces);

export const ACTIVE_WORKSPACE_COOKIE = "active_workspace_id";

// JWT session-metadata key carrying the operator's primary workspace.
// Shared between the resolver and any future claim reader.
const WORKSPACE_CLAIM_KEY = "workspace_id";

// HTTP statuses that mean "this workspace id is not usable by this token":
// 403 (backend `authorizeWorkspace` denial) or 404 (workspace gone). Either
// triggers the one-shot re-resolve in `withWorkspaceScope`.
const FORBIDDEN_STATUS = 403;
const NOT_FOUND_STATUS = 404;

/**
 * Where the active workspace id came from. `cookie`/`claim` are hints we
 * trust without a round-trip (the backend re-authorizes on the data call);
 * `list` is authoritative (it came straight from the tenant's workspace list).
 */
export type ActiveWorkspace = { id: string; source: "cookie" | "claim" | "list" };

/**
 * Resolves the active workspace id for the current operator WITHOUT listing
 * workspaces when a hint is available.
 *
 * Lookup order:
 *   1. `active_workspace_id` cookie  → `source: "cookie"`  (no round-trip)
 *   2. `workspace_id` session claim  → `source: "claim"`   (no round-trip)
 *   3. first workspace in the tenant list → `source: "list"` (one round-trip)
 *   4. `null` when the tenant owns no workspaces.
 *
 * Hints are NOT validated against the list here — that validation is the
 * round-trip this function exists to avoid. The backend authorizes every
 * workspace-scoped call (`src/.../workspace_guards.zig`), so a stale hint
 * yields `ERR_FORBIDDEN`, which `withWorkspaceScope` recovers from.
 */
export async function resolveActiveWorkspaceId(
  token: string,
): Promise<ActiveWorkspace | null> {
  const cookieStore = await cookies();
  const cookieId = cookieStore.get(ACTIVE_WORKSPACE_COOKIE)?.value;
  if (cookieId) return { id: cookieId, source: "cookie" };

  const claimId = await readWorkspaceClaim();
  if (claimId) return { id: claimId, source: "claim" };

  return resolveFromList(token);
}

// Authoritative fallback: fetch the tenant's workspace list (cached) and take
// the first. Used both when no hint exists and as the re-resolve target in
// `withWorkspaceScope`.
async function resolveFromList(token: string): Promise<ActiveWorkspace | null> {
  const { items } = await listTenantWorkspacesCached(token).catch(() => ({ items: [] }));
  const first = items[0];
  return first ? { id: first.id, source: "list" } : null;
}

/**
 * Runs `fn` against the resolved active workspace id, recovering from a stale
 * hint. If the id came from a cookie/claim hint and `fn` rejects with a
 * 403/404, this re-resolves against the authoritative list and retries `fn`
 * exactly once with the list-derived id (when it differs). A list-derived id
 * never retries — the list is the source of truth, so its rejection is real.
 *
 * Returns `null` when the tenant owns no workspace at all (callers render the
 * "no workspace yet" empty state). Any non-authorization `ApiError` propagates
 * to the caller's existing `.catch`.
 *
 * The stale cookie is intentionally NOT cleared here: Next 16 Server
 * Components cannot mutate cookies. It self-heals on the next workspace switch
 * (a Server Action). The cost of a persistent stale cookie is one extra
 * round-trip per render for that operator until they switch — a rare edge.
 */
export async function withWorkspaceScope<T>(
  token: string,
  fn: (workspaceId: string) => Promise<T>,
): Promise<T | null> {
  const active = await resolveActiveWorkspaceId(token);
  if (!active) return null;

  try {
    return await fn(active.id);
  } catch (err) {
    if (active.source === "list" || !isWorkspaceRejection(err)) throw err;

    // The hint id was rejected. Re-resolve against the authoritative list.
    const authoritative = await resolveFromList(token);
    // Empty list = the tenant genuinely owns no workspace (the hint was a
    // ghost of a deleted one). That is the no-workspace state, NOT an error —
    // return null so the route renders its empty state instead of throwing.
    if (!authoritative) return null;
    // List confirms the same id we already tried — the rejection is real
    // (e.g. a role/permission issue, not a stale hint). Surface it.
    if (authoritative.id === active.id) throw err;
    return fn(authoritative.id);
  }
}

export function isWorkspaceRejection(err: unknown): boolean {
  return (
    err instanceof ApiError &&
    (err.status === FORBIDDEN_STATUS || err.status === NOT_FOUND_STATUS)
  );
}

/**
 * `.catch` handler factory for routes that degrade a failed list/read to an
 * empty shape: returns `fallback` for ordinary failures but re-throws a
 * workspace rejection so `withWorkspaceScope` can re-resolve and retry. Without
 * this, an inner `.catch(() => empty)` would swallow the 403 and the stale-hint
 * recovery would never fire.
 */
export function orFallback<T>(fallback: T): (err: unknown) => T {
  return (err: unknown) => {
    if (isWorkspaceRejection(err)) throw err;
    return fallback;
  };
}

// Reads the `workspace_id` claim from the session metadata. Returns null when
// the auth provider isn't available, the session is anonymous, or the claim is
// missing/malformed — every caller already handles null.
async function readWorkspaceClaim(): Promise<string | null> {
  try {
    const { sessionClaims } = await auth();
    const metadata = (sessionClaims?.metadata ?? null) as Record<string, unknown> | null;
    const value = metadata && typeof metadata[WORKSPACE_CLAIM_KEY] === "string"
      ? (metadata[WORKSPACE_CLAIM_KEY] as string)
      : null;
    return value;
  } catch {
    return null;
  }
}
