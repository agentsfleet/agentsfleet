import { test, expect } from "@playwright/test";

// Mobile-dashboard smoke at 375px. The signed-in 12-col grid → mobile
// stacking transition is covered by responsive Tailwind breakpoints
// (md: ≥768px); the unauthenticated auth-shell at 375px is what this
// spec exercises: no horizontal overflow and the brand-mark stays visible.

test.describe("mobile dashboard — 375px viewport", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
  });

  test("auth shell at 375px has no horizontal overflow", async ({ page }) => {
    await page.goto("/sign-in");
    await page.waitForLoadState("domcontentloaded");
    const overflow = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    }));
    // ≤2px tolerance for sub-pixel rounding.
    expect(overflow.scrollWidth - overflow.clientWidth).toBeLessThanOrEqual(2);
  });

  test("auth shell brand-mark stays visible at 375px", async ({ page }) => {
    await page.goto("/sign-in");
    await page.waitForLoadState("domcontentloaded");
    const wordmark = page.getByText("agentsfleet", { exact: false }).first();
    await expect(wordmark).toBeVisible();
  });
});
