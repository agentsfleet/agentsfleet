import { test, expect } from "@playwright/test";

// Unauthenticated smoke for the dashboard routes: each loads and either renders
// content or redirects to sign-in — no third-party hosts. The root route is
// covered by smoke.spec.ts and auth-theme.spec.ts.

const EXPECTED_HOSTNAME = new URL(
  process.env.BASE_URL ?? "http://localhost:3000",
).hostname;

// Workspace-scoped routes now live under `/w/<id>/…`; a synthetic id suffices
// here because these are unauthenticated probes — clerkMiddleware bounces to
// /sign-in before the ownership guard runs. `/settings` stays at root.
const DASHBOARD_ROUTES = ["/w/ws_probe/fleets", "/w/ws_probe/secrets", "/settings"];

test.describe("dashboard routes — unauthenticated smoke", () => {
  for (const route of DASHBOARD_ROUTES) {
    test(`${route} stays on first-party host`, async ({ page }) => {
      await page.goto(route);
      await page.waitForLoadState("domcontentloaded");
      const url = new URL(page.url());
      expect(url.hostname).toBe(EXPECTED_HOSTNAME);
      expect(page.url()).not.toContain("accounts.dev");
    });
  }
});
