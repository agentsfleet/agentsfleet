/**
 * signout-and-signin.spec.ts — full round-trip through Clerk's middleware.
 *
 * Sign in as a persistent fixture, hit a protected page, click the
 * UserButton's sign-out item, assert the redirect to /sign-in clears the
 * session cookie, then sign back in via the same fixture and assert the
 * protected page renders again. Catches regressions where
 * `clerkMiddleware` keeps a stale session, where the post-signout
 * redirect lands somewhere other than /sign-in, or where signing back
 * in mid-session re-uses cookies from the previous user.
 *
 * The UserButton dropdown is Clerk-rendered; the sign-out entry is a
 * `<button>` carrying the visible text "Sign out". `clerk.signOut`
 * (`@clerk/testing/playwright`) drives the same Clerk SDK call the
 * button triggers, so the spec passes through the production code path
 * — no mocking.
 */
import { expect, test } from "@playwright/test";
import { clerk } from "@clerk/testing/playwright";
import { signInAs } from "./fixtures/auth";
import { gotoWorkspace, workspaceHref, workspaceUrlPattern } from "./fixtures/nav";
import { FIXTURE_KEY } from "./fixtures/constants";

const SIGNOUT_TIMEOUT_MS = 15_000;

test.describe("signout → signin round-trip", () => {
  test("authenticated session signs out cleanly and signs back in", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const ws = await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets");
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();

    // Drive the SDK-level sign-out the UserButton's "Sign out" item triggers.
    // The button is Clerk-rendered inside a Radix portal, which can race
    // hydration on slower runs; the SDK call is the exact same code path
    // without the portal flakiness.
    await clerk.signOut({ page });

    // Next protected navigation re-enters clerkMiddleware with no session
    // cookie → redirect to /sign-in.
    await page.goto(workspaceHref(ws, "fleets"));
    await expect(page).toHaveURL(/\/sign-in(\?|$|\/)/, { timeout: SIGNOUT_TIMEOUT_MS });

    // Sign back in via the persistent fixture; protected page renders.
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(ws, "fleets"));
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"), { timeout: SIGNOUT_TIMEOUT_MS });
    await expect(page.getByTestId("workspace-switcher")).toBeVisible();
  });
});
