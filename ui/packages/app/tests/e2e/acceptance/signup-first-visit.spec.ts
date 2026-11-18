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
 * signed replay, so either the setup screen or the workspace is a correct
 * first page. The replay makes the order deterministic when Clerk is slow.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
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
/** Under `AccountSetup`'s 90 s give-up, so a stall fails here first. */
const OPENED_TIMEOUT_MS = 60_000;

function uniqueEmail(): string {
  const tag = crypto.randomBytes(4).toString("hex");
  return `first-visit-${tag}+clerk_test@e2e.agentsfleet.net`;
}

const isProdApi = (process.env.NEXT_PUBLIC_API_URL ?? "").includes("api.agentsfleet.net");

test.describe("signup first visit", () => {
  test.skip(isProdApi, "Clerk test signups are development-only");
  test.setTimeout(120_000);

  let createdEmail: string | null = null;

  test.afterEach(async () => {
    if (!createdEmail) return;
    const userId = await findUserIdByEmail(createdEmail).catch(() => null);
    if (userId) await deleteUser(userId).catch((err: unknown) => {
        // Loud, not silent: a swallowed failure here is how users leak. The
        // global-teardown sweep is the backstop for anything that slips.
        console.error(`[e2e] fixture user cleanup failed for ${userId}:`, err);
      });
    createdEmail = null;
  });

  test("a dashboard opened before the account exists waits on setup, then lands in the workspace", {tag: "@post-deploy"}, async ({ page }) => {
    const email = uniqueEmail();
    createdEmail = email;
    const { sessionId, clerkUserId } = await createClerkAccount(page, email, PASSWORD);
    await activateSession(page, sessionId);

    // The person's first request, before this test has delivered `user.created`.
    await page.goto("/");
    let sawErrorPage = (await page.getByText(ERROR_PAGE).count()) > 0;
    const firstPage = (await page.getByText(SETUP_PAGE).count()) > 0 ? "setup" : new URL(page.url()).pathname;

    // The delivery the daemon may still be waiting for; a replay when Clerk's came first.
    await bootstrapTenant({ key: FIXTURE_KEY.regular, email, password: PASSWORD, clerkUserId });

    await expect
      .poll(async () => {
        if ((await page.getByText(ERROR_PAGE).count()) > 0) sawErrorPage = true;
        return new URL(page.url()).pathname;
      }, { timeout: OPENED_TIMEOUT_MS, intervals: [250] })
      .toMatch(WORKSPACE_PATH);
    expect(sawErrorPage, `the error page showed while the account opened (first page: ${firstPage})`).toBe(false);
  });
});
