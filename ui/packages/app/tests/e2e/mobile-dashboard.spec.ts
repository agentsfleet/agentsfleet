import { test, expect } from "@playwright/test";

// Mobile-dashboard smoke at 375px. The signed-in 12-col grid → mobile
// stacking transition is covered by responsive Tailwind breakpoints
// (md: ≥768px); the unauthenticated auth-shell at 375px is what this
// spec exercises: no horizontal overflow, the brand-mark stays visible,
// and the page does not redirect off-host on small viewports.

test.describe("mobile dashboard — 375px viewport", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
  });

  test("/ redirects into a renderable auth shell (no off-host redirect)", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("domcontentloaded");
    const expectedHost = new URL(process.env.BASE_URL ?? "http://localhost:3000").hostname;
    const url = new URL(page.url());
    expect(url.hostname).toBe(expectedHost);
    // Either at / or on /sign-in (Clerk catch-all). Both render the
    // auth shell (or the public dashboard skeleton).
    const okPath = url.pathname === "/" || url.pathname.startsWith("/sign-in");
    expect(okPath).toBe(true);
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
    const wordmark = page.getByText("usezombie", { exact: false }).first();
    await expect(wordmark).toBeVisible();
  });
});
