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
import { FIXTURE_KEY } from "./fixtures/constants";

const ROUTING_TIMEOUT_MS = 15_000;
const PUBLIC_AUTH_ROUTES = ["/sign-in", "/sign-up"] as const;
const PROTECTED_ROUTES = ["/", "/zombies", "/events", "/settings/billing"] as const;

test.describe("auth routing", () => {
  test("signed-in user can visit auth pages without losing dashboard access", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);

    for (const route of PUBLIC_AUTH_ROUTES) {
      await page.goto(route);
      await expect(page).toHaveURL(new RegExp(`${route}(\\?|$|/)`), {
        timeout: ROUTING_TIMEOUT_MS,
      });
    }

    await page.goto("/events");
    await expect(page).toHaveURL(/\/events(\?|$)/, { timeout: ROUTING_TIMEOUT_MS });
    await expect(page.getByRole("heading", { name: /^events$/i })).toBeVisible();
  });

  test("signed-out user is redirected from protected routes to sign-in", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await clerk.signOut({ page });

    for (const route of PROTECTED_ROUTES) {
      await page.goto(route);
      await expect(page).toHaveURL(/\/sign-in(\?|$|\/)/, { timeout: ROUTING_TIMEOUT_MS });
    }
  });
});
