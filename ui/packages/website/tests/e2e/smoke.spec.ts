import { test, expect } from "@playwright/test";

/**
 * Smoke tests — fast subset for CI pre-deploy gate.
 * Run with: bunx playwright test tests/e2e/smoke.spec.ts
 * For post-deploy Vercel: BASE_URL=https://usezombie.com bunx playwright test tests/e2e/smoke.spec.ts
 */
test.describe("Smoke", () => {
  test("home page loads", async ({ page }) => {
    await page.goto("/");
    await expect(page).toHaveTitle(/usezombie/i);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  });

  test("pricing section renders inline on home", async ({ page }) => {
    await page.goto("/#pricing");
    await expect(page.getByTestId("pricing-block")).toBeVisible();
    await expect(page.getByTestId("pricing-rate-event")).toHaveText("free");
    await expect(page.getByTestId("pricing-rate-run")).toHaveText("$0.0001/sec");
    await expect(page.getByTestId("pricing-rate-run-hourly")).toHaveText("≈ $0.36/hr");
  });

  test("agents page loads", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for autonomous agents.",
    );
  });

  test("privacy page loads", async ({ page }) => {
    await page.goto("/privacy");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Privacy Policy");
  });

  test("terms page loads", async ({ page }) => {
    await page.goto("/terms");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Terms of Service");
  });

  test("nav links are present on home", async ({ page }) => {
    await page.goto("/");
    const nav = page.getByRole("navigation", { name: /primary/i });
    await expect(nav.getByRole("link", { name: /^home$/i })).toBeVisible();
    await expect(nav.getByRole("link", { name: /^pricing$/i })).toBeVisible();
    await expect(nav.getByRole("link", { name: /^docs$/i })).toBeVisible();
  });

  test("footer renders on all routes", async ({ page }) => {
    for (const route of ["/", "/agents", "/privacy", "/terms"]) {
      await page.goto(route);
      await page.waitForLoadState("domcontentloaded");
      await expect(page.getByRole("contentinfo")).toBeVisible({ timeout: 10_000 });
    }
  });

  test("Discord link uses canonical URL", async ({ page }) => {
    await page.goto("/");
    const discord = page.getByRole("contentinfo").getByRole("link", { name: /^discord$/i });
    await expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
  });
});
