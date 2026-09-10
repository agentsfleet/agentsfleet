import { cache } from "react";
import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";

/**
 * The one place the dashboard learns what a credential is.
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

/** Where a session that expired mid-render sends the visitor. */
const SIGN_IN_PATH = "/sign-in";
