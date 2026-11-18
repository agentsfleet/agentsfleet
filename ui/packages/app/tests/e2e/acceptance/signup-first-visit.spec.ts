/**
 * The dashboard a person opens the instant signup finishes.
 *
 * Every other signup here waits for the tenant to reach Clerk before opening
 * the dashboard (`signUpAs`), which skips the seconds a real person meets: the
 * account exists in Clerk, the daemon's `user.created` handler has not yet
 * written the tenant back, and the session token carries none. In that window
 * the entry page must wait on account setup and then land in the new
 * workspace; its error page must never show.
 *
 * Clerk's own delivery may open the account before this test delivers its
 * signed replay, so the workspace is also a correct first page. When the
 * replay is what opens the account, no tenant existed at the first visit, and
 * that visit must have waited on setup.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { ACCOUNT_SETUP_GIVE_UP_MS } from "@/app/(dashboard)/AccountSetup";
import { bootstrapTenant } from "./fixtures/bootstrap";
import { deleteUser, findUserIdByEmail } from "./fixtures/clerk-admin";
import { FIXTURE_KEY } from "./fixtures/constants";
import { activateSession, createClerkAccount } from "./fixtures/signup";

const PASSWORD = "SignupFixture!2026-stable";
/** `(dashboard)/error.tsx`'s title. */
const ERROR_PAGE = "Something went wrong";
/** `(dashboard)/AccountSetup.tsx`'s title. */
const SETUP_PAGE = "Setting up your workspace";
const WORKSPACE_PATH = /^\/w\/[0-9a-f-]{36}\//;
/** Two thirds of `AccountSetup`'s own give-up, so a stall fails here before the page gives up. */
const OPENED_TIMEOUT_MS = (ACCOUNT_SETUP_GIVE_UP_MS * 2) / 3;
/** Signup and the signed delivery get a minute on top of the wait for the workspace. */
const TEST_TIMEOUT_MS = OPENED_TIMEOUT_MS + 60_000;
const PROD_API_HOST = "api.agentsfleet.net";

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  // `first-visit-` is in the global-teardown sweep's prefixes, so a crashed run still gets reaped.
  return `first-visit-${tag}+clerk_test@e2e.agentsfleet.net`;
}

const apiUrl = process.env.NEXT_PUBLIC_API_URL;
const isProdApi = apiUrl ? new URL(apiUrl).hostname === PROD_API_HOST : false;

test.describe("signup first visit", () => {
  test.skip(isProdApi, "Clerk test signups are development-only");
  test.setTimeout(TEST_TIMEOUT_MS);

  let createdEmail: string | null = null;

  test.afterEach(async () => {
    if (!createdEmail) return;
    const email = createdEmail;
    createdEmail = null;
    // A failed lookup throws rather than reading as "no user": a silent miss
    // here is how users leak, and the global-teardown sweep is only the backstop.
    const userId = await findUserIdByEmail(email);
    if (userId) await deleteUser(userId).catch((err: unknown) => {
        console.error(`[e2e] fixture user cleanup failed for ${userId}:`, err);
      });
  });

  test("a dashboard opened before signup's writeback reaches the workspace, waiting on setup and never on the error page", {tag: "@post-deploy"}, async ({ page }) => {
    const email = uniqueEmail();
    createdEmail = email;
    const { sessionId, clerkUserId } = await createClerkAccount(page, email, PASSWORD);
    await activateSession(page, sessionId);

    // The person's first request, before this test has delivered `user.created`.
    await page.goto("/");
    let sawErrorPage = (await page.getByText(ERROR_PAGE).count()) > 0;
    const firstPage = (await page.getByText(SETUP_PAGE).count()) > 0 ? "setup" : new URL(page.url()).pathname;

    // The delivery the daemon may still be waiting for; a replay when Clerk's came first.
    const delivery = await bootstrapTenant({ key: FIXTURE_KEY.regular, email, password: PASSWORD, clerkUserId });

    await expect
      .poll(async () => {
        if ((await page.getByText(ERROR_PAGE).count()) > 0) sawErrorPage = true;
        return new URL(page.url()).pathname;
      }, { timeout: OPENED_TIMEOUT_MS, intervals: [250] })
      .toMatch(WORKSPACE_PATH);
    expect(sawErrorPage, `the error page showed while the account opened (first page: ${firstPage})`).toBe(false);
    if (delivery.created) {
      expect(firstPage, "this test's delivery opened the account, so the first visit came before any tenant").toBe("setup");
    }
  });
});
