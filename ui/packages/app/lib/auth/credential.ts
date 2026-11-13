import type { ActionResult } from "@/lib/actions/with-token";
import type { AccountSession, AccountSessionPage } from "./account-types";
import type { IdentitySession } from "./account-provider";
import { z } from "zod";
import { cache } from "react";
import { auth, reverificationError } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { readProviderSessions, readProviderSession, revokeProviderSession } from "./account-provider";

const ownerSchema = z.string().min(1).max(256);
const pageSchema = z.number().int().min(0).max(10_000);
const sessionIdSchema = z.string().min(1).max(256).regex(/^[A-Za-z0-9_-]+$/);
const ACCOUNT_REQUEST_TIMEOUT_MS = 10_000;
const ENDED = new Set(["revoked", "ended", "removed", "expired", "abandoned"]);
const CHANGED = "Your signed-in account changed. Reload this page before continuing.";


/**
 * The one place the dashboard resolves a credential.
 *
 * Not the only importer of the provider's SDK — `proxy.ts` takes
 * `clerkMiddleware` and is irreducible, because the middleware IS the session
 * verification. It is the only place `auth()` is called, which is the claim
 * that matters: every consumer of a bearer goes through here.
 *
 * Twenty-seven files used to call the identity provider's `auth()` directly,
 * and twenty-six of them wanted the same thing: a bearer to put on a call to
 * agentsfleetd. None read a user id. None read an organisation. The provider
 * was never supplying identity to this app — the daemon decides that, from the
 * token, behind `requireScope` and `authorizeWorkspace`. It was supplying a
 * string, twenty-seven times, from a vendor SDK.
 *
 * So this module is a boundary rather than a facade. A facade renames the
 * dependency and leaves every page holding a session object; swapping providers
 * would still touch every page, because the SHAPE leaked. Here the shape is
 * "give me a bearer", which is our concept, not the provider's. Changing
 * provider is a rewrite of this file and of the component re-exports in
 * `client.ts`. Nothing under `app/` moves.
 *
 * The eventual shape is smaller still: `docs/AUTH.md` §Why the dashboard rides
 * one token records the Backend-for-Frontend as deferred, and under it these
 * functions forward a cookie instead of minting anything. That is a change to
 * this file when it comes.
 */

/** Where a session that expired mid-render sends the visitor. */
const SIGN_IN_PATH = "/sign-in";

/** The tenant claim's name, and the object the provider nests it under. */
const CLAIM_TENANT_ID = "tenant_id";
const CLAIM_METADATA = "metadata";

/**
 * The bearer for an outbound daemon call, or `null` when nobody is signed in.
 *
 * For callers that have their own answer to "not signed in" — a route handler
 * composing a 401 body, a layout that renders a signed-out shell. A caller that
 * simply cannot proceed wants [`requireCredential`] instead.
 *
 * # `cache()` is the reason this is one function and not twenty-seven
 *
 * Rendering the approvals page resolves a bearer FOUR times: the dashboard
 * layout, the workspace layout, and the page twice. Twenty-seven scattered
 * `auth()` calls cannot be collapsed — there is nothing to collapse them at.
 * One function can be, and `cache()` does it, the same primitive
 * `listWorkspaceFleetLibraryCached` already uses a few files over.
 *
 * React's `cache()` is scoped to ONE server request and torn down with it. That
 * distinction is load-bearing: a module-level `let token` would survive between
 * requests and hand one visitor another visitor's bearer. Never cache a
 * credential in module scope.
 */
export const credential = cache(async (): Promise<string | null> => {
  const { getToken } = await auth();
  return getToken();
});

/** App-owned identity for personal settings; never use a caller-supplied user as authority. */
export const accountSessionIdentity = cache(async () => {
  const identity = await auth();
  if (!identity.userId || !identity.sessionId) return null;
  return {subject: identity.userId, sessionId: identity.sessionId,
    recentlyVerified: identity.has({reverification: "strict"}), impersonating: Boolean(identity.actor)};
});

/**
 * The bearer, or a redirect to sign-in.
 *
 * The redirect throws, so the return type carries no null and no call site
 * needs a guard — which is the whole reason this exists beside
 * [`credential`]: twenty-four pages had written that guard by hand.
 *
 * No destination is carried, deliberately. `proxy.ts` already routes an
 * unauthenticated visitor to sign-in WITH the page they asked for; a page body
 * only runs once the middleware let the request through, so reaching here means
 * a session that expired mid-render, not a deep link to preserve.
 */
export async function requireCredential(): Promise<string> {
  const token = await credential();
  if (!token) redirect(SIGN_IN_PATH);
  return token;
}

/**
 * The verified claim set, for the dashboard's own scope check.
 *
 * The one thing this app reads beyond a bearer, and it is defence in depth:
 * `requireScope` in `afd_http`'s route table is the authoritative gate, and a
 * missing scope is refused there whatever the dashboard rendered. Returns
 * `null` when the provider is unavailable or the visitor is anonymous, so
 * every caller fails closed.
 */
export const claims = cache(async (): Promise<Record<string, unknown> | null> => {
  try {
    const { sessionClaims } = await auth();
    return (sessionClaims as Record<string, unknown> | null) ?? null;
  } catch {
    return null;
  }
});

export async function readAccountSessions(expectedSubject: string, offset: number): Promise<ActionResult<AccountSessionPage>> {
  const identity = await accountSessionIdentity();
  if (!identity) return failure(401, "Sign in to view your sessions.");
  if (identity.impersonating) return failure(403, "Personal sessions are unavailable while impersonating another account.");
  if (!ownerSchema.safeParse(expectedSubject).success || identity.subject !== expectedSubject) return failure(409, CHANGED);
  if (!pageSchema.safeParse(offset).success) return failure(400, "Invalid session page.");
  try {
    const page = await readProviderSessions(identity.subject, offset, AbortSignal.timeout(ACCOUNT_REQUEST_TIMEOUT_MS));
    if (page.data.some(session => session.user_id !== identity.subject)) return failure(503, "Your sessions could not be loaded.");
    const more = offset + page.data.length < page.total_count;
    if (more && page.data.length === 0) return failure(503, "Your session page could not be confirmed.");
    return {ok: true, data: {sessions: page.data.filter(session => session.status === "active").map(session => sessionView(session, identity.sessionId)),
      nextOffset: more ? offset + page.data.length : null}};
  } catch {return failure(503, "Your sessions could not be loaded. Try again.");}
}

/** Strict recent verification and target ownership precede the privileged provider write. */
export async function revokeAccountSession(expectedSubject: string, id: string) {
  const identity = await accountSessionIdentity();
  if (!identity) return failure(401, "Sign in to manage your sessions.");
  if (identity.impersonating) return failure(403, "Personal sessions are unavailable while impersonating another account.");
  if (!ownerSchema.safeParse(expectedSubject).success || identity.subject !== expectedSubject) return failure(409, CHANGED);
  if (!sessionIdSchema.safeParse(id).success) return failure(400, "Invalid session.");
  if (id === identity.sessionId) return failure(409, "Use Sign out to end the session on this device.");
  if (!identity.recentlyVerified) return reverificationError("strict");
  try {
    const signal = AbortSignal.timeout(ACCOUNT_REQUEST_TIMEOUT_MS);
    const session = await readProviderSession(id, signal);
    if (session.user_id !== identity.subject || session.id !== id) return failure(404, "This session is not available.");
    if (ENDED.has(session.status)) return {ok: true as const, data: {id}};
    if (session.status !== "active") return failure(409, "Refresh the session list before trying again.");
    const result = await revokeProviderSession(id, signal);
    if (result.id === id && result.user_id === identity.subject && ENDED.has(result.status)) return {ok: true as const, data: {id}};
  } catch { /* A lost response cannot establish whether revocation completed. */ }
  return failure(503, "Session sign-out could not be confirmed. Refresh the list before trying again.");
}

function sessionView(session: IdentitySession, currentId: string): AccountSession {
  const activity = session.latest_activity;
  const instant = typeof session.last_active_at === "number" ? new Date(session.last_active_at) : null;
  return {id: session.id, browser: activity?.browser_name || "Unknown browser", device: activity?.device_type || "Unknown device",
    location: [activity?.city, activity?.country].filter(Boolean).join(", ") || null, mobile: activity?.is_mobile ?? false,
    current: session.id === currentId, lastActiveAt: instant && Number.isFinite(instant.getTime()) ? instant.toISOString() : null};
}

function failure(status: number, error: string) {return {ok: false as const, status, error};}

/**
 * Whether a readable session carries no tenant: the signup webhook has not
 * yet written the account's tenant to the provider, or this token was minted
 * before it did.
 *
 * Read where the daemon reads it (`afd_identity/src/jwks/claims.rs`): the top
 * level first, then under `metadata`, where the session-token template
 * projects it. `false` when the claims cannot be read at all, so a provider
 * outage is never taken for an account still opening.
 */
export async function tenantClaimMissing(): Promise<boolean> {
  const session = await claims();
  if (session === null) return false;
  const metadata = session[CLAIM_METADATA];
  const nested =
    typeof metadata === "object" && metadata !== null
      ? (metadata as Record<string, unknown>)[CLAIM_TENANT_ID]
      : undefined;
  const tenant = session[CLAIM_TENANT_ID] ?? nested;
  return typeof tenant !== "string" || tenant === "";
}
