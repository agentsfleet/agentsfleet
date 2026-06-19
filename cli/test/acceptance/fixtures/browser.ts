/**
 * Playwright Chromium wrapper for the CLI-auth handshake.
 *
 * Establishes a real Clerk session via `@clerk/testing`'s `clerk.signIn`
 * (the same mechanism the dashboard acceptance suite's `signInAs` uses),
 * then drives the `/cli-auth/{session_id}` approve action and returns the
 * 6-digit verification code the page displays.
 *
 * Why `clerk.signIn` and not a manual cookie-mount: a Backend-API-minted
 * `__session` token lacks the `azp` claim `clerkMiddleware` now requires, so
 * a hand-mounted cookie is bounced to `/sign-in` on the first protected
 * navigation (it also zeroes `__client_uat`). clerk-js mints the cookies the
 * middleware was built to consume, so the approve page actually
 * authenticates. Requires `CLERK_PUBLISHABLE_KEY` + `CLERK_SECRET_KEY` in
 * env (resolved by global-setup); `setupClerkTestingToken` bypasses the dev
 * bot-protection on the sign-in form.
 *
 * Selector: approve button by accessible role (`button` named /approve/i).
 * Code: scraped from the `<output aria-label="Verification code">` the page
 * renders on success — the CLI's /verify call is the authoritative ack.
 */

const APPROVE_BUTTON_NAME = /approve/i;
const VERIFICATION_CODE_LABEL = "Verification code";
const VERIFICATION_CODE_RE = /^\d{6}$/;
const SIGN_IN_PATH = "/sign-in";
const DEFAULT_TIMEOUT_MS = 30_000;

export interface CliAuthHandoffOptions {
  readonly loginUrl: string;
  readonly email: string;
  readonly timeoutMs?: number;
}

/**
 * Sign in the fixture user via clerk-js, drive the CLI-auth approve action,
 * and return the 6-digit verification code the page displays on success.
 */
export async function completeCliAuthHandoff(opts: CliAuthHandoffOptions): Promise<string> {
  if (!opts?.loginUrl) throw new Error("completeCliAuthHandoff: loginUrl required");
  if (!opts?.email) throw new Error("completeCliAuthHandoff: email required");

  // Lazy imports — playwright + @clerk/testing are devDependencies; never
  // pulled into non-handshake paths (the specs import this module but only
  // call it when the handshake is enabled).
  const { chromium } = await import("playwright");
  const { clerk, clerkSetup, setupClerkTestingToken } = await import("@clerk/testing/playwright");

  // clerkSetup fetches the Clerk Frontend API URL (from CLERK_PUBLISHABLE_KEY)
  // that setupClerkTestingToken needs to bypass dev bot-protection. The
  // dashboard suite calls this in global setup; we call it here (idempotent)
  // so the handshake fixture is self-contained.
  await clerkSetup();

  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const origin = new URL(opts.loginUrl).origin;

  // Vercel deployment protection guards the dev/preview dashboard — without
  // the bypass header the browser hits Vercel's password page instead of the
  // app, so clerk-js never loads and clerk.signIn hangs on window.Clerk.loaded.
  // Mirrors ui/.../playwright.acceptance.config.ts. Omitted on public deploys.
  const bypass = process.env.VERCEL_BYPASS_SECRET;
  const contextOptions = bypass
    ? { extraHTTPHeaders: { "x-vercel-protection-bypass": bypass, "x-vercel-set-bypass-cookie": "true" } }
    : {};

  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext(contextOptions);
    const page = await context.newPage();
    page.setDefaultTimeout(timeoutMs);

    // clerk-js needs a Clerk-aware page mounted before it can mint a session;
    // /sign-in is the cheapest such page in the dashboard.
    await setupClerkTestingToken({ page });
    await page.goto(`${origin}${SIGN_IN_PATH}`, { waitUntil: "load", timeout: timeoutMs });
    await clerk.signIn({ page, emailAddress: opts.email });

    await page.goto(opts.loginUrl, { waitUntil: "load", timeout: timeoutMs });
    const approve = page.getByRole("button", { name: APPROVE_BUTTON_NAME });
    await approve.waitFor({ state: "visible", timeout: timeoutMs });
    await approve.click();

    const codeOutput = page.getByLabel(VERIFICATION_CODE_LABEL);
    await codeOutput.waitFor({ state: "visible", timeout: timeoutMs });
    const code = ((await codeOutput.textContent()) ?? "").trim();
    if (!VERIFICATION_CODE_RE.test(code)) {
      throw new Error(`completeCliAuthHandoff: expected a 6-digit code, got ${JSON.stringify(code)}`);
    }
    return code;
  } finally {
    await browser.close().catch(() => {});
  }
}
