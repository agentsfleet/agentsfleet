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

  test("pricing page loads", async ({ page }) => {
    await page.goto("/pricing");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Free and Scale plans");
  });

  test("agents page loads", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("autonomous agents");
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
    await expect(nav.getByRole("link", { name: "Home" })).toBeVisible();
    await expect(nav.getByRole("link", { name: "Pricing" })).toBeVisible();
    await expect(nav.getByRole("link", { name: "Docs" })).toBeVisible();
  });

  test("footer renders on all routes", async ({ page }) => {
    for (const route of ["/", "/pricing", "/agents", "/privacy", "/terms"]) {
      await page.goto(route);
      await expect(page.getByRole("contentinfo")).toBeVisible();
    }
  });

  test("Discord link uses canonical URL", async ({ page }) => {
    await page.goto("/");
    const discord = page.getByRole("contentinfo").getByRole("link", { name: "Discord" });
    await expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
  });
});
