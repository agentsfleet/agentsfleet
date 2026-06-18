/**
 * Playwright Chromium wrapper for §5's CLI-auth handshake.
 *
 * Used only by `lifecycle-after-login.spec.ts`. We deliberately avoid
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
 *
 * After approve, the page renders the 6-digit verification code in an
 * `<output aria-label="Verification code">` element. We scrape it and
 * return it so the caller can type it into the CLI's pty prompt — the code
 * is the binding between the human who approved in the browser and the
 * human (here, the harness) typing into the terminal.
 */

const APPROVE_SELECTOR = "[data-testid=\"cli-auth-approve\"]";
const VERIFICATION_CODE_LABEL = "Verification code";
const VERIFICATION_CODE_RE = /^\d{6}$/;
const DEFAULT_TIMEOUT_MS = 30_000;

interface CookieAttrs {
  readonly domain: string;
  readonly path: "/";
  readonly sameSite: "Lax";
  readonly secure: boolean;
}

function decodeJwtIat(jwt: string): number {
  const payload = jwt.split(".")[1];
  if (!payload) throw new Error("malformed cookieJwt (no payload segment)");
  const json = JSON.parse(Buffer.from(payload, "base64url").toString("utf8")) as { iat?: unknown };
  if (typeof json.iat !== "number") throw new Error("cookieJwt missing iat claim");
  return json.iat;
}

function cookieAttrs(loginUrl: string): CookieAttrs {
  const url = new URL(loginUrl);
  return {
    domain: url.hostname,
    path: "/",
    sameSite: "Lax",
    secure: url.protocol === "https:",
  };
}

export interface CliAuthHandoffOptions {
  readonly loginUrl: string;
  readonly cookieJwt: string;
  readonly timeoutMs?: number;
}

/**
 * Drive a Playwright Chromium context through the CLI-auth approve action
 * and return the 6-digit verification code the page displays on success.
 */
export async function completeCliAuthHandoff(opts: CliAuthHandoffOptions): Promise<string> {
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

    // On success the page swaps to the "Type this code into your CLI" card.
    // Read the code from its accessible <output> rather than pinning a URL —
    // the CLI's /verify call is the authoritative ack of approval.
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
