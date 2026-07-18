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
 * `buildSignInUrl("https://app.dev.agentsfleet.net/w/ws_1/fleets", "/w/ws_1/fleets")`
 * → `https://app.dev.agentsfleet.net/sign-in?redirect_url=%2Fw%2Fws_1%2Ffleets`.
 * `destination` is a relative path (`pathname + search`) so it stays same-origin;
 * `<SignIn>` honors `redirect_url` over its `fallbackRedirectUrl`.
 */
export function buildSignInUrl(requestUrl: string, destination: string): string {
  const signInUrl = new URL(SIGN_IN_PATH, requestUrl);
  signInUrl.searchParams.set(REDIRECT_URL_PARAM, destination);
  return signInUrl.toString();
}
