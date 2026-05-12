/**
 * Playwright Chromium wrapper for §5's CLI-auth handshake.
 *
 * Used only by `lifecycle-after-login.spec.js`. We deliberately avoid
 * `@playwright/test` — the spec orchestrates the CLI subprocess itself,
 * and a parallel test-runner framework on top would add no value. One
 * `chromium.launch()` per call, closed in `finally`.
 *
 * Cookie-mount mirrors the dashboard suite's `signInAs` shape (three
 * Clerk DEV cookies: `__session`, `__client_uat`, `__clerk_db_jwt`).
 * The `__client_uat` value MUST be `<= jwt.iat` per Clerk's middleware —
 * we decode the cookieJwt's iat and set uat to `iat - 1`.
 *
 * Selector contract — the dashboard's `/cli-auth/{session_id}` page MUST
 * expose `data-testid="cli-auth-approve"` on the approve action. The
 * carve-out subsection in `docs/AUTH.md` documents this contract; the
 * dashboard's CLI-auth handoff PR lands the page + selector.
 */

const APPROVE_SELECTOR = "[data-testid=\"cli-auth-approve\"]";
const DEFAULT_TIMEOUT_MS = 30_000;

function decodeJwtIat(jwt) {
  const payload = jwt.split(".")[1];
  if (!payload) throw new Error("malformed cookieJwt (no payload segment)");
  const json = JSON.parse(Buffer.from(payload, "base64url").toString("utf8"));
  if (typeof json.iat !== "number") throw new Error("cookieJwt missing iat claim");
  return json.iat;
}

function cookieAttrs(loginUrl) {
  const url = new URL(loginUrl);
  return {
    domain: url.hostname,
    path: "/",
    sameSite: "Lax",
    secure: url.protocol === "https:",
  };
}

/**
 * Drive a Playwright Chromium context through the CLI-auth approve action.
 *
 * @param {object} opts
 * @param {string} opts.loginUrl - the URL the CLI emitted on stdout
 * @param {string} opts.cookieJwt - default Clerk session JWT (no template)
 * @param {number} [opts.timeoutMs] - per-action timeout (default 30s)
 * @returns {Promise<void>}
 */
export async function completeCliAuthHandoff(opts) {
  if (!opts?.loginUrl) throw new Error("completeCliAuthHandoff: loginUrl required");
  if (!opts?.cookieJwt) throw new Error("completeCliAuthHandoff: cookieJwt required");

  // Lazy import — playwright is a devDependency; never pulled in non-§5 paths.
  const { chromium } = await import("playwright");

  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const iat = decodeJwtIat(opts.cookieJwt);
  const attrs = cookieAttrs(opts.loginUrl);

  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext();
    await context.addCookies([
      { name: "__session", value: opts.cookieJwt, httpOnly: true, ...attrs },
      { name: "__client_uat", value: String(iat - 1), httpOnly: false, ...attrs },
      { name: "__clerk_db_jwt", value: "fixture-dev-browser", httpOnly: false, ...attrs },
    ]);

    const page = await context.newPage();
    page.setDefaultTimeout(timeoutMs);
    await page.goto(opts.loginUrl, { waitUntil: "load", timeout: timeoutMs });
    await page.waitForSelector(APPROVE_SELECTOR, { state: "visible", timeout: timeoutMs });
    await page.click(APPROVE_SELECTOR);
    // Wait for the page to acknowledge the click — either a redirect or a
    // confirmation marker. We don't pin the destination URL; the CLI's
    // status poll is the authoritative ack of approval.
    await page.waitForLoadState("networkidle", { timeout: timeoutMs }).catch(() => {});
  } finally {
    await browser.close().catch(() => {});
  }
}
