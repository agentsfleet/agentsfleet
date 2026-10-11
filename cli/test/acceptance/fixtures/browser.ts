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
 * Selector: approve button by accessible role (`button` named /approve/i).
 * Code: scraped from the `<output aria-label="Verification code">` the page
 * renders on success — the CLI's /verify call is the authoritative ack.
 */

import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  createSignInTicket,
  withClientSessionSweepOnFailure,
  withSessionRevocation,
} from "./clerk-admin.ts";
import { CLERK_SECRET_KEY_ENV } from "./constants.ts";
import { vercelBypassState } from "./vercel-bypass.ts";
import type { Page } from "playwright";

const APPROVE_BUTTON_NAME = /approve/i;
const VERIFICATION_CODE_LABEL = "Verification code";
const VERIFICATION_CODE_RE = /^\d{6}$/;
const SIGN_IN_PATH = "/sign-in";
const DEFAULT_TIMEOUT_MS = 30_000;
const NODE_BIN = "node";
const NODE_STRIP_TYPES_FLAG = "--experimental-strip-types";

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

async function runBrowserHandoff(opts: CliAuthHandoffOptions): Promise<string> {
  if (!opts?.loginUrl) throw new Error("completeCliAuthHandoff: loginUrl required");
  if (!opts?.clerkUserId) throw new Error("completeCliAuthHandoff: clerkUserId required");
  const clerkSecret = process.env[CLERK_SECRET_KEY_ENV];
  if (!clerkSecret) throw new Error(`completeCliAuthHandoff: ${CLERK_SECRET_KEY_ENV} required`);
  // Node hosts the Playwright transport; the parent Bun process owns orchestration.
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

/**
 * Run the Playwright transport under Node while Bun retains test orchestration.
 * Playwright's direct Bun transport cannot parse Clerk's relative response URL.
 */
export async function completeCliAuthHandoff(opts: CliAuthHandoffOptions): Promise<string> {
  const child = Bun.spawn([
    NODE_BIN,
    NODE_STRIP_TYPES_FLAG,
    fileURLToPath(import.meta.url),
  ], {
    env: process.env,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  child.stdin.write(JSON.stringify(opts));
  child.stdin.end();
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (exitCode !== 0) {
    throw new Error(`browser handoff exited ${exitCode}: ${stderr.trim()}`);
  }
  const code = stdout.trim().split("\n").at(-1)?.trim() ?? "";
  if (!VERIFICATION_CODE_RE.test(code)) {
    throw new Error(`browser handoff returned invalid code: ${JSON.stringify(code)}`);
  }
  return code;
}

const isNodeEntrypoint = process.argv[1] !== undefined
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isNodeEntrypoint) {
  try {
    let input = "";
    for await (const chunk of process.stdin) input += String(chunk);
    const opts = JSON.parse(input) as CliAuthHandoffOptions;
    process.stdout.write(await runBrowserHandoff(opts));
  } catch (error: unknown) {
    process.stderr.write(`${error instanceof Error ? error.stack ?? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
