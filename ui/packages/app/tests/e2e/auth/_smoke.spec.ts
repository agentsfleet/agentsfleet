/**
 * WS-A.1 skeleton smoke — proves the authenticated e2e wire is alive.
 *
 * Asserts:
 *   1. globalSetup ran (env-guard didn't throw — implicit by reaching this test).
 *   2. The dashboard's /sign-in renders against the local Next.js dev server
 *      configured to talk to api-dev.
 *
 * This is a placeholder. The real signed-in smoke (assert dashboard renders
 * fixture-user email in header) lands in WS-A.2 once signInAs is wired.
 */
import { expect, test } from "@playwright/test";

test.describe("auth e2e wire", () => {
  test("dashboard /sign-in renders against api-dev-pointed dev server", async ({ page }) => {
    await page.goto("/sign-in");
    const heading = page.getByRole("heading", { level: 1, name: /sign in/i });
    await expect(heading).toBeVisible();
  });

  test("globalSetup confirmed both Clerk credentials and api-dev allow-list", () => {
    expect(process.env.NEXT_PUBLIC_API_URL).toBe("https://api-dev.usezombie.com");
    expect(process.env.CLERK_SECRET_KEY?.length ?? 0).toBeGreaterThan(20);
    expect(process.env.CLERK_WEBHOOK_SECRET?.length ?? 0).toBeGreaterThan(20);
  });
});
