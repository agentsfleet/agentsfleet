import { test, expect } from "@playwright/test";

// Unauthenticated smoke for M27 routes. Authenticated kill-switch flow requires
// a Clerk test user + a seeded zombie in api-dev; deferred until that harness
// lands (see follow-up spec). These assertions confirm each route loads and
// either renders content or redirects to sign-in — no third-party hosts.

const EXPECTED_HOSTNAME = new URL(
  process.env.BASE_URL ?? "http://localhost:3000",
).hostname;

const M27_ROUTES = ["/", "/zombies", "/firewall", "/credentials", "/settings"];

test.describe("M27 dashboard routes — unauthenticated smoke", () => {
  for (const route of M27_ROUTES) {
    test(`${route} stays on first-party host`, async ({ page }) => {
      await page.goto(route);
      await page.waitForLoadState("domcontentloaded");
      const url = new URL(page.url());
      expect(url.hostname).toBe(EXPECTED_HOSTNAME);
      expect(page.url()).not.toContain("accounts.dev");
    });
  }

  test("mobile viewport renders without third-party redirect", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await page.goto("/");
    await page.waitForLoadState("domcontentloaded");
    const url = new URL(page.url());
    expect(url.hostname).toBe(EXPECTED_HOSTNAME);
    expect(page.url()).not.toContain("accounts.dev");
    const pathname = url.pathname;
    const okPath = pathname === "/" || pathname.startsWith("/sign-in");
    expect(okPath).toBe(true);
  });
});
