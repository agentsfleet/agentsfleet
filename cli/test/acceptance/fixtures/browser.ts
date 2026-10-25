/**
 * Playwright Chromium wrapper for the CLI-auth handshake.
 *
 * Establishes a real Clerk session from a one-time sign-in ticket (the same
 * mechanism the dashboard acceptance suite's `signInAs` uses), then drives
 * the `/cli-auth/{session_id}` approve action and returns the 6-digit
 * verification code the page displays.
 *
 * Why a ticket and not a manual cookie-mount: a Backend-API-minted
 * `__session` token lacks the `azp` claim `clerkMiddleware` now requires, so
 * a hand-mounted cookie is bounced to `/sign-in` on the first protected
 * navigation (it also zeroes `__client_uat`). Clerk’s browser client mints the cookies the
 * middleware was built to consume, so the approve page actually
 * authenticates. Requires `CLERK_PUBLISHABLE_KEY` + `CLERK_SECRET_KEY` in
 * env (resolved by global-setup); `setupClerkTestingToken` bypasses the dev
 * bot-protection on the sign-in form.
 *
 * Selector: authorize button by accessible role (`button` named /Authorize/i).
 * Code: scraped from the `<output aria-label="Verification code">` the page
 * renders on success — the CLI's /verify call is the authoritative ack.
 */

import {
  createSignInTicket,
  withClientSessionSweepOnFailure,
  withSessionRevocation,
} from "./clerk-admin.ts";
import { CLERK_SECRET_KEY_ENV } from "./constants.ts";
import { vercelBypassState } from "./vercel-bypass.ts";
import type { Page } from "playwright";

const APPROVE_BUTTON_NAME = /Authorize/i;
const VERIFICATION_CODE_LABEL = "Verification code";
const VERIFICATION_CODE_RE = /^\d{6}$/;
const SIGN_IN_PATH = "/sign-in";
const DEFAULT_TIMEOUT_MS = 30_000;

export interface CliAuthHandoffOptions {
  readonly loginUrl: string;
  readonly clerkUserId: string;
  readonly timeoutMs?: number;
}

interface BrowserClerk {
  readonly client?: {
    readonly id: string;
    readonly signIn: {
      create(input: { strategy: string; ticket: string }): Promise<{
        readonly status: string;
        readonly createdSessionId?: string | null;
      }>;
    };
  };
  setActive(input: {
    session: string;
    navigate: () => Promise<void>;
  }): Promise<void>;
}

async function signInFixture(
  page: Page, origin: string, clerkSecret: string, clerkUserId: string, timeoutMs: number,
): Promise<string> {
  const { setupClerkTestingToken } = await import("@clerk/testing/playwright");
  await setupClerkTestingToken({ page });
  await page.goto(`${origin}${SIGN_IN_PATH}`, { waitUntil: "load", timeout: timeoutMs });
  await page.waitForFunction(() =>
    Boolean((globalThis as unknown as { Clerk?: BrowserClerk }).Clerk?.client)
  );
  const browserClientId = await page.evaluate(() => {
    const id = (globalThis as unknown as { Clerk?: BrowserClerk }).Clerk?.client?.id;
    if (!id) throw new Error("Clerk client id unavailable during fixture sign-in");
    return id;
  });
  const ticket = await createSignInTicket(clerkSecret, clerkUserId);
  return withClientSessionSweepOnFailure(clerkSecret, browserClientId, () =>
    page.evaluate(async (signInTicket) => {
      const clerk = (globalThis as unknown as { Clerk?: BrowserClerk }).Clerk;
      if (!clerk?.client) throw new Error("Clerk client unavailable during fixture sign-in");
      const attempt = await clerk.client.signIn.create({ strategy: "ticket", ticket: signInTicket });
      if (attempt.status !== "complete" || !attempt.createdSessionId) {
        throw new Error(`fixture ticket sign-in did not complete (${attempt.status})`);
      }
      return attempt.createdSessionId;
    }, ticket)
  );
}

async function approveLogin(page: Page, opts: CliAuthHandoffOptions, sessionId: string): Promise<string> {
  await page.evaluate(async (id) => {
    const clerk = (globalThis as unknown as { Clerk?: BrowserClerk }).Clerk;
    if (!clerk) throw new Error("Clerk client unavailable during fixture activation");
    await clerk.setActive({ session: id, navigate: async () => {} });
  }, sessionId);
  await page.waitForFunction(() =>
    Boolean((globalThis as unknown as { Clerk?: { user?: unknown } }).Clerk?.user)
  );
  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  await page.goto(opts.loginUrl, { waitUntil: "load", timeout: timeoutMs });
  const approve = page.getByRole("button", { name: APPROVE_BUTTON_NAME });
  await approve.waitFor({ state: "visible", timeout: timeoutMs });
  await approve.click();
  const codeOutput = page.getByLabel(VERIFICATION_CODE_LABEL);
  await codeOutput.waitFor({ state: "visible", timeout: timeoutMs });
  const code = ((await codeOutput.textContent()) ?? "").trim();
  if (!VERIFICATION_CODE_RE.test(code)) {
    throw new Error("completeCliAuthHandoff: expected a 6-digit verification code");
  }
  return code;
}

export async function completeCliAuthHandoff(opts: CliAuthHandoffOptions): Promise<string> {
  if (!opts?.loginUrl) throw new Error("completeCliAuthHandoff: loginUrl required");
  if (!opts?.clerkUserId) throw new Error("completeCliAuthHandoff: clerkUserId required");
  const clerkSecret = process.env[CLERK_SECRET_KEY_ENV];
  if (!clerkSecret) throw new Error(`completeCliAuthHandoff: ${CLERK_SECRET_KEY_ENV} required`);
  const { chromium } = await import("playwright");
  const { clerkSetup } = await import("@clerk/testing/playwright");
  await clerkSetup();
  const origin = new URL(opts.loginUrl).origin;
  const storageState = await vercelBypassState(origin, process.env.VERCEL_BYPASS_SECRET);
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext({ storageState });
    const page = await context.newPage();
    const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    page.setDefaultTimeout(timeoutMs);
    const sessionId = await signInFixture(page, origin, clerkSecret, opts.clerkUserId, timeoutMs);
    return await withSessionRevocation(clerkSecret, sessionId, () => approveLogin(page, opts, sessionId));
  } finally {
    await browser.close().catch(() => {});
  }
}
