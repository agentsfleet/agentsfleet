/**
 * auth-routing.spec.ts — public auth pages and protected dashboard routes.
 *
 * The suite already proves a signed-in fixture can render dashboard pages.
 * This spec covers the route-boundary behavior around that session: signed-in
 * users can visit Clerk public auth pages without losing the session, and
 * signed-out users are bounced from protected app routes to /sign-in.
 */
import { expect, test } from "@playwright/test";
import { clerk } from "@clerk/testing/playwright";
import { signInAs } from "./fixtures/auth";
import { gotoWorkspace, workspaceUrlPattern } from "./fixtures/nav";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";

const ROUTING_TEST_TIMEOUT_MS = 90_000;
const ROUTING_TIMEOUT_MS = 30_000;
const PUBLIC_AUTH_ROUTES = ["/sign-in", "/sign-up"] as const;
// Post-M118 the workspace pages live under `/w/<id>/…`. clerkMiddleware gates
// auth before routing, so a signed-out visit to any workspace-scoped path (a
// synthetic id is fine — the auth bounce fires before the ownership guard) and
// to the tenant-scoped root/billing routes all redirect to /sign-in.
const PROTECTED_ROUTES = [
  "/",
  "/w/ws_probe/fleets",
  "/w/ws_probe/events",
  "/settings/billing",
  "/settings/account",
  "/settings/account/security",
] as const;

test.describe("auth routing", () => {
  test.setTimeout(ROUTING_TEST_TIMEOUT_MS);

  test("bare settings and workspace URLs return 404 while access notices remain available", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    for (const route of ["/settings", `/w/${workspace}`]) {
      const response = await page.goto(route);
      expect(response?.status()).toBe(404);
      expect(new URL(page.url()).pathname).toBe(route);
      await expect(page.getByRole("heading", { name: "Page not found", exact: true })).toBeVisible();
    }
    const notice = await page.goto("/settings?notice=models-platform-admin-only");
    expect(notice?.status()).toBe(200);
    await expect(page.getByRole("heading", { name: "Access restricted", exact: true })).toBeVisible();
  });

  test("signed-in user can visit auth pages without losing dashboard access", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);

    for (const route of PUBLIC_AUTH_ROUTES) {
      await page.goto(route, { waitUntil: "domcontentloaded", timeout: ROUTING_TIMEOUT_MS });
      await expect(page).toHaveURL(new RegExp(`${route}(\\?|$|/)`), {
        timeout: ROUTING_TIMEOUT_MS,
      });
    }

    await gotoWorkspace(page, FIXTURE_KEY.regular, "events");
    await expect(page).toHaveURL(workspaceUrlPattern("events"), { timeout: ROUTING_TIMEOUT_MS });
    await expect(page.getByRole("heading", { name: /^events$/i })).toBeVisible();
  });

  test("signed-out user is redirected from protected routes to sign-in", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets");
    await clerk.signOut({ page });

    for (const route of PROTECTED_ROUTES) {
      await page.goto(route, { waitUntil: "domcontentloaded", timeout: ROUTING_TIMEOUT_MS });
      await expect(page).toHaveURL(/\/sign-in(\?|$|\/)/, { timeout: ROUTING_TIMEOUT_MS });
    }
  });
});
