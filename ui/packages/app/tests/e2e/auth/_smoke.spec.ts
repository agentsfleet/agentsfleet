/**
 * WS-A.1 skeleton smoke — proves the authenticated e2e wire is alive.
 *
 * Asserts:
 *   1. globalSetup ran (fail-fast didn't throw — implicit by reaching this test).
 *   2. The dashboard's /sign-in renders against whatever target the workflow
 *      pointed NEXT_PUBLIC_API_URL at.
 *
 * This is a placeholder. The real signed-in smoke (assert dashboard renders
 * fixture-user email in header) lands in WS-A.2 once signInAs is wired.
 */
import { expect, test } from "@playwright/test";

test.describe("auth e2e wire", () => {
  test("dashboard /sign-in renders", async ({ page }) => {
    await page.goto("/sign-in");
    const heading = page.getByRole("heading", { level: 1, name: /sign in/i });
    await expect(heading).toBeVisible();
  });

  test("required Clerk credentials present in env", () => {
    expect(process.env.NEXT_PUBLIC_API_URL?.length ?? 0).toBeGreaterThan(0);
    expect(process.env.CLERK_SECRET_KEY?.length ?? 0).toBeGreaterThan(20);
    expect(process.env.CLERK_WEBHOOK_SECRET?.length ?? 0).toBeGreaterThan(20);
  });
});
