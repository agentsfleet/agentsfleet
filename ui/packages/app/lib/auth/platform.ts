import { auth } from "@clerk/nextjs/server";

/**
 * Reads the operator `scopes` claim off the Clerk session token.
 *
 * `scopes` is a **top-level** claim (not nested under `metadata`) that Clerk's
 * session-token customization projects from `public_metadata.scopes`
 * (docs/AUTH.md §Session token). It is the same claim the backend verifies via
 * `requireScope` — which stays the authoritative gate (a missing scope is
 * rejected `403 UZ-AUTH-022` regardless of what the UI shows). Every read here
 * is the dashboard's defence-in-depth check: hide/redirect/refuse a surface the
 * caller's token can't reach.
 *
 * The documented session-token form is a space-delimited string
 * (`"runner:read runner:enroll model:admin"`); we also accept a JSON array to
 * mirror the backend's tolerant reader (`claims.zig` `getScopesOwned`) in case
 * the template is ever switched to array form.
 *
 * Returns an empty set when the auth provider isn't available, the session is
 * anonymous, or the claim is absent (fail-closed) — every caller treats a
 * missing scope as "not permitted".
 */
export async function readSessionScopes(): Promise<ReadonlySet<string>> {
  try {
    const { sessionClaims } = await auth();
    const raw = (sessionClaims as Record<string, unknown> | null)?.scopes;
    return parseScopes(raw);
  } catch {
    return new Set();
  }
}

/** True iff the live session token carries `scope`. Fail-closed on any error. */
export async function hasScope(scope: string): Promise<boolean> {
  return (await readSessionScopes()).has(scope);
}

function parseScopes(raw: unknown): ReadonlySet<string> {
  if (typeof raw === "string") return new Set(raw.split(/\s+/).filter(Boolean));
  if (Array.isArray(raw)) return new Set(raw.filter((s): s is string => typeof s === "string"));
  return new Set();
}
