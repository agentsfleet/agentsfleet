// Builds the URL Clerk's middleware sends unauthenticated requests to: our
// embedded `/sign-in` page, carrying the intended destination on `redirect_url`
// so a completed sign-in returns there instead of dropping the deep-link. Kept
// pure (strings in, string out) so the redirect_url contract is unit-testable
// without mocking Clerk's `clerkMiddleware`/`auth.protect`.

/** Embedded sign-in route. Clerk's hosted Account Portal is bypassed — see `proxy.ts`. */
export const SIGN_IN_PATH = "/sign-in";

/** Query param `<SignIn>` reads to return the user to their intended page post-auth. */
const REDIRECT_URL_PARAM = "redirect_url";

/**
 * Collapses a destination to `/` unless it is an unambiguously same-origin
 * absolute path. A protocol-relative (`//evil.example`) or backslash-tricked
 * (`/\evil.example`, which browsers normalize to `//`) value carries a
 * cross-origin authority a browser would resolve as an absolute URL — storing
 * it on `redirect_url` would turn a completed sign-in into an open redirect.
 * A same-origin deep link always begins with exactly one `/`.
 */
function sameOriginDestination(destination: string): string {
  if (!destination.startsWith("/")) return "/";
  if (destination.startsWith("//") || destination.startsWith("/\\")) return "/";
  return destination;
}

/**
 * `buildSignInUrl("https://app.dev.agentsfleet.net/w/ws_1/fleets", "/w/ws_1/fleets")`
 * → `https://app.dev.agentsfleet.net/sign-in?redirect_url=%2Fw%2Fws_1%2Ffleets`.
 * `destination` is a relative path (`pathname + search`) so it stays same-origin;
 * `<SignIn>` honors `redirect_url` over its `fallbackRedirectUrl`. A destination
 * that is not an unambiguous same-origin path collapses to root — a completed
 * sign-in must never be steerable off-origin.
 */
export function buildSignInUrl(requestUrl: string, destination: string): string {
  const signInUrl = new URL(SIGN_IN_PATH, requestUrl);
  signInUrl.searchParams.set(REDIRECT_URL_PARAM, sameOriginDestination(destination));
  return signInUrl.toString();
}
